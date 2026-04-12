use indexmap::IndexMap;

use jslite::{
    ExecutionOptions, ExecutionStep, ResumeOptions, ResumePayload, RuntimeLimits, SnapshotPolicy,
    StructuredValue, compile, dump_snapshot, inspect_snapshot, load_snapshot, resume,
    resume_with_options, start,
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
