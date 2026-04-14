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
        const tokenMatch = "jwks timeout dns".match(/jwks|timeout|dns/g);
        const replaced = lower.replace(/bar/g, "baz");
        const compact = "A\tB\nC".replaceAll(/\s+/g, " ");
        const filtered = "a?!b".replaceAll(/[^a-z0-9 ]+/g, " ");
        values.sort((left, right) => left - right);
        [
          row.foo,
          row[key],
          map.get("foo"),
          set.has(2),
          includes,
          literalMatch.length,
          tokenMatch.length,
          replaced,
          compact,
          filtered,
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
                    number(3.0),
                    StructuredValue::from("foo-baz"),
                    StructuredValue::from("A B C"),
                    StructuredValue::from("a b"),
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
    assert!(metrics.ascii_token_regex_fast_path_hits > 0);
    assert!(metrics.comparator_sort_invocations > 0);
}

#[test]
fn non_ascii_string_paths_preserve_results() {
    let program = compile(
        r#"
        const lower = "CAFÉ".toLowerCase();
        const includes = lower.includes("fé");
        const compact = "CAFÉ\n".replaceAll(/\s+/g, " ");
        const tokens = "CAFÉ timeout".toLowerCase().match(/caf|timeout/g);
        [lower, includes, compact, tokens.length];
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
                    StructuredValue::from("café"),
                    StructuredValue::Bool(true),
                    StructuredValue::from("CAFÉ "),
                    number(2.0),
                ])
            );
        }
        ExecutionStep::Suspended(_) => panic!("program should not suspend"),
    }

    assert!(metrics.string_case_conversions > 0);
    assert!(metrics.literal_string_searches > 0);
    assert!(metrics.regex_search_or_replacements > 0);
    assert!(metrics.ascii_token_regex_fast_path_fallbacks > 0);
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

#[test]
fn collection_debug_metrics_capture_hottest_call_sites() {
    let program = compile(
        r#"
        const map = new Map([
          ["a", 1],
          ["b", 2],
        ]);
        const set = new Set(["seed"]);
        let total = 0;
        for (const key of ["a", "b"]) {
          total += map.get(key);
        }
        for (const value of ["seed", "fresh"]) {
          set.add(value);
        }
        map.set("c", total);
        [total, set.has("fresh"), map.get("c")];
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
                StructuredValue::Array(
                    vec![number(3.0), StructuredValue::Bool(true), number(3.0),]
                )
            );
        }
        ExecutionStep::Suspended(_) => panic!("program should not suspend"),
    }

    assert_eq!(
        metrics
            .collection_call_sites
            .iter()
            .map(|site| site.map_get_calls)
            .sum::<u64>(),
        3
    );
    assert_eq!(
        metrics
            .collection_call_sites
            .iter()
            .map(|site| site.map_set_calls)
            .sum::<u64>(),
        1
    );
    assert_eq!(
        metrics
            .collection_call_sites
            .iter()
            .map(|site| site.set_add_calls)
            .sum::<u64>(),
        2
    );
    assert_eq!(
        metrics
            .collection_call_sites
            .iter()
            .map(|site| site.set_has_calls)
            .sum::<u64>(),
        1
    );
    assert!(
        metrics
            .collection_call_sites
            .iter()
            .all(|site| { site.span.end > site.span.start && site.total_calls() > 0 })
    );
    assert_eq!(
        metrics.collection_call_sites[0].map_get_calls, 2,
        "the hottest site should be the looped map.get call"
    );
}
