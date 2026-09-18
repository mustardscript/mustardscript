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

#[test]
fn rejects_malformed_structured_control_transfers() {
    for instruction in [
        Instruction::AbruptJump {
            target: 99,
            target_handler_depth: 0,
            target_scope_depth: 0,
            target_finally_depth: 0,
        },
        Instruction::AbruptJump {
            target: 1,
            target_handler_depth: 0,
            target_scope_depth: 0,
            target_finally_depth: 1,
        },
        Instruction::PushCompletionJump {
            target: 1,
            target_handler_depth: 1,
            target_scope_depth: 0,
            target_finally_depth: 0,
        },
        Instruction::ContinuePendingRegion {
            handler_depth: 0,
            scope_depth: 0,
        },
        Instruction::AbruptReturn,
    ] {
        let mut program = lower_to_bytecode(&compile("1;").unwrap()).unwrap();
        program.functions[program.root].code = vec![instruction, Instruction::Return];
        let bytes = dump_program(&program).expect("serialize malformed program");
        assert!(load_program(&bytes).is_err(), "malformed transfer accepted");
    }
}

#[test]
fn rejects_invalid_async_array_driver_and_reaction_state() {
    for corrupt_reaction in [false, true] {
        let mut suspension = suspend_async_host_wait(
            "Array.fromAsync([1, 2], async value => await fetch_data(value));",
        );
        let runtime = &mut suspension.snapshot.runtime;
        let state = runtime
            .promises
            .values_mut()
            .find_map(|promise| match promise.driver.as_mut() {
                Some(PromiseDriver::ArrayFromAsync(state)) => Some(state),
                _ => None,
            })
            .expect("fromAsync driver");
        if corrupt_reaction {
            state.phase = ArrayFromAsyncPhase::Value;
        } else {
            state.index = usize::MAX;
        }
        let bytes = dump_snapshot(&suspension.snapshot).expect("serialize corrupt snapshot");
        let error = load_snapshot(&bytes).expect_err("corrupt async array snapshot must reject");
        assert!(error.to_string().contains("Array.fromAsync"), "{error}");
    }
}

#[test]
fn rejects_invalid_number_format_configuration_before_allocation() {
    for invalid in 0..3 {
        let mut suspension = suspend_async_host_wait(
            "const f = Intl.NumberFormat('en-US', {style:'currency',currency:'USD'}); await fetch_data(); f.format(1);",
        );
        let formatter = suspension
            .snapshot
            .runtime
            .objects
            .values_mut()
            .find_map(|object| match &mut object.kind {
                ObjectKind::IntlNumberFormat(formatter) => Some(formatter),
                _ => None,
            })
            .expect("number formatter");
        match invalid {
            0 => formatter.maximum_fraction_digits = usize::MAX,
            1 => formatter.minimum_fraction_digits = 3,
            _ => formatter.currency = None,
        }
        let bytes = dump_snapshot(&suspension.snapshot).expect("serialize invalid formatter");
        let error = load_snapshot(&bytes).expect_err("invalid formatter must reject");
        assert!(error.to_string().contains("Intl.NumberFormat"), "{error}");
    }
}

#[test]
fn rejects_corrupt_equality_continuations_in_snapshots() {
    for corruption in 0..6 {
        let mut suspension = suspend_async_host_wait("[{toString: fetch_data}] == '7';");
        let frame = &mut suspension.snapshot.runtime.frames[0];
        let state = frame
            .pending_equality
            .as_mut()
            .expect("equality is pending");
        match corruption {
            0 => state.primitive = state.work[0].root(),
            1 => state.negate = !state.negate,
            2 => state.work.clear(),
            3 => {
                if let CoercionWork::Primitive { next_method, .. } = state.work.last_mut().unwrap()
                {
                    *next_method = 3;
                } else {
                    panic!("expected pending method");
                }
            }
            4 => {
                for work in &mut state.work {
                    if let CoercionWork::ArrayJoin {
                        next_index, length, ..
                    } = work
                    {
                        *next_index = *length + 1;
                    }
                }
            }
            5 => frame.ip = 1,
            _ => unreachable!(),
        }
        let bytes = dump_snapshot(&suspension.snapshot).unwrap();
        let error = load_snapshot(&bytes).expect_err("corrupt equality must not resume");
        assert!(error.to_string().contains("continuation"), "{error}");
    }
}

#[test]
fn rejects_invalid_error_property_attributes() {
    for invalid in [0x80, ErrorObject::property_bit("cause")] {
        let mut suspension =
            suspend_async_host_wait("const error=new Error(); fetch_data(error.name); error;");
        let error = suspension
            .snapshot
            .runtime
            .objects
            .values_mut()
            .find_map(|object| match &mut object.kind {
                ObjectKind::Error(error) => Some(error),
                _ => None,
            })
            .expect("guest error exists");
        error.non_enumerable |= invalid;
        let bytes = dump_snapshot(&suspension.snapshot).unwrap();
        assert!(
            load_snapshot(&bytes)
                .unwrap_err()
                .to_string()
                .contains("Error")
        );
    }
}
