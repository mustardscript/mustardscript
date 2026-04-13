use super::*;

pub(in crate::runtime) fn validate_snapshot(snapshot: &ExecutionSnapshot) -> MustardResult<()> {
    let runtime = &snapshot.runtime;
    validate_bytecode_program(&runtime.program)?;
    if runtime.envs.get(runtime.globals).is_none() {
        return Err(snapshot_error("missing globals environment"));
    }
    if runtime.frames.len() > runtime.limits.call_depth_limit {
        return Err(limit_error("call depth limit exceeded"));
    }
    if runtime.frames.is_empty()
        && runtime.suspended_host_call.is_none()
        && runtime.root_result.is_none()
        && runtime.microtasks.is_empty()
    {
        return Err(snapshot_error(
            "suspended runtime has no frames or async state",
        ));
    }

    validate_envs(runtime)?;
    validate_closures(runtime)?;
    validate_builtin_function_objects(runtime)?;
    for frame in &runtime.frames {
        validate_frame(runtime, frame)?;
        walk::walk_frame_values(frame, &mut |value| validate_runtime_value(runtime, value))?;
    }
    walk::walk_heap_values(runtime, &mut |value| validate_runtime_value(runtime, value))?;
    validate_iterators(runtime)?;
    if let Some(root_result) = &runtime.root_result {
        validate_runtime_value(runtime, root_result)?;
    }
    for request in &runtime.pending_host_calls {
        validate_pending_host_call_snapshot(runtime, request)?;
    }
    if let Some(request) = &runtime.suspended_host_call {
        validate_pending_host_call_snapshot(runtime, request)?;
    }
    for (promise_key, promise) in &runtime.promises {
        validate_promise_snapshot(runtime, promise_key, promise)?;
        walk::walk_promise_values(promise, &mut |value| validate_runtime_value(runtime, value))?;
        for continuation in &promise.awaiters {
            walk::walk_continuation_frames(continuation, &mut |frame| {
                validate_frame(runtime, frame)?;
                walk::walk_frame_values(frame, &mut |value| validate_runtime_value(runtime, value))
            })?;
        }
    }
    for microtask in &runtime.microtasks {
        validate_microtask_snapshot(runtime, microtask)?;
        walk::walk_microtask_values(
            microtask,
            &mut |frame| {
                validate_frame(runtime, frame)?;
                walk::walk_frame_values(frame, &mut |value| validate_runtime_value(runtime, value))
            },
            &mut |value| validate_runtime_value(runtime, value),
        )?;
    }
    if let Some(exception) = &runtime.pending_internal_exception {
        validate_runtime_value(runtime, &exception.value)?;
    }

    Ok(())
}

fn snapshot_error(message: impl Into<String>) -> MustardError {
    MustardError::Message {
        kind: DiagnosticKind::Serialization,
        message: format!("snapshot validation failed: {}", message.into()),
        span: None,
        traceback: Vec::new(),
    }
}

fn validate_envs(runtime: &Runtime) -> MustardResult<()> {
    for (env_key, env) in &runtime.envs {
        if let Some(parent) = env.parent
            && runtime.envs.get(parent).is_none()
        {
            return Err(snapshot_error(format!(
                "environment {:?} references missing parent {:?}",
                env_key, parent
            )));
        }
        for cell in env.bindings.values() {
            if runtime.cells.get(*cell).is_none() {
                return Err(snapshot_error(format!(
                    "environment {:?} references missing cell {:?}",
                    env_key, cell
                )));
            }
        }
    }
    Ok(())
}

fn validate_closures(runtime: &Runtime) -> MustardResult<()> {
    for (closure_key, closure) in &runtime.closures {
        if closure.function_id >= runtime.program.functions.len() {
            return Err(snapshot_error(format!(
                "closure {:?} references missing function {}",
                closure_key, closure.function_id
            )));
        }
        if runtime.envs.get(closure.env).is_none() {
            return Err(snapshot_error(format!(
                "closure {:?} references missing environment {:?}",
                closure_key, closure.env
            )));
        }
    }
    Ok(())
}

fn validate_builtin_function_objects(runtime: &Runtime) -> MustardResult<()> {
    for (function, object) in &runtime.builtin_prototypes {
        if runtime.objects.get(*object).is_none() {
            return Err(snapshot_error(format!(
                "builtin prototype {:?} references missing object {:?}",
                function, object
            )));
        }
    }
    for (function, object) in &runtime.builtin_function_objects {
        if runtime.objects.get(*object).is_none() {
            return Err(snapshot_error(format!(
                "builtin function object {:?} references missing object {:?}",
                function, object
            )));
        }
    }
    for (capability, object) in &runtime.host_function_objects {
        if runtime.objects.get(*object).is_none() {
            return Err(snapshot_error(format!(
                "host function object `{capability}` references missing object {:?}",
                object
            )));
        }
    }
    Ok(())
}

fn validate_frame(runtime: &Runtime, frame: &Frame) -> MustardResult<()> {
    let Some(function) = runtime.program.functions.get(frame.function_id) else {
        return Err(snapshot_error(format!(
            "frame references missing function {}",
            frame.function_id
        )));
    };
    if frame.ip >= function.code.len() {
        return Err(snapshot_error(format!(
            "frame instruction pointer {} is out of range for function {}",
            frame.ip, frame.function_id
        )));
    }
    if runtime.envs.get(frame.env).is_none() {
        return Err(snapshot_error(format!(
            "frame references missing environment {:?}",
            frame.env
        )));
    }
    for env in &frame.scope_stack {
        if runtime.envs.get(*env).is_none() {
            return Err(snapshot_error(format!(
                "frame scope stack references missing environment {:?}",
                env
            )));
        }
    }
    for handler in &frame.handlers {
        if let Some(catch) = handler.catch
            && catch >= function.code.len()
        {
            return Err(snapshot_error(format!(
                "frame handler catch target {} is out of range for function {}",
                catch, frame.function_id
            )));
        }
        if let Some(finally) = handler.finally
            && finally >= function.code.len()
        {
            return Err(snapshot_error(format!(
                "frame handler finally target {} is out of range for function {}",
                finally, frame.function_id
            )));
        }
        if runtime.envs.get(handler.env).is_none() {
            return Err(snapshot_error(format!(
                "frame handler references missing environment {:?}",
                handler.env
            )));
        }
        if handler.scope_stack_len > frame.scope_stack.len()
            || handler.stack_len > frame.stack.len()
        {
            return Err(snapshot_error(
                "frame handler restore state exceeds the current frame state",
            ));
        }
    }
    for completion in &frame.pending_completions {
        match completion {
            CompletionRecord::Jump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => {
                if *target >= function.code.len() {
                    return Err(snapshot_error(format!(
                        "pending jump target {} is out of range for function {}",
                        target, frame.function_id
                    )));
                }
                if *target_handler_depth > frame.handlers.len() {
                    return Err(snapshot_error(format!(
                        "pending jump targets handler depth {} but only {} handlers are active",
                        target_handler_depth,
                        frame.handlers.len()
                    )));
                }
                if *target_scope_depth > frame.scope_stack.len() {
                    return Err(snapshot_error(format!(
                        "pending jump targets scope depth {} but only {} scopes are active",
                        target_scope_depth,
                        frame.scope_stack.len()
                    )));
                }
            }
            CompletionRecord::Return(value) | CompletionRecord::Throw(value) => {
                validate_runtime_value(runtime, value)?;
            }
        }
    }
    for active in &frame.active_finally {
        if active.completion_index >= frame.pending_completions.len() {
            return Err(snapshot_error(
                "active finally references a missing completion",
            ));
        }
        if active.exit >= function.code.len() {
            return Err(snapshot_error(format!(
                "active finally exit target {} is out of range for function {}",
                active.exit, frame.function_id
            )));
        }
    }
    Ok(())
}

fn validate_iterators(runtime: &Runtime) -> MustardResult<()> {
    for iterator in runtime.iterators.values() {
        match iterator.state {
            IteratorState::Array(ref state)
            | IteratorState::ArrayKeys(ref state)
            | IteratorState::ArrayEntries(ref state) => {
                if runtime.arrays.get(state.array).is_none() {
                    return Err(snapshot_error(format!(
                        "iterator references missing array {:?}",
                        state.array
                    )));
                }
            }
            IteratorState::String(_) => {}
            IteratorState::MapEntries(ref state)
            | IteratorState::MapKeys(ref state)
            | IteratorState::MapValues(ref state) => {
                if runtime.maps.get(state.map).is_none() {
                    return Err(snapshot_error(format!(
                        "iterator references missing map {:?}",
                        state.map
                    )));
                }
            }
            IteratorState::SetEntries(ref state) | IteratorState::SetValues(ref state) => {
                if runtime.sets.get(state.set).is_none() {
                    return Err(snapshot_error(format!(
                        "iterator references missing set {:?}",
                        state.set
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_pending_host_call_snapshot(
    runtime: &Runtime,
    request: &PendingHostCall,
) -> MustardResult<()> {
    if let Some(promise) = request.promise
        && runtime.promises.get(promise).is_none()
    {
        return Err(snapshot_error(
            "pending host call references a missing promise",
        ));
    }
    Ok(())
}

fn validate_promise_combinator_target(
    runtime: &Runtime,
    owner: &str,
    target: PromiseKey,
    index: usize,
    kind: PromiseCombinatorKind,
) -> MustardResult<()> {
    let target_promise = runtime.promises.get(target).ok_or_else(|| {
        snapshot_error(format!(
            "{owner} combinator reaction references missing target {:?}",
            target
        ))
    })?;
    let Some(driver) = target_promise.driver.as_ref() else {
        return Err(snapshot_error(format!(
            "{owner} combinator target {:?} is missing driver state",
            target
        )));
    };
    let len = match (kind, driver) {
        (PromiseCombinatorKind::Race, _) => None,
        (PromiseCombinatorKind::All, PromiseDriver::All { values, .. }) => Some(values.len()),
        (PromiseCombinatorKind::AllSettled, PromiseDriver::AllSettled { results, .. }) => {
            Some(results.len())
        }
        (PromiseCombinatorKind::Any, PromiseDriver::Any { reasons, .. }) => Some(reasons.len()),
        _ => {
            return Err(snapshot_error(format!(
                "{owner} combinator reaction kind does not match target {:?} driver",
                target
            )));
        }
    };
    if let Some(len) = len
        && index >= len
    {
        return Err(snapshot_error(format!(
            "{owner} combinator index {} is out of range for target {:?}",
            index, target
        )));
    }
    Ok(())
}

fn validate_microtask_snapshot(runtime: &Runtime, microtask: &MicrotaskJob) -> MustardResult<()> {
    match microtask {
        MicrotaskJob::ResumeAsync {
            continuation: _,
            source,
        } => validate_settled_microtask_source(runtime, "resume async microtask", *source)?,
        MicrotaskJob::PromiseReaction { reaction, source } => {
            validate_settled_microtask_source(runtime, "promise reaction microtask", *source)?;
            let target = match reaction {
                PromiseReaction::Then { target, .. }
                | PromiseReaction::Finally { target, .. }
                | PromiseReaction::FinallyPassThrough { target, .. }
                | PromiseReaction::Combinator { target, .. } => *target,
            };
            if runtime.promises.get(target).is_none() {
                return Err(snapshot_error(format!(
                    "promise reaction microtask references missing target {:?}",
                    target
                )));
            }
            if let PromiseReaction::Combinator { index, kind, .. } = reaction {
                validate_promise_combinator_target(
                    runtime,
                    "promise reaction microtask",
                    target,
                    *index,
                    *kind,
                )?;
            }
        }
        MicrotaskJob::PromiseCombinator {
            target,
            index,
            kind,
            input,
        } => {
            if runtime.promises.get(*target).is_none() {
                return Err(snapshot_error(format!(
                    "promise combinator microtask references missing target {:?}",
                    target
                )));
            }
            validate_promise_combinator_target(
                runtime,
                "promise combinator microtask",
                *target,
                *index,
                *kind,
            )?;
            if let PromiseCombinatorInput::Promise(source) = input {
                validate_settled_microtask_source(
                    runtime,
                    "promise combinator microtask",
                    *source,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_settled_microtask_source(
    runtime: &Runtime,
    owner: &str,
    source: PromiseKey,
) -> MustardResult<()> {
    let source_promise = runtime
        .promises
        .get(source)
        .ok_or_else(|| snapshot_error(format!("{owner} references missing source {:?}", source)))?;
    if matches!(source_promise.state, PromiseState::Pending) {
        return Err(snapshot_error(format!(
            "{owner} source {:?} is still pending",
            source
        )));
    }
    Ok(())
}

fn validate_promise_snapshot(
    runtime: &Runtime,
    promise_key: PromiseKey,
    promise: &PromiseObject,
) -> MustardResult<()> {
    for dependent in &promise.dependents {
        if runtime.promises.get(*dependent).is_none() {
            return Err(snapshot_error(format!(
                "promise {:?} references missing dependent {:?}",
                promise_key, dependent
            )));
        }
    }
    for reaction in &promise.reactions {
        match reaction {
            PromiseReaction::Then { target, .. }
            | PromiseReaction::Finally { target, .. }
            | PromiseReaction::FinallyPassThrough { target, .. }
            | PromiseReaction::Combinator { target, .. } => {
                if runtime.promises.get(*target).is_none() {
                    return Err(snapshot_error(format!(
                        "promise {:?} reaction references missing target {:?}",
                        promise_key, target
                    )));
                }
            }
        }

        if let PromiseReaction::Combinator {
            target,
            index,
            kind,
        } = reaction
        {
            validate_promise_combinator_target(
                runtime,
                &format!("promise {:?}", promise_key),
                *target,
                *index,
                *kind,
            )?;
        }
    }
    Ok(())
}

fn validate_runtime_value(runtime: &Runtime, value: &Value) -> MustardResult<()> {
    match value {
        Value::Object(object) if runtime.objects.get(*object).is_none() => Err(snapshot_error(
            format!("value references missing object {:?}", object),
        )),
        Value::Array(array) if runtime.arrays.get(*array).is_none() => Err(snapshot_error(
            format!("value references missing array {:?}", array),
        )),
        Value::Map(map) if runtime.maps.get(*map).is_none() => Err(snapshot_error(format!(
            "value references missing map {:?}",
            map
        ))),
        Value::Set(set) if runtime.sets.get(*set).is_none() => Err(snapshot_error(format!(
            "value references missing set {:?}",
            set
        ))),
        Value::Iterator(iterator) if runtime.iterators.get(*iterator).is_none() => Err(
            snapshot_error(format!("value references missing iterator {:?}", iterator)),
        ),
        Value::Closure(closure) if runtime.closures.get(*closure).is_none() => Err(snapshot_error(
            format!("value references missing closure {:?}", closure),
        )),
        Value::Promise(promise) if runtime.promises.get(*promise).is_none() => Err(snapshot_error(
            format!("value references missing promise {:?}", promise),
        )),
        _ => Ok(()),
    }
}
