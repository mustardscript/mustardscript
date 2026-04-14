use indexmap::IndexMap;
use std::sync::Arc;

use mustard::{
    ExecutionOptions, ExecutionStep, RuntimeLimits, StructuredValue, compile, lower_to_bytecode,
    start_shared_bytecode_with_metrics,
};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

fn shaped_rows_input() -> StructuredValue {
    StructuredValue::Array(vec![
        StructuredValue::Object(IndexMap::from([
            ("foo".to_string(), number(1.0)),
            ("bar".to_string(), number(10.0)),
        ])),
        StructuredValue::Object(IndexMap::from([
            ("foo".to_string(), number(2.0)),
            ("bar".to_string(), number(20.0)),
        ])),
        StructuredValue::Object(IndexMap::from([
            ("foo".to_string(), number(3.0)),
            ("bar".to_string(), number(30.0)),
        ])),
    ])
}

#[test]
fn runtime_debug_metrics_track_ptc_relevant_operations() {
    let program = compile(
        r#"
        const key = "bar";
        const row = { foo: 1, bar: 2 };
        const values = [3, 1, 2];
        const map = new Map();
        map.set("foo", row.foo);
        const set = new Set();
        set.add(row[key]);
        const lower = "FOO-bar".toLowerCase();
        const includes = lower.includes("foo");
        const literalMatch = lower.match("bar");
        const replaced = lower.replace(/bar/g, "baz");
        values.sort((left, right) => left - right);
        [
          row.foo,
          row[key],
          map.get("foo"),
          set.has(2),
          includes,
          literalMatch.length,
          replaced,
          values[0],
        ];
        "#,
    )
    .expect("source should compile");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let (step, metrics) = start_shared_bytecode_with_metrics(
        Arc::new(bytecode),
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should execute");

    match step {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Array(vec![
                    number(1.0),
                    number(2.0),
                    number(1.0),
                    StructuredValue::Bool(true),
                    StructuredValue::Bool(true),
                    number(1.0),
                    StructuredValue::from("foo-baz"),
                    number(1.0),
                ])
            );
        }
        ExecutionStep::Suspended(_) => panic!("program should not suspend"),
    }

    assert!(metrics.static_property_reads > 0);
    assert!(metrics.computed_property_reads > 0);
    assert!(metrics.dynamic_instructions > 0);
    assert!(metrics.object_allocations > 0);
    assert!(metrics.array_allocations > 0);
    assert!(metrics.map_get_calls > 0);
    assert!(metrics.map_set_calls > 0);
    assert!(metrics.set_add_calls > 0);
    assert!(metrics.set_has_calls > 0);
    assert!(metrics.string_case_conversions > 0);
    assert!(metrics.literal_string_searches > 0);
    assert!(metrics.regex_search_or_replacements > 0);
    assert!(metrics.comparator_sort_invocations > 0);
}

#[test]
fn shape_backed_host_rows_feed_property_inline_cache_metrics() {
    let program = compile(
        r#"
        let total = 0;
        for (const row of rows) {
          total += row.foo;
        }
        total;
        "#,
    )
    .expect("source should compile");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let (step, metrics) = start_shared_bytecode_with_metrics(
        Arc::new(bytecode),
        ExecutionOptions {
            inputs: IndexMap::from([("rows".to_string(), shaped_rows_input())]),
            capabilities: Vec::new(),
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should execute");

    match step {
        ExecutionStep::Completed(value) => assert_eq!(value, number(6.0)),
        ExecutionStep::Suspended(_) => panic!("program should not suspend"),
    }

    assert!(metrics.static_property_reads >= 3);
    assert!(metrics.property_ic_misses > 0);
    assert!(metrics.property_ic_hits > 0);
    assert_eq!(metrics.property_ic_deopts, 0);
}

#[test]
fn shape_backed_rows_fall_back_for_computed_access_and_mutation() {
    let program = compile(
        r#"
        const key = "foo";
        const first = rows[0][key];
        rows[1].foo = rows[1].foo + 5;
        [first, rows[1].foo];
        "#,
    )
    .expect("source should compile");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let (step, metrics) = start_shared_bytecode_with_metrics(
        Arc::new(bytecode),
        ExecutionOptions {
            inputs: IndexMap::from([("rows".to_string(), shaped_rows_input())]),
            capabilities: Vec::new(),
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should execute");

    match step {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Array(vec![number(1.0), number(7.0)])
            );
        }
        ExecutionStep::Suspended(_) => panic!("program should not suspend"),
    }

    assert!(metrics.computed_property_reads > 0);
    assert!(metrics.property_ic_deopts > 0);
}
