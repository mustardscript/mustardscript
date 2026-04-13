use mustard::runtime::Instruction;
use mustard::{
    ExecutionOptions, ExecutionStep, RuntimeLimits, StructuredValue, compile, execute,
    lower_to_bytecode, start,
};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

#[test]
fn nullish_assignment_preserves_existing_identifier_values() {
    let program = compile(
        r#"
        let value = 3;
        value ??= 9;
        value;
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(result, number(3.0));
}

#[test]
fn nullish_assignment_writes_identifiers_and_members_only_when_needed() {
    let program = compile(
        r#"
        let missing;
        missing ??= 7;
        const box = { present: 5, absent: undefined };
        box.present ??= 9;
        box.absent ??= 11;
        const key = "dynamic";
        box[key] ??= 13;
        [missing, box.present, box.absent, box.dynamic];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![number(7.0), number(5.0), number(11.0), number(13.0)])
    );
}

#[test]
fn call_depth_limit_is_enforced() {
    let program = compile(
        r#"
        function recurse(value) {
          if (value === 0) {
            return 0;
          }
          return recurse(value - 1);
        }
        recurse(3);
        "#,
    )
    .expect("source should compile");

    let error = execute(
        &program,
        ExecutionOptions {
            limits: RuntimeLimits {
                call_depth_limit: 3,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("execution should fail once the call depth limit is exceeded");

    assert!(error.to_string().contains("call depth limit exceeded"));
}

#[test]
fn cached_startup_image_still_fails_closed_on_zero_limits() {
    let program = compile("0;").expect("source should compile");

    let heap_error = execute(
        &program,
        ExecutionOptions {
            limits: RuntimeLimits {
                heap_limit_bytes: 0,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("startup heap usage should still count against zero-byte limits");
    assert!(heap_error.to_string().contains("heap limit exceeded"));

    let allocation_error = execute(
        &program,
        ExecutionOptions {
            limits: RuntimeLimits {
                allocation_budget: 0,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("startup allocations should still count against zero-allocation limits");
    assert!(
        allocation_error
            .to_string()
            .contains("allocation budget exhausted")
    );
}

#[test]
fn fresh_executions_do_not_share_mutated_global_or_builtin_state() {
    let mutator = compile(
        r#"
        globalThis.cachedFlag = 1;
        Math.cachedFlag = 2;
        0;
        "#,
    )
    .expect("source should compile");
    execute(&mutator, ExecutionOptions::default()).expect("mutating run should complete");

    let reader =
        compile("[globalThis.cachedFlag, Math.cachedFlag];").expect("source should compile");
    let result = execute(&reader, ExecutionOptions::default())
        .expect("fresh run should not observe prior runtime mutations");
    assert_eq!(
        result,
        StructuredValue::Array(vec![StructuredValue::Undefined, StructuredValue::Undefined])
    );
}

#[test]
fn conditional_expressions_do_not_corrupt_enclosing_object_literals() {
    let program = compile(
        r#"
        const customer = { arrUsd: 540000, openEscalations: 1 };
        const noteHits = [1];
        const actionQueue = [
          { label: "page", priority: 75 },
          { label: "notify", priority: customer.arrUsd > 250000 ? 95 : 60 },
          { label: "stage", priority: noteHits.length > 0 || customer.openEscalations > 0 ? 90 : 40 },
          { label: "offer", priority: noteHits.length > 0 ? 80 : 20 },
        ];
        actionQueue;
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Object(indexmap::IndexMap::from([
                ("label".to_string(), "page".into()),
                ("priority".to_string(), number(75.0)),
            ])),
            StructuredValue::Object(indexmap::IndexMap::from([
                ("label".to_string(), "notify".into()),
                ("priority".to_string(), number(95.0)),
            ])),
            StructuredValue::Object(indexmap::IndexMap::from([
                ("label".to_string(), "stage".into()),
                ("priority".to_string(), number(90.0)),
            ])),
            StructuredValue::Object(indexmap::IndexMap::from([
                ("label".to_string(), "offer".into()),
                ("priority".to_string(), number(80.0)),
            ])),
        ])
    );
}

#[test]
fn object_literals_support_computed_keys_methods_and_spread() {
    let program = compile(
        r#"
        const key = "value";
        const extra = [3];
        extra.label = "ok";
        const record = {
          alpha: 1,
          [key]: 2,
          total(step) {
            return this.alpha + this[key] + step;
          },
          ...null,
          ...extra,
          ...{ beta: 4 },
        };
        [record.value, record[0], record.label, record.total(5)];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(2.0),
            number(3.0),
            StructuredValue::String("ok".to_string()),
            number(8.0),
        ])
    );
}

#[test]
fn static_object_literals_lower_without_runtime_property_mutation_ops() {
    let program = compile(
        r#"
        ({
          alpha: 1,
          beta: 2,
          gamma: 3,
        });
        "#,
    )
    .expect("source should compile");

    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let instructions: Vec<_> = bytecode
        .functions
        .iter()
        .flat_map(|function| function.code.iter())
        .collect();

    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::MakeObject { keys } if keys.len() == 3
    )));
    assert!(!instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::SetPropStatic { .. } | Instruction::SetPropComputed | Instruction::Dup
    )));
}

#[test]
fn nested_closures_keep_shadowed_slots_distinct_while_updating_outer_bindings() {
    let program = compile(
        r#"
        function makeCounter(seed) {
          let total = seed;
          return function(step) {
            let readShadow;
            {
              let total = 1000;
              readShadow = function() {
                return total + step;
              };
            }
            const apply = function(extra) {
              total = total + extra;
              return [readShadow(), total];
            };
            return apply(step);
          };
        }
        const counter = makeCounter(1);
        [counter(2), counter(3)];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![number(1002.0), number(3.0)]),
            StructuredValue::Array(vec![number(1003.0), number(6.0)]),
        ])
    );
}

#[test]
fn unresolved_globals_lower_to_fast_path_and_preserve_runtime_behavior() {
    let program = compile(
        r#"
        function run(step) {
          value += step;
          return fetch_data(value + (Math.PI > 3 ? 1 : 0));
        }
        run(2);
        "#,
    )
    .expect("source should compile");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let instructions: Vec<_> = bytecode
        .functions
        .iter()
        .flat_map(|function| function.code.iter())
        .collect();
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::LoadGlobal(name) if name == "value"
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::StoreGlobal(name) if name == "value"
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::LoadGlobal(name) if name == "fetch_data"
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::LoadGlobal(name) if name == "Math"
    )));

    let step = start(
        &program,
        ExecutionOptions {
            inputs: indexmap::IndexMap::from([("value".to_string(), number(5.0))]),
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("program should start");
    let suspended = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(_) => panic!("program should suspend on fetch_data"),
    };
    assert_eq!(suspended.capability, "fetch_data");
    assert_eq!(suspended.args, vec![number(8.0)]);
}

#[test]
fn missing_unresolved_globals_still_raise_reference_errors() {
    let program = compile("missingValue + 1;").expect("source should compile");

    let error = execute(&program, ExecutionOptions::default())
        .expect_err("missing global should still fail closed");

    assert!(
        error
            .to_string()
            .contains("ReferenceError: `missingValue` is not defined")
    );
}

#[test]
fn sequence_expressions_preserve_left_to_right_side_effects_and_last_value() {
    let program = compile(
        r#"
        let steps = 0;
        const result = (steps = steps + 1, steps = steps + 2, 2 ** 3 ** 2);
        [result, steps];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![number(512.0), number(3.0)])
    );
}

#[test]
fn in_operator_checks_the_supported_property_surface() {
    let program = compile(
        r#"
        const object = { alpha: undefined };
        const array = [4];
        array.extra = 5;
        const map = new Map();
        const set = new Set();
        const promise = Promise.resolve(1);
        const regex = /a/g;
        const date = new Date(5);
        [
          "alpha" in object,
          "missing" in object,
          0 in array,
          1 in array,
          "length" in array,
          "push" in array,
          "extra" in array,
          "log" in Math,
          "parse" in JSON,
          "then" in promise,
          "exec" in regex,
          "getTime" in date,
          "size" in map,
          "add" in set,
          "from" in Array,
          "assign" in Object,
          "now" in Date,
          "resolve" in Promise,
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Bool(true),
            StructuredValue::Bool(false),
            StructuredValue::Bool(true),
            StructuredValue::Bool(false),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
        ])
    );
}

#[test]
fn in_operator_fails_closed_for_primitive_right_hand_sides() {
    let program = compile(r#""length" in "abc";"#).expect("source should compile");
    let error = execute(&program, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("right-hand side of 'in' must be an object in the supported surface")
    );
}

#[test]
fn sparse_array_find_helpers_match_documented_hole_visitation() {
    let program = compile(
        r#"
        const values = [1, , 3];
        ({
          found: values.find((value, index) => index === 1),
          foundIndex: values.findIndex((value) => value === undefined),
          visited: (() => {
            const seen = [];
            values.find((value, index) => {
              seen[seen.length] = [index, value === undefined, index in values];
              return false;
            });
            return seen;
          })(),
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(indexmap::IndexMap::from([
            ("found".to_string(), StructuredValue::Undefined),
            ("foundIndex".to_string(), number(1.0)),
            (
                "visited".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Array(vec![
                        number(0.0),
                        StructuredValue::Bool(false),
                        StructuredValue::Bool(true),
                    ]),
                    StructuredValue::Array(vec![
                        number(1.0),
                        StructuredValue::Bool(true),
                        StructuredValue::Bool(false),
                    ]),
                    StructuredValue::Array(vec![
                        number(2.0),
                        StructuredValue::Bool(false),
                        StructuredValue::Bool(true),
                    ]),
                ]),
            ),
        ]))
    );
}
