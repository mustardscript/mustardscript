use super::*;

#[test]
fn round_trips_program_and_snapshot() {
    let source = "const value = fetch_data(1); value + 2;";
    let program = compile(source).expect("compile should succeed");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let program_bytes = dump_program(&bytecode).expect("program dump should succeed");
    let loaded_program = load_program(&program_bytes).expect("program load should succeed");
    assert_eq!(loaded_program.root, bytecode.root);
    assert_eq!(loaded_program.functions.len(), bytecode.functions.len());

    let suspension = suspend(source, &["fetch_data"]);
    let snapshot_bytes = dump_snapshot(&suspension.snapshot).expect("snapshot dump should succeed");
    let loaded_snapshot = load_snapshot(&snapshot_bytes).expect("snapshot load should succeed");
    let resumed = resume_with_options(
        loaded_snapshot,
        ResumePayload::Value(number(1.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(SnapshotPolicy {
                capabilities: vec!["fetch_data".to_string()],
                limits: RuntimeLimits::default(),
            }),
        },
    )
    .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(value, number(3.0));
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn round_trips_detached_snapshot_with_external_program() {
    let source = "const value = fetch_data(1); value + 2;";
    let program = compile(source).expect("compile should succeed");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let suspension = suspend(source, &["fetch_data"]);
    let snapshot_bytes =
        dump_detached_snapshot(&suspension.snapshot).expect("detached snapshot dump should succeed");
    let loaded_snapshot = load_detached_snapshot(&snapshot_bytes, std::sync::Arc::new(bytecode))
        .expect("detached snapshot load should succeed");
    let resumed = resume_with_options(
        loaded_snapshot,
        ResumePayload::Value(number(1.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(SnapshotPolicy {
                capabilities: vec!["fetch_data".to_string()],
                limits: RuntimeLimits::default(),
            }),
        },
    )
    .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(value, number(3.0));
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn rejects_detached_snapshot_with_mismatched_program_identity() {
    let suspension = suspend("const value = fetch_data(1); value + 2;", &["fetch_data"]);
    let snapshot_bytes =
        dump_detached_snapshot(&suspension.snapshot).expect("detached snapshot should serialize");
    let wrong_program = lower_to_bytecode(&compile("1;").expect("compile should succeed"))
        .expect("lowering should succeed");
    let error = load_detached_snapshot(&snapshot_bytes, std::sync::Arc::new(wrong_program))
        .expect_err("mismatched detached program should fail");
    assert!(
        error
            .to_string()
            .contains("detached snapshot program identity mismatch")
    );
}

#[test]
fn rejects_invalid_jump_targets_before_execution() {
    let program = invalid_program(vec![Instruction::Jump(99), Instruction::Return]);
    let error = start_bytecode(&program, ExecutionOptions::default())
        .expect_err("invalid jump target should fail validation");
    assert!(error.to_string().contains("jumps to invalid target 99"));
}

#[test]
fn rejects_inconsistent_stack_depth_in_serialized_programs() {
    let program = invalid_program(vec![
        Instruction::PushNumber(1.0),
        Instruction::JumpIfTrue(3),
        Instruction::Pop,
        Instruction::Return,
    ]);
    let bytes = dump_program(&program).expect("invalid program still serializes");
    let error =
        load_program(&bytes).expect_err("invalid serialized program should fail validation");
    assert!(
        error
            .to_string()
            .contains("has inconsistent validation state")
    );
}

#[test]
fn rejects_cross_version_serialized_programs() {
    let program = lower_to_bytecode(&compile("1;").expect("compile should succeed"))
        .expect("lowering should succeed");
    let mut bytes = dump_program(&program).expect("program should serialize");
    bytes[0] = bytes[0].saturating_add(1);
    let error = load_program(&bytes).expect_err("cross-version program should be rejected");
    assert!(
        error
            .to_string()
            .contains("serialized program version mismatch")
    );
}

#[test]
fn rejects_invalid_snapshot_frame_state() {
    let mut suspension = suspend("const value = fetch_data(1); value + 2;", &["fetch_data"]);
    suspension.snapshot.runtime.frames[0].ip = 999;
    let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let error = load_snapshot(&bytes).expect_err("invalid snapshot should fail validation");
    assert!(
        error
            .to_string()
            .contains("frame instruction pointer 999 is out of range")
    );
}

#[test]
fn direct_execution_snapshot_deserialize_reruns_validation() {
    let mut suspension = suspend("const value = fetch_data(1); value + 2;", &["fetch_data"]);
    suspension.snapshot.runtime.frames[0].ip = 999;
    let bytes =
        bincode::serialize(&suspension.snapshot).expect("snapshot should serialize directly");
    let error = bincode::deserialize::<ExecutionSnapshot>(&bytes)
        .expect_err("invalid snapshots should fail validation during direct deserialize");
    assert!(
        error
            .to_string()
            .contains("frame instruction pointer 999 is out of range")
    );
}

#[test]
fn rejects_cross_version_snapshots() {
    let suspension = suspend("const value = fetch_data(1); value + 2;", &["fetch_data"]);

    let mut bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    bytes[0] = bytes[0].saturating_add(1);
    let error = load_snapshot(&bytes).expect_err("cross-version snapshot should be rejected");
    assert!(
        error
            .to_string()
            .contains("serialized snapshot version mismatch")
    );
}

#[test]
fn rejects_out_of_range_promise_combinator_snapshot_state() {
    let mut suspension = suspend(
        r#"
        async function main() {
          return Promise.all([fetch_data(1), fetch_data(2)]);
        }
        main();
        "#,
        &["fetch_data"],
    );

    let target = suspension
        .snapshot
        .runtime
        .promises
        .iter()
        .find_map(|(key, promise)| match promise.driver.as_ref() {
            Some(PromiseDriver::All { .. }) => Some(key),
            _ => None,
        })
        .expect("Promise.all target should exist");

    let mutated = suspension
        .snapshot
        .runtime
        .promises
        .values_mut()
        .find_map(|promise| {
            promise.reactions.iter_mut().find_map(|reaction| match reaction {
                PromiseReaction::Combinator {
                    target: reaction_target,
                    index,
                    ..
                } if *reaction_target == target => {
                    *index = 99;
                    Some(())
                }
                _ => None,
            })
        });
    assert!(mutated.is_some(), "Promise.all combinator reaction should exist");

    let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let error = load_snapshot(&bytes).expect_err("forged snapshot should fail validation");
    assert!(
        error
            .to_string()
            .contains("promise")
            && error.to_string().contains("combinator index"),
        "unexpected error: {error}"
    );
}
