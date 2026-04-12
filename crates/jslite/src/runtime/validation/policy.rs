use super::*;

pub(in crate::runtime) fn validate_snapshot_policy(
    runtime: &Runtime,
    policy: &SnapshotPolicy,
) -> JsliteResult<()> {
    let allowed = policy
        .capabilities
        .iter()
        .cloned()
        .collect::<HashSet<String>>();

    walk::walk_heap_values(runtime, &mut |value| {
        validate_runtime_host_capability(value, &allowed)
    })?;
    for frame in &runtime.frames {
        validate_frame_host_capabilities(frame, &allowed)?;
    }
    if let Some(root_result) = &runtime.root_result {
        validate_runtime_host_capability(root_result, &allowed)?;
    }
    for request in &runtime.pending_host_calls {
        validate_pending_host_call_capability(request, &allowed)?;
    }
    if let Some(request) = &runtime.suspended_host_call {
        validate_pending_host_call_capability(request, &allowed)?;
    }
    for promise in runtime.promises.values() {
        walk::walk_promise_values(promise, &mut |value| {
            validate_runtime_host_capability(value, &allowed)
        })?;
        for continuation in &promise.awaiters {
            walk::walk_continuation_frames(continuation, &mut |frame| {
                validate_frame_host_capabilities(frame, &allowed)
            })?;
        }
    }
    for microtask in &runtime.microtasks {
        walk::walk_microtask_values(
            microtask,
            &mut |frame| validate_frame_host_capabilities(frame, &allowed),
            &mut |value| validate_runtime_host_capability(value, &allowed),
        )?;
    }
    if let Some(exception) = &runtime.pending_internal_exception {
        validate_promise_rejection_host_capabilities(exception, &allowed)?;
    }
    Ok(())
}

fn validate_runtime_host_capability(value: &Value, allowed: &HashSet<String>) -> JsliteResult<()> {
    match value {
        Value::HostFunction(capability) if !allowed.contains(capability) => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!("snapshot policy rejected unauthorized capability `{capability}`"),
                span: None,
                traceback: Vec::new(),
            })
        }
        _ => Ok(()),
    }
}

fn validate_pending_host_call_capability(
    request: &PendingHostCall,
    allowed: &HashSet<String>,
) -> JsliteResult<()> {
    if !allowed.contains(&request.capability) {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "snapshot policy rejected unauthorized capability `{}`",
                request.capability
            ),
            span: None,
            traceback: Vec::new(),
        });
    }
    Ok(())
}

fn validate_frame_host_capabilities(frame: &Frame, allowed: &HashSet<String>) -> JsliteResult<()> {
    walk::walk_frame_values(frame, &mut |value| validate_runtime_host_capability(value, allowed))
}

fn validate_promise_rejection_host_capabilities(
    rejection: &PromiseRejection,
    allowed: &HashSet<String>,
) -> JsliteResult<()> {
    validate_runtime_host_capability(&rejection.value, allowed)
}
