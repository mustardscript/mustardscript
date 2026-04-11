use std::collections::VecDeque;

use crate::diagnostic::{DiagnosticKind, JsliteError, JsliteResult};

use super::{
    CompletionRecord, IteratorState, PromiseDriver, PromiseOutcome, PromiseReaction,
    PromiseSettledResult, PromiseState, Runtime, Value,
    api::ExecutionSnapshot,
    bytecode::{BytecodeProgram, FunctionPrototype, Instruction},
    limit_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidationState {
    stack_depth: usize,
    scope_depth: usize,
    handler_depth: usize,
    pending_depth: usize,
}

pub(super) fn validate_bytecode_program(program: &BytecodeProgram) -> JsliteResult<()> {
    if program.functions.is_empty() {
        return Err(JsliteError::validation(
            "bytecode validation failed: program defines no functions",
            None,
        ));
    }
    if program.root >= program.functions.len() {
        return Err(JsliteError::validation(
            format!(
                "bytecode validation failed: root function {} is out of range for {} functions",
                program.root,
                program.functions.len()
            ),
            None,
        ));
    }
    for (function_id, function) in program.functions.iter().enumerate() {
        validate_function(program, function_id, function)?;
    }
    Ok(())
}

fn validate_function(
    program: &BytecodeProgram,
    function_id: usize,
    function: &FunctionPrototype,
) -> JsliteResult<()> {
    if function.code.is_empty() {
        return Err(JsliteError::validation(
            format!("bytecode validation failed: function {function_id} has no instructions"),
            None,
        ));
    }
    if !matches!(function.code.last(), Some(Instruction::Return)) {
        return Err(JsliteError::validation(
            format!("bytecode validation failed: function {function_id} does not end in Return"),
            None,
        ));
    }

    let code_len = function.code.len();
    for (ip, instruction) in function.code.iter().enumerate() {
        match instruction {
            Instruction::MakeClosure {
                function_id: target,
            } if *target >= program.functions.len() => {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} references missing closure target {target}"
                    ),
                    None,
                ));
            }
            Instruction::Jump(target)
            | Instruction::JumpIfFalse(target)
            | Instruction::JumpIfTrue(target)
            | Instruction::JumpIfNullish(target)
            | Instruction::EnterFinally { exit: target }
            | Instruction::PushPendingJump { target, .. }
                if *target >= code_len =>
            {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} jumps to invalid target {target}"
                    ),
                    None,
                ));
            }
            Instruction::PushHandler { catch, finally }
                if catch.is_some_and(|target| target >= code_len)
                    || finally.is_some_and(|target| target >= code_len) =>
            {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} references an invalid exception target"
                    ),
                    None,
                ));
            }
            _ => {}
        }
    }

    let mut states = vec![None; code_len];
    let mut work = VecDeque::from([(
        0usize,
        ValidationState {
            stack_depth: 0,
            scope_depth: 0,
            handler_depth: 0,
            pending_depth: 0,
        },
    )]);
    while let Some((ip, state)) = work.pop_front() {
        if let Some(existing) = states[ip] {
            if existing != state {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} has inconsistent validation state: existing={existing:?}, new={state:?}"
                    ),
                    None,
                ));
            }
            continue;
        }
        states[ip] = Some(state);

        let instruction = &function.code[ip];
        let next_state = apply_validation_effect(function_id, ip, instruction, state)?;
        for successor in validation_successors(ip, instruction, code_len) {
            work.push_back((successor, next_state));
        }
        match instruction {
            Instruction::PushHandler { catch, finally } => {
                if let Some(target) = catch {
                    work.push_back((
                        *target,
                        ValidationState {
                            handler_depth: state.handler_depth,
                            ..state
                        },
                    ));
                } else if let Some(target) = finally {
                    work.push_back((
                        *target,
                        ValidationState {
                            handler_depth: state.handler_depth,
                            pending_depth: state.pending_depth + 1,
                            ..state
                        },
                    ));
                }
            }
            Instruction::PushPendingJump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => {
                work.push_back((
                    *target,
                    ValidationState {
                        scope_depth: *target_scope_depth,
                        handler_depth: *target_handler_depth,
                        ..state
                    },
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

fn apply_validation_effect(
    function_id: usize,
    ip: usize,
    instruction: &Instruction,
    state: ValidationState,
) -> JsliteResult<ValidationState> {
    let require_stack = |count: usize| -> JsliteResult<()> {
        if state.stack_depth < count {
            return Err(JsliteError::validation(
                format!(
                    "bytecode validation failed: function {function_id} instruction {ip} requires stack depth {count}, found {}",
                    state.stack_depth
                ),
                None,
            ));
        }
        Ok(())
    };

    let next = match instruction {
        Instruction::PushUndefined
        | Instruction::PushNull
        | Instruction::PushBool(_)
        | Instruction::PushNumber(_)
        | Instruction::PushString(_)
        | Instruction::PushRegExp { .. }
        | Instruction::LoadName(_)
        | Instruction::MakeClosure { .. }
        | Instruction::BeginCatch => ValidationState {
            stack_depth: state.stack_depth + 1,
            ..state
        },
        Instruction::StoreName(_) => {
            require_stack(1)?;
            state
        }
        Instruction::InitializePattern(_) | Instruction::Pop => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::PushEnv => ValidationState {
            scope_depth: state.scope_depth + 1,
            ..state
        },
        Instruction::PopEnv => {
            if state.scope_depth == 0 {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} pops an empty scope stack"
                    ),
                    None,
                ));
            }
            ValidationState {
                scope_depth: state.scope_depth - 1,
                ..state
            }
        }
        Instruction::DeclareName { .. } => state,
        Instruction::MakeArray { count } => {
            require_stack(*count)?;
            ValidationState {
                stack_depth: state.stack_depth - count + 1,
                ..state
            }
        }
        Instruction::MakeObject { keys } => {
            require_stack(keys.len())?;
            ValidationState {
                stack_depth: state.stack_depth - keys.len() + 1,
                ..state
            }
        }
        Instruction::CreateIterator => {
            require_stack(1)?;
            state
        }
        Instruction::IteratorNext => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth + 1,
                ..state
            }
        }
        Instruction::GetPropStatic { .. } => {
            require_stack(1)?;
            state
        }
        Instruction::GetPropComputed { .. } => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::SetPropStatic { .. } => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::SetPropComputed => {
            require_stack(3)?;
            ValidationState {
                stack_depth: state.stack_depth - 2,
                ..state
            }
        }
        Instruction::Unary(_) => {
            require_stack(1)?;
            state
        }
        Instruction::Binary(_) => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::Dup => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth + 1,
                ..state
            }
        }
        Instruction::Dup2 => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth + 2,
                ..state
            }
        }
        Instruction::PushHandler { .. } => ValidationState {
            handler_depth: state.handler_depth + 1,
            ..state
        },
        Instruction::PopHandler => {
            if state.handler_depth == 0 {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} pops an empty handler stack"
                    ),
                    None,
                ));
            }
            ValidationState {
                handler_depth: state.handler_depth - 1,
                ..state
            }
        }
        Instruction::EnterFinally { .. } => state,
        Instruction::Throw { .. } => {
            require_stack(1)?;
            state
        }
        Instruction::PushPendingJump {
            target_handler_depth,
            target_scope_depth,
            ..
        } => {
            if *target_handler_depth > state.handler_depth {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} targets handler depth {target_handler_depth} from depth {}",
                        state.handler_depth
                    ),
                    None,
                ));
            }
            if *target_scope_depth > state.scope_depth {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} targets scope depth {target_scope_depth} from depth {}",
                        state.scope_depth
                    ),
                    None,
                ));
            }
            ValidationState {
                pending_depth: state.pending_depth + 1,
                ..state
            }
        }
        Instruction::PushPendingReturn => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                pending_depth: state.pending_depth + 1,
                ..state
            }
        }
        Instruction::PushPendingThrow => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                pending_depth: state.pending_depth + 1,
                ..state
            }
        }
        Instruction::ContinuePending => {
            if state.pending_depth == 0 {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} resumes without a pending completion"
                    ),
                    None,
                ));
            }
            ValidationState {
                pending_depth: state.pending_depth - 1,
                ..state
            }
        }
        Instruction::Jump(_) => state,
        Instruction::JumpIfFalse(_)
        | Instruction::JumpIfTrue(_)
        | Instruction::JumpIfNullish(_) => {
            require_stack(1)?;
            state
        }
        Instruction::Call {
            argc, with_this, ..
        } => {
            let required = argc + 1 + usize::from(*with_this);
            require_stack(required)?;
            ValidationState {
                stack_depth: state.stack_depth - required + 1,
                ..state
            }
        }
        Instruction::Await => {
            require_stack(1)?;
            state
        }
        Instruction::Construct { argc } => {
            let required = argc + 1;
            require_stack(required)?;
            ValidationState {
                stack_depth: state.stack_depth - required + 1,
                ..state
            }
        }
        Instruction::Return => state,
    };
    Ok(next)
}

fn validation_successors(ip: usize, instruction: &Instruction, code_len: usize) -> Vec<usize> {
    match instruction {
        Instruction::Jump(target) => vec![*target],
        Instruction::JumpIfFalse(target)
        | Instruction::JumpIfTrue(target)
        | Instruction::JumpIfNullish(target) => {
            let mut successors = vec![*target];
            if ip + 1 < code_len {
                successors.push(ip + 1);
            }
            successors
        }
        Instruction::ContinuePending | Instruction::Throw { .. } | Instruction::Return => {
            Vec::new()
        }
        _ if ip + 1 < code_len => vec![ip + 1],
        _ => Vec::new(),
    }
}

pub(super) fn validate_snapshot(snapshot: &ExecutionSnapshot) -> JsliteResult<()> {
    let runtime = &snapshot.runtime;
    validate_bytecode_program(&runtime.program)?;
    if runtime.envs.get(runtime.globals).is_none() {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot validation failed: missing globals environment".to_string(),
            span: None,
            traceback: Vec::new(),
        });
    }
    if runtime.frames.len() > runtime.limits.call_depth_limit {
        return Err(limit_error("call depth limit exceeded"));
    }
    if runtime.frames.is_empty()
        && runtime.suspended_host_call.is_none()
        && runtime.root_result.is_none()
        && runtime.microtasks.is_empty()
    {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot validation failed: suspended runtime has no frames or async state"
                .to_string(),
            span: None,
            traceback: Vec::new(),
        });
    }

    for (env_key, env) in &runtime.envs {
        if let Some(parent) = env.parent
            && runtime.envs.get(parent).is_none()
        {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: environment {:?} references missing parent {:?}",
                    env_key, parent
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        for cell in env.bindings.values() {
            if runtime.cells.get(*cell).is_none() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: environment {:?} references missing cell {:?}",
                        env_key, cell
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
        }
    }

    for (closure_key, closure) in &runtime.closures {
        if closure.function_id >= runtime.program.functions.len() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: closure {:?} references missing function {}",
                    closure_key, closure.function_id
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        if runtime.envs.get(closure.env).is_none() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: closure {:?} references missing environment {:?}",
                    closure_key, closure.env
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
    }

    for frame in &runtime.frames {
        let Some(function) = runtime.program.functions.get(frame.function_id) else {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: frame references missing function {}",
                    frame.function_id
                ),
                span: None,
                traceback: Vec::new(),
            });
        };
        if frame.ip >= function.code.len() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: frame instruction pointer {} is out of range for function {}",
                    frame.ip, frame.function_id
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        if runtime.envs.get(frame.env).is_none() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: frame references missing environment {:?}",
                    frame.env
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        for env in &frame.scope_stack {
            if runtime.envs.get(*env).is_none() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame scope stack references missing environment {:?}",
                        env
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
        }
        for value in &frame.stack {
            validate_runtime_value(runtime, value)?;
        }
        if let Some(value) = &frame.pending_exception {
            validate_runtime_value(runtime, value)?;
        }
        for handler in &frame.handlers {
            if let Some(catch) = handler.catch
                && catch >= function.code.len()
            {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame handler catch target {} is out of range for function {}",
                        catch, frame.function_id
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if let Some(finally) = handler.finally
                && finally >= function.code.len()
            {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame handler finally target {} is out of range for function {}",
                        finally, frame.function_id
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if runtime.envs.get(handler.env).is_none() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame handler references missing environment {:?}",
                        handler.env
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if handler.scope_stack_len > frame.scope_stack.len()
                || handler.stack_len > frame.stack.len()
            {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: "snapshot validation failed: frame handler restore state exceeds the current frame state".to_string(),
                    span: None,
                    traceback: Vec::new(),
                });
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
                        return Err(JsliteError::Message {
                            kind: DiagnosticKind::Serialization,
                            message: format!(
                                "snapshot validation failed: pending jump target {} is out of range for function {}",
                                target, frame.function_id
                            ),
                            span: None,
                            traceback: Vec::new(),
                        });
                    }
                    if *target_handler_depth > frame.handlers.len() {
                        return Err(JsliteError::Message {
                            kind: DiagnosticKind::Serialization,
                            message: format!(
                                "snapshot validation failed: pending jump targets handler depth {} but only {} handlers are active",
                                target_handler_depth,
                                frame.handlers.len()
                            ),
                            span: None,
                            traceback: Vec::new(),
                        });
                    }
                    if *target_scope_depth > frame.scope_stack.len() {
                        return Err(JsliteError::Message {
                            kind: DiagnosticKind::Serialization,
                            message: format!(
                                "snapshot validation failed: pending jump targets scope depth {} but only {} scopes are active",
                                target_scope_depth,
                                frame.scope_stack.len()
                            ),
                            span: None,
                            traceback: Vec::new(),
                        });
                    }
                }
                CompletionRecord::Return(value) | CompletionRecord::Throw(value) => {
                    validate_runtime_value(runtime, value)?;
                }
            }
        }
        for active in &frame.active_finally {
            if active.completion_index >= frame.pending_completions.len() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message:
                        "snapshot validation failed: active finally references a missing completion"
                            .to_string(),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if active.exit >= function.code.len() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: active finally exit target {} is out of range for function {}",
                        active.exit, frame.function_id
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
        }
    }

    for cell in runtime.cells.values() {
        validate_runtime_value(runtime, &cell.value)?;
    }
    for object in runtime.objects.values() {
        for value in object.properties.values() {
            validate_runtime_value(runtime, value)?;
        }
    }
    for array in runtime.arrays.values() {
        for value in &array.elements {
            validate_runtime_value(runtime, value)?;
        }
        for value in array.properties.values() {
            validate_runtime_value(runtime, value)?;
        }
    }
    for map in runtime.maps.values() {
        for entry in &map.entries {
            validate_runtime_value(runtime, &entry.key)?;
            validate_runtime_value(runtime, &entry.value)?;
        }
    }
    for set in runtime.sets.values() {
        for value in &set.entries {
            validate_runtime_value(runtime, value)?;
        }
    }
    for iterator in runtime.iterators.values() {
        match iterator.state {
            IteratorState::Array(ref state) => {
                if runtime.arrays.get(state.array).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing array {:?}",
                            state.array
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
            IteratorState::ArrayKeys(ref state) | IteratorState::ArrayEntries(ref state) => {
                if runtime.arrays.get(state.array).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing array {:?}",
                            state.array
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
            IteratorState::String(_) => {}
            IteratorState::MapEntries(ref state)
            | IteratorState::MapKeys(ref state)
            | IteratorState::MapValues(ref state) => {
                if runtime.maps.get(state.map).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing map {:?}",
                            state.map
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
            IteratorState::SetEntries(ref state) | IteratorState::SetValues(ref state) => {
                if runtime.sets.get(state.set).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing set {:?}",
                            state.set
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
        }
    }
    if let Some(root_result) = &runtime.root_result {
        validate_runtime_value(runtime, root_result)?;
    }
    for request in &runtime.pending_host_calls {
        if runtime.promises.get(request.promise).is_none() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message:
                    "snapshot validation failed: pending host call references a missing promise"
                        .to_string(),
                span: None,
                traceback: Vec::new(),
            });
        }
    }
    if let Some(request) = &runtime.suspended_host_call
        && runtime.promises.get(request.promise).is_none()
    {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot validation failed: suspended host call references a missing promise"
                .to_string(),
            span: None,
            traceback: Vec::new(),
        });
    }
    for promise in runtime.promises.values() {
        match &promise.state {
            PromiseState::Pending => {}
            PromiseState::Fulfilled(value) => validate_runtime_value(runtime, value)?,
            PromiseState::Rejected(rejection) => validate_runtime_value(runtime, &rejection.value)?,
        }
        for reaction in &promise.reactions {
            match reaction {
                PromiseReaction::Then {
                    on_fulfilled,
                    on_rejected,
                    ..
                } => {
                    if let Some(handler) = on_fulfilled {
                        validate_runtime_value(runtime, handler)?;
                    }
                    if let Some(handler) = on_rejected {
                        validate_runtime_value(runtime, handler)?;
                    }
                }
                PromiseReaction::Finally { callback, .. } => {
                    if let Some(callback) = callback {
                        validate_runtime_value(runtime, callback)?;
                    }
                }
                PromiseReaction::FinallyPassThrough {
                    original_outcome, ..
                } => match original_outcome {
                    PromiseOutcome::Fulfilled(value) => validate_runtime_value(runtime, value)?,
                    PromiseOutcome::Rejected(rejection) => {
                        validate_runtime_value(runtime, &rejection.value)?
                    }
                },
                PromiseReaction::Combinator { .. } => {}
            }
        }
        if let Some(driver) = &promise.driver {
            match driver {
                PromiseDriver::All { values, .. } => {
                    for value in values.iter().flatten() {
                        validate_runtime_value(runtime, value)?;
                    }
                }
                PromiseDriver::AllSettled { results, .. } => {
                    for result in results.iter().flatten() {
                        match result {
                            PromiseSettledResult::Fulfilled(value)
                            | PromiseSettledResult::Rejected(value) => {
                                validate_runtime_value(runtime, value)?
                            }
                        }
                    }
                }
                PromiseDriver::Any { reasons, .. } => {
                    for value in reasons.iter().flatten() {
                        validate_runtime_value(runtime, value)?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_runtime_value(runtime: &Runtime, value: &Value) -> JsliteResult<()> {
    match value {
        Value::Object(object) if runtime.objects.get(*object).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing object {:?}",
                    object
                ),
                span: None,
                traceback: Vec::new(),
            })
        }
        Value::Array(array) if runtime.arrays.get(*array).is_none() => Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "snapshot validation failed: value references missing array {:?}",
                array
            ),
            span: None,
            traceback: Vec::new(),
        }),
        Value::Map(map) if runtime.maps.get(*map).is_none() => Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "snapshot validation failed: value references missing map {:?}",
                map
            ),
            span: None,
            traceback: Vec::new(),
        }),
        Value::Set(set) if runtime.sets.get(*set).is_none() => Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "snapshot validation failed: value references missing set {:?}",
                set
            ),
            span: None,
            traceback: Vec::new(),
        }),
        Value::Iterator(iterator) if runtime.iterators.get(*iterator).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing iterator {:?}",
                    iterator
                ),
                span: None,
                traceback: Vec::new(),
            })
        }
        Value::Closure(closure) if runtime.closures.get(*closure).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing closure {:?}",
                    closure
                ),
                span: None,
                traceback: Vec::new(),
            })
        }
        Value::Promise(promise) if runtime.promises.get(*promise).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing promise {:?}",
                    promise
                ),
                span: None,
                traceback: Vec::new(),
            })
        }
        _ => Ok(()),
    }
}
