use super::*;

#[test]
fn round_trips_program_and_snapshot() {
    let program =
        compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let program_bytes = dump_program(&bytecode).expect("program dump should succeed");
    let loaded_program = load_program(&program_bytes).expect("program load should succeed");
    assert_eq!(loaded_program.root, bytecode.root);
    assert_eq!(loaded_program.functions.len(), bytecode.functions.len());

    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend");
    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };
    let snapshot_bytes = dump_snapshot(&suspension.snapshot).expect("snapshot dump should succeed");
    let loaded_snapshot = load_snapshot(&snapshot_bytes).expect("snapshot load should succeed");
    let resumed = resume(
        loaded_snapshot,
        ResumePayload::Value(StructuredValue::Number(StructuredNumber::Finite(1.0))),
    )
    .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Number(StructuredNumber::Finite(3.0))
            );
        }
        other => panic!("expected completion, got {other:?}"),
    }
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
    let program =
        compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend");
    let mut suspension = match step {
        ExecutionStep::Suspended(suspension) => *suspension,
        other => panic!("expected suspension, got {other:?}"),
    };
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
fn rejects_cross_version_snapshots() {
    let program =
        compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend");
    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };

    let mut bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    bytes[0] = bytes[0].saturating_add(1);
    let error = load_snapshot(&bytes).expect_err("cross-version snapshot should be rejected");
    assert!(
        error
            .to_string()
            .contains("serialized snapshot version mismatch")
    );
}
