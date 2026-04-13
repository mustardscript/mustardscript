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
    let source = suspension
        .snapshot
        .runtime
        .insert_promise(PromiseState::Pending)
        .expect("settled microtask source should allocate");
    suspension
        .snapshot
        .runtime
        .resolve_promise(source, Value::Number(1.0))
        .expect("settled microtask source should resolve");
    suspension
        .snapshot
        .runtime
        .microtasks
        .push_back(MicrotaskJob::ResumeAsync {
            continuation,
            source,
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

#[test]
fn rejects_pending_promise_combinator_microtask_source() {
    let mut suspension = suspend_async_host_wait(
        r#"
        async function main() {
          const value = await fetch_data(1);
          return value + 2;
        }
        main();
        "#,
    );

    let target = suspension
        .snapshot
        .runtime
        .insert_promise(PromiseState::Pending)
        .expect("Promise.all target should allocate");
    suspension
        .snapshot
        .runtime
        .replace_promise_driver(
            target,
            Some(PromiseDriver::All {
                remaining: 1,
                values: vec![None],
            }),
        )
        .expect("Promise.all driver should attach");
    let pending_source = suspension
        .snapshot
        .runtime
        .insert_promise(PromiseState::Pending)
        .expect("pending source promise should allocate");
    suspension
        .snapshot
        .runtime
        .microtasks
        .push_back(MicrotaskJob::PromiseCombinator {
            target,
            index: 0,
            kind: PromiseCombinatorKind::All,
            input: PromiseCombinatorInput::Promise(pending_source),
        });

    let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let error =
        load_snapshot(&bytes).expect_err("pending combinator source should fail validation");
    assert!(
        error
            .to_string()
            .contains("promise combinator microtask source")
            && error.to_string().contains("pending"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_pending_promise_reaction_microtask_source() {
    let mut suspension = suspend_async_host_wait(
        r#"
        async function main() {
          const value = await fetch_data(1);
          return value + 2;
        }
        main();
        "#,
    );

    let target = suspension
        .snapshot
        .runtime
        .insert_promise(PromiseState::Pending)
        .expect("reaction target promise should allocate");
    let pending_source = suspension
        .snapshot
        .runtime
        .insert_promise(PromiseState::Pending)
        .expect("pending source promise should allocate");
    suspension
        .snapshot
        .runtime
        .microtasks
        .push_back(MicrotaskJob::PromiseReaction {
            reaction: PromiseReaction::Then {
                target,
                on_fulfilled: None,
                on_rejected: None,
            },
            source: pending_source,
        });

    let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let error = load_snapshot(&bytes).expect_err("pending reaction source should fail validation");
    assert!(
        error
            .to_string()
            .contains("promise reaction microtask source")
            && error.to_string().contains("pending"),
        "unexpected error: {error}"
    );
}
