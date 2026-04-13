use super::*;

pub(super) fn walk_heap_values<F>(runtime: &Runtime, visit: &mut F) -> MustardResult<()>
where
    F: FnMut(&Value) -> MustardResult<()>,
{
    for cell in runtime.cells.values() {
        visit(&cell.value)?;
    }
    for object in runtime.objects.values() {
        match &object.kind {
            ObjectKind::FunctionPrototype(constructor) => visit(constructor)?,
            ObjectKind::BoundFunction(bound) => {
                visit(&bound.target)?;
                visit(&bound.this_value)?;
                for value in &bound.args {
                    visit(value)?;
                }
            }
            _ => {}
        }
        for value in object.properties.values() {
            visit(value)?;
        }
    }
    for array in runtime.arrays.values() {
        for value in array.elements.iter().flatten() {
            visit(value)?;
        }
        for value in array.properties.values() {
            visit(value)?;
        }
    }
    for map in runtime.maps.values() {
        for entry in &map.entries {
            let Some(entry) = entry else {
                continue;
            };
            visit(&entry.key)?;
            visit(&entry.value)?;
        }
    }
    for set in runtime.sets.values() {
        for value in &set.entries {
            let Some(value) = value else {
                continue;
            };
            visit(value)?;
        }
    }
    for closure in runtime.closures.values() {
        visit(&closure.this_value)?;
        if let Some(prototype) = closure.prototype {
            visit(&Value::Object(prototype))?;
        }
        for value in closure.properties.values() {
            visit(value)?;
        }
    }
    Ok(())
}

pub(super) fn walk_frame_values<F>(frame: &Frame, visit: &mut F) -> MustardResult<()>
where
    F: FnMut(&Value) -> MustardResult<()>,
{
    for value in &frame.stack {
        visit(value)?;
    }
    if let Some(value) = &frame.pending_exception {
        visit(value)?;
    }
    for completion in &frame.pending_completions {
        match completion {
            CompletionRecord::Return(value) | CompletionRecord::Throw(value) => visit(value)?,
            CompletionRecord::Jump { .. } => {}
        }
    }
    Ok(())
}

pub(super) fn walk_promise_values<F>(promise: &PromiseObject, visit: &mut F) -> MustardResult<()>
where
    F: FnMut(&Value) -> MustardResult<()>,
{
    match &promise.state {
        PromiseState::Pending => {}
        PromiseState::Fulfilled(value) => visit(value)?,
        PromiseState::Rejected(rejection) => visit(&rejection.value)?,
    }
    for reaction in &promise.reactions {
        match reaction {
            PromiseReaction::Then {
                on_fulfilled,
                on_rejected,
                ..
            } => {
                if let Some(handler) = on_fulfilled {
                    visit(handler)?;
                }
                if let Some(handler) = on_rejected {
                    visit(handler)?;
                }
            }
            PromiseReaction::Finally { callback, .. } => {
                if let Some(callback) = callback {
                    visit(callback)?;
                }
            }
            PromiseReaction::FinallyPassThrough {
                original_outcome, ..
            } => match original_outcome {
                PromiseOutcome::Fulfilled(value) => visit(value)?,
                PromiseOutcome::Rejected(rejection) => visit(&rejection.value)?,
            },
            PromiseReaction::Combinator { .. } => {}
        }
    }
    if let Some(driver) = &promise.driver {
        match driver {
            PromiseDriver::Thenable { value } => visit(value)?,
            PromiseDriver::All { values, .. } => {
                for value in values.iter().flatten() {
                    visit(value)?;
                }
            }
            PromiseDriver::AllSettled { results, .. } => {
                for result in results.iter().flatten() {
                    match result {
                        PromiseSettledResult::Fulfilled(value)
                        | PromiseSettledResult::Rejected(value) => visit(value)?,
                    }
                }
            }
            PromiseDriver::Any { reasons, .. } => {
                for value in reasons.iter().flatten() {
                    visit(value)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn walk_continuation_frames<F>(
    continuation: &AsyncContinuation,
    visit_frame: &mut F,
) -> MustardResult<()>
where
    F: FnMut(&Frame) -> MustardResult<()>,
{
    for frame in &continuation.frames {
        visit_frame(frame)?;
    }
    Ok(())
}

pub(super) fn walk_microtask_values<F, G>(
    microtask: &MicrotaskJob,
    visit_frame: &mut F,
    visit_value: &mut G,
) -> MustardResult<()>
where
    F: FnMut(&Frame) -> MustardResult<()>,
    G: FnMut(&Value) -> MustardResult<()>,
{
    match microtask {
        MicrotaskJob::ResumeAsync {
            continuation,
            outcome,
        } => {
            walk_continuation_frames(continuation, visit_frame)?;
            match outcome {
                PromiseOutcome::Fulfilled(value) => visit_value(value)?,
                PromiseOutcome::Rejected(rejection) => visit_value(&rejection.value)?,
            }
        }
        MicrotaskJob::PromiseReaction { reaction, outcome } => {
            match reaction {
                PromiseReaction::Then {
                    on_fulfilled,
                    on_rejected,
                    ..
                } => {
                    if let Some(handler) = on_fulfilled {
                        visit_value(handler)?;
                    }
                    if let Some(handler) = on_rejected {
                        visit_value(handler)?;
                    }
                }
                PromiseReaction::Finally { callback, .. } => {
                    if let Some(callback) = callback {
                        visit_value(callback)?;
                    }
                }
                PromiseReaction::FinallyPassThrough {
                    original_outcome, ..
                } => match original_outcome {
                    PromiseOutcome::Fulfilled(value) => visit_value(value)?,
                    PromiseOutcome::Rejected(rejection) => visit_value(&rejection.value)?,
                },
                PromiseReaction::Combinator { .. } => {}
            }
            match outcome {
                PromiseOutcome::Fulfilled(value) => visit_value(value)?,
                PromiseOutcome::Rejected(rejection) => visit_value(&rejection.value)?,
            }
        }
    }
    Ok(())
}
