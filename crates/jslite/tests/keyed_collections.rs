use indexmap::IndexMap;

use jslite::{
    ExecutionOptions, ExecutionStep, ResumeOptions, RuntimeLimits, SnapshotPolicy, StructuredValue,
    compile, dump_snapshot, execute, load_snapshot, resume_with_options, start,
};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

fn string(value: &str) -> StructuredValue {
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
fn map_supports_same_value_zero_identity_and_mutation_operations() {
    let program = compile(
        r#"
        const shared = {};
        const nan = Number('nope');
        const map = new Map();
        map.set('alpha', 1);
        map.set(nan, 'nan');
        map.set(-0, 'zero');
        map.set(shared, 7);
        map.set('alpha', 2);
        [
          map.size,
          map.get('alpha'),
          map.has('alpha'),
          map.get(nan),
          map.has(0),
          map.get(0),
          map.get(-0),
          map.get(shared),
          map.delete('missing'),
          map.delete(nan),
          map.has(nan),
          map.size,
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(4.0),
            number(2.0),
            StructuredValue::Bool(true),
            string("nan"),
            StructuredValue::Bool(true),
            string("zero"),
            string("zero"),
            number(7.0),
            StructuredValue::Bool(false),
            StructuredValue::Bool(true),
            StructuredValue::Bool(false),
            number(3.0),
        ])
    );
}

#[test]
fn set_supports_same_value_zero_and_clear_operations() {
    let program = compile(
        r#"
        const shared = {};
        const nan = Number('nope');
        const set = new Set();
        set.add('alpha');
        set.add(nan);
        set.add(-0);
        set.add(shared);
        set.add(nan);
        set.add(0);
        const before = [
          set.size,
          set.has(nan),
          set.has(0),
          set.has(-0),
          set.has(shared),
        ];
        const removed = [
          set.delete('missing'),
          set.delete(nan),
          set.has(nan),
          set.size,
        ];
        set.clear();
        [before, removed, set.size, set.has(shared)];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                number(4.0),
                StructuredValue::Bool(true),
                StructuredValue::Bool(true),
                StructuredValue::Bool(true),
                StructuredValue::Bool(true),
            ]),
            StructuredValue::Array(vec![
                StructuredValue::Bool(false),
                StructuredValue::Bool(true),
                StructuredValue::Bool(false),
                number(3.0),
            ]),
            number(0.0),
            StructuredValue::Bool(false),
        ])
    );
}

#[test]
fn collection_methods_require_compatible_receivers() {
    let program = compile(
        r#"
        const map = new Map();
        const set = new Set();
        const mapGet = map.get;
        const setAdd = set.add;
        [
          (() => {
            try {
              mapGet('alpha');
              return 'unreachable';
            } catch (error) {
              return [error.name, error.message];
            }
          })(),
          (() => {
            try {
              setAdd(1);
              return 'unreachable';
            } catch (error) {
              return [error.name, error.message];
            }
          })(),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                string("TypeError"),
                string("Map.prototype.get called on incompatible receiver"),
            ]),
            StructuredValue::Array(vec![
                string("TypeError"),
                string("Set.prototype.add called on incompatible receiver"),
            ]),
        ])
    );
}

#[test]
fn keyed_collections_support_iterable_inputs_and_iteration_helpers() {
    let program = compile(
        r#"
        const map = new Map([['alpha', 1], ['beta', 2], ['alpha', 3]]);
        const set = new Set('abba');
        const entry = map.entries().next();
        const key = map.keys().next();
        const value = map.values().next();
        const setEntry = set.entries().next();
        const seen = [];
        for (const [itemKey, itemValue] of map) {
          seen[seen.length] = itemKey + ':' + itemValue;
        }
        let setSeen = '';
        for (const item of set) {
          setSeen += item;
        }
        [
          map.size,
          map.get('alpha'),
          set.size,
          entry.value[0],
          entry.value[1],
          entry.done,
          key.value,
          value.value,
          setEntry.value[0],
          setEntry.value[1],
          setSeen,
          seen,
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(2.0),
            number(3.0),
            number(2.0),
            string("alpha"),
            number(3.0),
            StructuredValue::Bool(false),
            string("alpha"),
            number(3.0),
            string("a"),
            string("a"),
            string("ab"),
            StructuredValue::Array(vec![string("alpha:3"), string("beta:2")]),
        ])
    );
}

#[test]
fn keyed_collection_iterators_visit_entries_appended_during_active_iteration() {
    let program = compile(
        r#"
        const map = new Map([
          ['alpha', 1],
          ['omega', 2],
        ]);
        const seen = [];
        for (const [key, value] of map) {
          seen[seen.length] = [key, value];
          if (key === 'alpha') {
            map.set('tail', 3);
          }
          if (key === 'omega') {
            map.delete('alpha');
          }
        }

        const set = new Set(['alpha', 'omega']);
        const setSeen = [];
        for (const value of set) {
          setSeen[setSeen.length] = value;
          if (value === 'alpha') {
            set.add('tail');
          }
          if (value === 'omega') {
            set.delete('alpha');
          }
        }

        [seen, Array.from(map.entries()), setSeen, Array.from(set.values())];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                StructuredValue::Array(vec![string("alpha"), number(1.0)]),
                StructuredValue::Array(vec![string("omega"), number(2.0)]),
                StructuredValue::Array(vec![string("tail"), number(3.0)]),
            ]),
            StructuredValue::Array(vec![
                StructuredValue::Array(vec![string("omega"), number(2.0)]),
                StructuredValue::Array(vec![string("tail"), number(3.0)]),
            ]),
            StructuredValue::Array(vec![string("alpha"), string("omega"), string("tail")]),
            StructuredValue::Array(vec![string("omega"), string("tail")]),
        ])
    );
}

#[test]
fn keyed_collection_iterators_continue_after_clear_followed_by_new_entries() {
    let program = compile(
        r#"
        const map = new Map([
          ['alpha', 1],
          ['omega', 2],
        ]);
        const seen = [];
        for (const [key, value] of map) {
          seen[seen.length] = [key, value];
          if (key === 'alpha') {
            map.clear();
            map.set('tail', 3);
          }
        }

        const set = new Set(['alpha', 'omega']);
        const setSeen = [];
        for (const value of set) {
          setSeen[setSeen.length] = value;
          if (value === 'alpha') {
            set.clear();
            set.add('tail');
          }
        }

        [seen, Array.from(map.entries()), setSeen, Array.from(set.values())];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                StructuredValue::Array(vec![string("alpha"), number(1.0)]),
                StructuredValue::Array(vec![string("tail"), number(3.0)]),
            ]),
            StructuredValue::Array(vec![StructuredValue::Array(vec![
                string("tail"),
                number(3.0),
            ])]),
            StructuredValue::Array(vec![string("alpha"), string("tail")]),
            StructuredValue::Array(vec![string("tail")]),
        ])
    );
}

#[test]
fn maps_and_sets_reject_structured_host_boundary_crossing() {
    let output = compile(
        r#"
        const map = new Map();
        map.set('alpha', 1);
        map;
        "#,
    )
    .expect("source should compile");
    let error = execute(&output, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Map and Set values cannot cross the structured host boundary")
    );

    let capability = compile(
        r#"
        const set = new Set();
        set.add(1);
        sink(set);
        "#,
    )
    .expect("source should compile");
    let error = start(
        &capability,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["sink".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect_err("start should reject map/set arguments before suspension");
    assert!(
        error
            .to_string()
            .contains("Map and Set values cannot cross the structured host boundary")
    );
}

#[test]
fn snapshots_preserve_keyed_collections_and_cycles() {
    let program = compile(
        r#"
        const key = { label: 'shared' };
        const map = new Map();
        const set = new Set();
        map.set('count', 1);
        map.set(key, set);
        set.add(key);
        set.add(map);
        const value = fetch_data(41);
        map.set('count', value);
        ({
          count: map.get('count'),
          hasKey: map.has(key),
          setHasMap: set.has(map),
          setSize: set.size,
          mapSize: map.size,
        });
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
    assert_eq!(first.args, vec![number(41.0)]);

    let encoded = dump_snapshot(&first.snapshot).expect("snapshot should serialize");
    let loaded = load_snapshot(&encoded).expect("snapshot should deserialize");

    let completed = resume_with_options(
        loaded,
        jslite::ResumePayload::Value(number(41.0)),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(&["fetch_data"], RuntimeLimits::default())),
        },
    )
    .expect("resume should work");
    match completed {
        ExecutionStep::Completed(value) => assert_eq!(
            value,
            StructuredValue::Object(IndexMap::from([
                ("count".to_string(), number(41.0)),
                ("hasKey".to_string(), StructuredValue::Bool(true)),
                ("setHasMap".to_string(), StructuredValue::Bool(true)),
                ("setSize".to_string(), number(2.0)),
                ("mapSize".to_string(), number(2.0)),
            ]))
        ),
        ExecutionStep::Suspended(other) => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn keyed_collection_cycles_and_clear_delete_behavior_survive_heap_pressure() {
    let program = compile(
        r#"
        let total = 0;
        for (let i = 0; i < 80; i += 1) {
          let map = new Map();
          let set = new Set();
          let key = { index: i };
          map.set(key, set);
          set.add(map);
          set.add(key);
          total += map.size + set.size;
          map.delete(key);
          set.delete(map);
          set.clear();
        }
        total;
        "#,
    )
    .expect("source should compile");

    let value = execute(
        &program,
        ExecutionOptions {
            limits: RuntimeLimits {
                instruction_budget: 20_000,
                heap_limit_bytes: 24 * 1024,
                allocation_budget: 512,
                ..RuntimeLimits::default()
            },
            cancellation_token: None,
            ..ExecutionOptions::default()
        },
    )
    .expect("gc should reclaim keyed-collection cycles under pressure");
    assert_eq!(value, number(240.0));
}
