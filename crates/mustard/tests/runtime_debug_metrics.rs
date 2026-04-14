use indexmap::IndexMap;
use std::sync::Arc;

use mustard::{
    ExecutionOptions, ExecutionStep, RuntimeLimits, StructuredValue, compile, lower_to_bytecode,
    start_shared_bytecode_with_metrics,
};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
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
