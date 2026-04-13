use indexmap::IndexMap;
use std::sync::Arc;

use mustard::{
    ExecutionOptions, ExecutionSnapshot, ExecutionStep, ResumeOptions, ResumePayload,
    RuntimeLimits, SnapshotPolicy, StructuredValue, compile, dump_snapshot, inspect_snapshot,
    load_snapshot, lower_to_bytecode, resume, resume_with_options, start,
    start_shared_bytecode_with_metrics,
};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

fn snapshot_policy(capabilities: &[&str], limits: RuntimeLimits) -> SnapshotPolicy {
    SnapshotPolicy {
        capabilities: capabilities
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        limits,
    }
}

fn serialized_suspension(source: &str, limits: RuntimeLimits) -> Vec<u8> {
    let program = compile(source).expect("source should compile");
    let step = start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits,
            cancellation_token: None,
        },
    )
    .expect("program should suspend");
    let suspension = match step {
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
        ExecutionStep::Suspended(suspension) => suspension,
    };
    dump_snapshot(&suspension.snapshot).expect("snapshot should serialize")
}

fn replace_all_ascii(buffer: &mut [u8], from: &str, to: &str) {
    let from = from.as_bytes();
    let to = to.as_bytes();
    assert_eq!(from.len(), to.len(), "replacement must preserve length");

    let mut index = 0usize;
    while index + from.len() <= buffer.len() {
        if &buffer[index..index + from.len()] == from {
            buffer[index..index + to.len()].copy_from_slice(to);
            index += from.len();
        } else {
            index += 1;
        }
    }
}

#[test]
fn loaded_snapshots_require_explicit_policy_before_resume() {
    let bytes = serialized_suspension(
        "const value = fetch_data(1); value + 1;",
        RuntimeLimits::default(),
    );
    let snapshot = load_snapshot(&bytes).expect("snapshot should deserialize");
    let error = resume(snapshot, ResumePayload::Value(number(1.0)))
        .expect_err("resume should fail closed without policy");
    assert!(
        error
            .to_string()
            .contains("loaded snapshots require explicit host policy"),
        "unexpected error: {error}"
    );
}

#[test]
fn inspect_snapshot_derives_metadata_and_rejects_unauthorized_capabilities() {
    let bytes = serialized_suspension(
        "const value = fetch_data(7); value + 1;",
        RuntimeLimits::default(),
    );

    let mut unauthorized = load_snapshot(&bytes).expect("snapshot should deserialize");
    let error = inspect_snapshot(
        &mut unauthorized,
        snapshot_policy(&["drop_table"], RuntimeLimits::default()),
    )
    .expect_err("unauthorized capability should be rejected");
    assert!(
        error
            .to_string()
            .contains("snapshot policy rejected unauthorized capability `fetch_data`"),
        "unexpected error: {error}"
    );

    let mut authorized = load_snapshot(&bytes).expect("snapshot should deserialize");
    let inspection = inspect_snapshot(
        &mut authorized,
        snapshot_policy(&["fetch_data"], RuntimeLimits::default()),
    )
    .expect("authorized snapshot should inspect");
    assert_eq!(inspection.capability, "fetch_data");
    assert_eq!(inspection.args, vec![number(7.0)]);
}

#[test]
fn inspect_snapshot_reports_runtime_metrics_from_suspended_start() {
    let program =
        compile("const value = fetch_data(7); value + 1;").expect("source should compile");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let (step, start_metrics) = start_shared_bytecode_with_metrics(
        Arc::new(bytecode),
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should start");
    let snapshot = match step {
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
        ExecutionStep::Suspended(suspension) => suspension.snapshot,
    };

    let mut snapshot = snapshot;
    let inspection = inspect_snapshot(
        &mut snapshot,
        snapshot_policy(&["fetch_data"], RuntimeLimits::default()),
    )
    .expect("authorized snapshot should inspect");

    assert_eq!(inspection.metrics, start_metrics);
}

#[test]
fn loaded_snapshots_reject_closure_retained_capabilities_outside_restore_policy() {
    let program = compile(
        r#"
            function helper() {}
            helper.backdoor = drop_table;
            const value = fetch_data(1);
            helper.backdoor("boom");
        "#,
    )
    .expect("source should compile");
    let step = start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string(), "drop_table".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should suspend");
    let snapshot = match step {
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
        ExecutionStep::Suspended(suspension) => suspension.snapshot,
    };

    let error = resume_with_options(
        snapshot,
        ResumePayload::Value(number(1.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(&["fetch_data"], RuntimeLimits::default())),
        },
    )
    .expect_err("restore policy should reject hidden unauthorized capabilities");
    assert!(
        error
            .to_string()
            .contains("snapshot policy rejected unauthorized capability `drop_table`"),
        "unexpected error: {error}"
    );
}

#[test]
fn forged_snapshots_cannot_switch_to_unauthorized_capabilities() {
    let mut bytes = serialized_suspension(
        "const first = fetch_data(1); const second = fetch_data(2); [first, second];",
        RuntimeLimits::default(),
    );
    replace_all_ascii(&mut bytes, "fetch_data", "drop_table");

    let snapshot = load_snapshot(&bytes).expect("mutated snapshot should deserialize");
    let error = resume_with_options(
        snapshot,
        ResumePayload::Value(number(1.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(&["fetch_data"], RuntimeLimits::default())),
        },
    )
    .expect_err("unauthorized capability should be rejected");
    assert!(
        error
            .to_string()
            .contains("snapshot policy rejected unauthorized capability `drop_table`"),
        "unexpected error: {error}"
    );
}

#[test]
fn loaded_snapshots_reapply_host_limits_before_resume() {
    let source = r#"
        const ready = fetch_data(1);
        let total = 0;
        for (let i = 0; i < 10000; i = i + 1) {
          total = total + 1;
        }
        total;
    "#;
    let bytes = serialized_suspension(
        source,
        RuntimeLimits {
            instruction_budget: 5_000_000,
            ..RuntimeLimits::default()
        },
    );
    let snapshot = load_snapshot(&bytes).expect("snapshot should deserialize");
    let error = resume_with_options(
        snapshot,
        ResumePayload::Value(number(1.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(
                &["fetch_data"],
                RuntimeLimits {
                    instruction_budget: 50,
                    ..RuntimeLimits::default()
                },
            )),
        },
    )
    .expect_err("lower host budget should win on resume");
    assert!(
        error.to_string().contains("instruction budget exhausted"),
        "unexpected error: {error}"
    );
}

#[test]
fn loaded_snapshots_reapply_heap_limits_before_resume() {
    let bytes = serialized_suspension(
        "const payload = [1, 2, 3, 4]; const value = fetch_data(payload.length); value + payload.length;",
        RuntimeLimits::default(),
    );
    let snapshot = load_snapshot(&bytes).expect("snapshot should deserialize");
    let error = resume_with_options(
        snapshot,
        ResumePayload::Value(number(4.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(
                &["fetch_data"],
                RuntimeLimits {
                    heap_limit_bytes: 0,
                    ..RuntimeLimits::default()
                },
            )),
        },
    )
    .expect_err("lower heap limit should fail closed before resume");
    assert!(
        error
            .to_string()
            .contains("snapshot validation failed: heap usage exceeds configured heap limit"),
        "unexpected error: {error}"
    );
}

#[test]
fn loaded_snapshots_reapply_allocation_limits_before_resume() {
    let bytes = serialized_suspension(
        "const payload = [1, 2, 3, 4]; const value = fetch_data(payload.length); value + payload.length;",
        RuntimeLimits::default(),
    );
    let snapshot = load_snapshot(&bytes).expect("snapshot should deserialize");
    let error = resume_with_options(
        snapshot,
        ResumePayload::Value(number(4.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(
                &["fetch_data"],
                RuntimeLimits {
                    allocation_budget: 0,
                    ..RuntimeLimits::default()
                },
            )),
        },
    )
    .expect_err("lower allocation budget should fail closed before resume");
    assert!(
        error.to_string().contains(
            "snapshot validation failed: allocation count exceeds configured allocation budget"
        ),
        "unexpected error: {error}"
    );
}

#[test]
fn loaded_snapshots_still_reclaim_cyclic_garbage_under_pressure_after_restore() {
    let source = r#"
        const seed = fetch_data(1);
        let total = 0;
        for (let index = 0; index < 120; index += 1) {
          const left = {};
          const right = {};
          left.peer = right;
          right.peer = left;
          total = total + index;
        }
        seed + total;
    "#;
    let limits = RuntimeLimits {
        heap_limit_bytes: 24 * 1024,
        allocation_budget: 256,
        instruction_budget: 1_000_000,
        ..RuntimeLimits::default()
    };
    let bytes = serialized_suspension(source, limits);
    let snapshot = load_snapshot(&bytes).expect("snapshot should deserialize");
    let resumed = resume_with_options(
        snapshot,
        ResumePayload::Value(number(1.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(&["fetch_data"], limits)),
        },
    )
    .expect("restored execution should still reclaim cyclic garbage under pressure");

    match resumed {
        ExecutionStep::Completed(value) => assert_eq!(value, number(7141.0)),
        ExecutionStep::Suspended(other) => {
            panic!("expected completion after restore, got {other:?}")
        }
    }
}

#[test]
fn direct_execution_snapshot_deserialize_requires_explicit_policy_before_resume() {
    let source = "const value = fetch_data(1); value + 1;";
    let bytes = {
        let program = compile(source).expect("source should compile");
        let step = start(
            &program,
            ExecutionOptions {
                inputs: IndexMap::new(),
                capabilities: vec!["fetch_data".to_string()],
                limits: RuntimeLimits::default(),
                cancellation_token: None,
            },
        )
        .expect("program should suspend");
        let suspension = match step {
            ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
            ExecutionStep::Suspended(suspension) => suspension,
        };
        bincode::serialize(&suspension.snapshot).expect("snapshot should serialize directly")
    };

    let snapshot: ExecutionSnapshot =
        bincode::deserialize(&bytes).expect("snapshot should deserialize directly");
    let error = resume(snapshot, ResumePayload::Value(number(1.0)))
        .expect_err("directly deserialized snapshots should still require policy");
    assert!(
        error
            .to_string()
            .contains("loaded snapshots require explicit host policy"),
        "unexpected error: {error}"
    );
}
