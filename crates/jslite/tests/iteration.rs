use indexmap::IndexMap;

use jslite::{
    ExecutionOptions, ExecutionStep, ResumeOptions, ResumePayload, RuntimeLimits, SnapshotPolicy,
    StructuredValue, compile, dump_snapshot, execute, load_snapshot, resume, resume_with_options,
    start,
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

#[test]
fn array_for_of_preserves_index_order_and_observes_growth() {
    let program = compile(
        r#"
        const values = [1, 2];
        let seen = [];
        for (let value of values) {
          seen[seen.length] = value;
          if (value === 1) {
            values[values.length] = 3;
          }
        }
        seen;
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![number(1.0), number(2.0), number(3.0)])
    );
}

#[test]
fn for_of_supports_destructuring_and_fresh_iteration_bindings() {
    let program = compile(
        r#"
        const fns = [];
        for (const [value] of [[1], [2]]) {
          fns[fns.length] = () => value;
        }
        [fns[0](), fns[1]()];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![number(1.0), number(2.0)])
    );
}

#[test]
fn for_of_supports_identifier_assignment_targets() {
    let program = compile(
        r#"
        let value = 0;
        const fns = [];
        for (value of [1, 2]) {
          fns[fns.length] = () => value;
        }
        [fns[0](), fns[1](), value];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![number(2.0), number(2.0), number(2.0)])
    );
}

#[test]
fn for_of_supports_member_assignment_targets() {
    let program = compile(
        r#"
        const boxes = [{ current: 0 }, { current: 0 }];
        let index = 0;
        for (boxes[index].current of [3, 4]) {
          index += 1;
        }
        [boxes[0].current, boxes[1].current, index];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![number(3.0), number(4.0), number(2.0)])
    );
}

#[test]
fn for_of_runs_finally_blocks_on_continue_and_break() {
    let program = compile(
        r#"
        let total = 0;
        let events = [];
        for (const value of [1, 2, 3, 4]) {
          try {
            if (value === 2) {
              continue;
            }
            if (value === 4) {
              break;
            }
            total += value;
          } finally {
            events[events.length] = value;
          }
        }
        ({ total, events });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            ("total".to_string(), number(4.0)),
            (
                "events".to_string(),
                StructuredValue::Array(vec![number(1.0), number(2.0), number(3.0), number(4.0),]),
            ),
        ]))
    );
}

#[test]
fn for_of_supports_strings_maps_sets_and_iterator_helpers() {
    let program = compile(
        r#"
        const map = new Map([['alpha', 1], ['beta', 2]]);
        const set = new Set('aba');
        const seen = [];
        for (const [key, value] of map) {
          seen[seen.length] = key + ':' + value;
        }
        let chars = '';
        for (const value of 'hi') {
          chars += value;
        }
        let setChars = '';
        for (const value of set.keys()) {
          setChars += value;
        }
        const pair = [10, 20].entries().next();
        [seen, chars, setChars, pair.value[0], pair.value[1], pair.done];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![string("alpha:1"), string("beta:2")]),
            string("hi"),
            string("ab"),
            number(0.0),
            number(10.0),
            StructuredValue::Bool(false),
        ])
    );
}

#[test]
fn for_of_rejects_unsupported_iterable_inputs() {
    let program = compile(
        r#"
        for (const value of { alpha: 1 }) {
          value;
        }
        "#,
    )
    .expect("source should compile");

    let error = execute(&program, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("value is not iterable in the supported surface")
    );
}

#[test]
fn snapshot_round_trip_preserves_active_array_iterators() {
    let program = compile(
        r#"
        let total = 0;
        for (const value of [1, 2, 3]) {
          total += fetch_data(value);
        }
        total;
        "#,
    )
    .expect("source should compile");

    let first = match start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("start should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
    };
    assert_eq!(first.capability, "fetch_data");
    assert_eq!(first.args, vec![number(1.0)]);

    let encoded = dump_snapshot(&first.snapshot).expect("snapshot should serialize");
    let loaded = load_snapshot(&encoded).expect("snapshot should deserialize");

    let second = match resume_with_options(
        loaded,
        ResumePayload::Value(number(10.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(&["fetch_data"], RuntimeLimits::default())),
        },
    )
    .expect("resume should work")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected a second suspension, got {value:?}"),
    };
    assert_eq!(second.capability, "fetch_data");
    assert_eq!(second.args, vec![number(2.0)]);

    let third = match resume(second.snapshot, ResumePayload::Value(number(20.0)))
        .expect("second resume should work")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected a third suspension, got {value:?}"),
    };
    assert_eq!(third.capability, "fetch_data");
    assert_eq!(third.args, vec![number(3.0)]);

    let completed = resume(third.snapshot, ResumePayload::Value(number(30.0)))
        .expect("final resume should work");
    match completed {
        ExecutionStep::Completed(value) => assert_eq!(value, number(60.0)),
        ExecutionStep::Suspended(other) => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn snapshot_round_trip_preserves_assignment_target_for_of_headers() {
    let program = compile(
        r#"
        const state = { current: 0, total: 0 };
        for (state.current of [1, 2, 3]) {
          state.total += fetch_data(state.current);
        }
        [state.current, state.total];
        "#,
    )
    .expect("source should compile");

    let first = match start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("start should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
    };
    assert_eq!(first.capability, "fetch_data");
    assert_eq!(first.args, vec![number(1.0)]);

    let encoded = dump_snapshot(&first.snapshot).expect("snapshot should serialize");
    let loaded = load_snapshot(&encoded).expect("snapshot should deserialize");

    let second = match resume_with_options(
        loaded,
        ResumePayload::Value(number(10.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(&["fetch_data"], RuntimeLimits::default())),
        },
    )
    .expect("resume should work")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected a second suspension, got {value:?}"),
    };
    assert_eq!(second.capability, "fetch_data");
    assert_eq!(second.args, vec![number(2.0)]);

    let third = match resume(second.snapshot, ResumePayload::Value(number(20.0)))
        .expect("second resume should work")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected a third suspension, got {value:?}"),
    };
    assert_eq!(third.capability, "fetch_data");
    assert_eq!(third.args, vec![number(3.0)]);

    let completed = resume(third.snapshot, ResumePayload::Value(number(30.0)))
        .expect("final resume should work");
    match completed {
        ExecutionStep::Completed(value) => assert_eq!(
            value,
            StructuredValue::Array(vec![number(3.0), number(60.0)])
        ),
        ExecutionStep::Suspended(other) => panic!("expected completion, got {other:?}"),
    }
}

fn string(value: &str) -> StructuredValue {
    StructuredValue::from(value)
}
