use crate::{RuntimeLimits, compile};

use super::*;

fn suspend_async_host_wait(source: &str) -> Suspension {
    let program = compile(source).expect("source should compile");
    match start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend")
    {
        ExecutionStep::Suspended(suspension) => *suspension,
        other => panic!("expected suspension, got {other:?}"),
    }
}

#[test]
fn rejects_invalid_async_continuation_frame_state() {
    let mut suspension = suspend_async_host_wait(
        r#"
        async function main() {
          const value = await fetch_data(1);
          return value + 2;
        }
        main();
        "#,
    );

    let continuation = suspension
        .snapshot
        .runtime
        .promises
        .values_mut()
        .find_map(|promise| promise.awaiters.first_mut())
        .expect("awaiting async promise continuation should exist");
    continuation.frames[0].ip = 999;

    let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let error =
        load_snapshot(&bytes).expect_err("invalid async continuation should fail validation");
    assert!(
        error
            .to_string()
            .contains("frame instruction pointer 999 is out of range"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_invalid_microtask_frame_state() {
    let mut suspension = suspend_async_host_wait(
        r#"
        async function main() {
          const value = await fetch_data(1);
          return value + 2;
        }
        main();
        "#,
    );

    let mut continuation = suspension
        .snapshot
        .runtime
        .promises
        .values()
        .find_map(|promise| promise.awaiters.first().cloned())
        .expect("awaiting async promise continuation should exist");
    continuation.frames[0].ip = 999;
    suspension
        .snapshot
        .runtime
        .microtasks
        .push_back(MicrotaskJob::ResumeAsync {
            continuation,
            outcome: PromiseOutcome::Fulfilled(Value::Number(1.0)),
        });

    let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let error =
        load_snapshot(&bytes).expect_err("invalid microtask continuation should fail validation");
    assert!(
        error
            .to_string()
            .contains("frame instruction pointer 999 is out of range"),
        "unexpected error: {error}"
    );
}
