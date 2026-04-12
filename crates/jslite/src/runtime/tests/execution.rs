use super::*;

#[test]
fn runs_arithmetic_and_locals() {
    let value = run(r#"
            const a = 4;
            const b = 3;
            a * b + 2;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(14.0))
    );
}

#[test]
fn runs_functions_and_closures() {
    let value = run(r#"
            function makeAdder(x) {
              return (y) => x + y;
            }
            const add2 = makeAdder(2);
            add2(5);
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(7.0))
    );
}

#[test]
fn runs_arrays_objects_and_member_access() {
    let value = run(r#"
            const values = [1, 2];
            const record = { total: values[0] + values[1] };
            record.total;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(3.0))
    );
}

#[test]
fn runs_for_in_over_plain_objects_and_arrays() {
    let value = run(r#"
            const object = { beta: 2, alpha: 1 };
            const array = [10, 20];
            array.extra = 30;
            const objectKeys = [];
            for (const key in object) {
              objectKeys[objectKeys.length] = key;
            }
            const arrayKeys = [];
            for (const key in array) {
              arrayKeys[arrayKeys.length] = key;
            }
            [objectKeys, arrayKeys];
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                StructuredValue::String("alpha".to_string()),
                StructuredValue::String("beta".to_string()),
            ]),
            StructuredValue::Array(vec![
                StructuredValue::String("0".to_string()),
                StructuredValue::String("1".to_string()),
                StructuredValue::String("extra".to_string()),
            ]),
        ])
    );
}

#[test]
fn for_in_supports_assignment_target_headers() {
    let value = run(r#"
            const record = { current: "" };
            for (record.current in { beta: 2, alpha: 1 }) {
            }
            record.current;
            "#);
    assert_eq!(value, StructuredValue::String("beta".to_string()));
}

#[test]
fn runs_branching_loops_and_switch() {
    let value = run(r#"
            let total = 0;
            let i = 0;
            while (i < 4) {
              total += i;
              i += 1;
            }
            do {
              total += 1;
            } while (false);
            for (let j = 0; j < 2; j += 1) {
              if (j === 0) {
                continue;
              }
              total += j;
            }
            switch (total) {
              case 8:
                total += 1;
                break;
              default:
                total = 0;
            }
            total;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(9.0))
    );
}

#[test]
fn for_in_snapshots_keys_and_survives_resumption() {
    let program = compile(r#"
            let total = 0;
            for (const key in { beta: 2, alpha: 1 }) {
              total += fetch_data(key);
            }
            total;
            "#)
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
    .expect("start should suspend")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
    };
    assert_eq!(first.capability, "fetch_data");
    assert_eq!(
        first.args,
        vec![StructuredValue::String("alpha".to_string())]
    );

    let encoded = dump_snapshot(&first.snapshot).expect("snapshot should serialize");
    let loaded = load_snapshot(&encoded).expect("snapshot should deserialize");

    let second = match resume_with_options(
        loaded,
        ResumePayload::Value(StructuredValue::Number(StructuredNumber::Finite(10.0))),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(
                &["fetch_data"],
                RuntimeLimits::default(),
            )),
        },
    )
    .expect("resume should work")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected second suspension, got {value:?}"),
    };
    assert_eq!(second.capability, "fetch_data");
    assert_eq!(
        second.args,
        vec![StructuredValue::String("beta".to_string())]
    );

    match resume(second.snapshot, ResumePayload::Value(StructuredValue::Number(
        StructuredNumber::Finite(20.0),
    )))
    .expect("final resume should work")
    {
        ExecutionStep::Completed(value) => {
            assert_eq!(value, StructuredValue::Number(StructuredNumber::Finite(30.0)));
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn for_in_rejects_unsupported_rhs_values() {
    let program = compile(
        r#"
        for (const key in new Map()) {
          key;
        }
        "#,
    )
    .expect("source should compile");

    let error = execute(&program, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Object helpers currently only support plain objects and arrays")
    );
}

#[test]
fn runs_math_and_json_builtins() {
    let value = run(r#"
            const encoded = JSON.stringify({ value: Math.max(1, 9, 4) });
            JSON.parse(encoded).value;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(9.0))
    );
}

#[test]
fn preserves_supported_enumeration_order_for_json_stringify() {
    let value = run(r#"
            const record = {};
            record.beta = "b";
            record[10] = "ten";
            record.alpha = "a";
            record[2] = "two";
            record["01"] = "leading";
            const values = ["c", "d"];
            values.extra = "ignored";
            JSON.stringify({ record, values });
            "#);
    assert_eq!(
        value,
        StructuredValue::String(
            r#"{"record":{"2":"two","10":"ten","beta":"b","alpha":"a","01":"leading"},"values":["c","d"]}"#
                .to_string()
        )
    );
}

#[test]
fn runs_object_literals_with_computed_keys_methods_and_spread() {
    let value = run(
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
            [record.value, record[0], record.label, record.total(5), record.beta];
            "#,
    );
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::Number(StructuredNumber::Finite(2.0)),
            StructuredValue::Number(StructuredNumber::Finite(3.0)),
            StructuredValue::String("ok".to_string()),
            StructuredValue::Number(StructuredNumber::Finite(8.0)),
            StructuredValue::Number(StructuredNumber::Finite(4.0)),
        ])
    );
}

#[test]
fn runs_array_spread_and_spread_arguments() {
    let value = run(
        r#"
            const values = [2, 4];
            const sparse = [2, , 4];
            const extra = new Set("ab");
            const box = {
              base: 10,
              total(...args) {
                return this.base + args[0] + args[1] + args[2] + args[3];
              },
            };
            const array = [1, ...sparse, ...extra, 5];
            const built = new Array(...values, 9);
            [
              array,
              box.total(...values, 6),
              [1, ...sparse][2],
              Math.max(...values, 3),
              built,
              ({ missing: null }).missing?.(...values),
            ];
            "#,
    );
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                StructuredValue::Number(StructuredNumber::Finite(1.0)),
                StructuredValue::Number(StructuredNumber::Finite(2.0)),
                StructuredValue::Null,
                StructuredValue::Number(StructuredNumber::Finite(4.0)),
                StructuredValue::String("a".to_string()),
                StructuredValue::String("b".to_string()),
                StructuredValue::Number(StructuredNumber::Finite(5.0)),
            ]),
            StructuredValue::Number(StructuredNumber::Finite(22.0)),
            StructuredValue::Null,
            StructuredValue::Number(StructuredNumber::Finite(4.0)),
            StructuredValue::Array(vec![
                StructuredValue::Number(StructuredNumber::Finite(2.0)),
                StructuredValue::Number(StructuredNumber::Finite(4.0)),
                StructuredValue::Number(StructuredNumber::Finite(9.0)),
            ]),
            StructuredValue::Undefined,
        ])
    );
}

#[test]
fn spread_fails_closed_for_unsupported_iterables() {
    let program = compile(
        r#"
            const object = { alpha: 1 };
            [ [...object], Math.max(...object) ];
            "#,
    )
    .expect("spread should lower");
    let error = execute(&program, ExecutionOptions::default())
        .expect_err("unsupported spread sources should fail closed at runtime");
    assert!(
        error
            .to_string()
            .contains("value is not iterable in the supported surface")
    );
}

#[test]
fn runs_sparse_arrays_across_helpers_and_json() {
    let value = run(
        r#"
            const values = [1, , undefined, 4];
            const callbackIndexes = [];
            values.forEach((value, index) => {
              callbackIndexes[callbackIndexes.length] = index;
            });
            const sliced = values.slice(0, 4);
            const mapped = values.map((value, index) => value ?? (index + 10));
            JSON.stringify({
              length: values.length,
              holeIsUndefined: values[1] === undefined,
              hasHole: 1 in values,
              hasUndefined: 2 in values,
              keys: Object.keys(values),
              entries: Object.entries(values),
              iterated: Array.from(values.values()),
              includesUndefined: values.includes(undefined),
              indexOfUndefined: values.indexOf(undefined),
              joined: values.join("-"),
              json: JSON.stringify(values),
              callbackIndexes,
              slicedHasHole: 1 in sliced,
              mappedHasHole: 1 in mapped,
              mappedKeys: Object.keys(mapped),
            });
            "#,
    );
    assert_eq!(
        value,
        StructuredValue::String(
            r#"{"length":4,"holeIsUndefined":true,"hasHole":false,"hasUndefined":true,"keys":["0","2","3"],"entries":[["0",1],["2",null],["3",4]],"iterated":[1,null,null,4],"includesUndefined":true,"indexOfUndefined":2,"joined":"1---4","json":"[1,null,null,4]","callbackIndexes":[0,2,3],"slicedHasHole":false,"mappedHasHole":false,"mappedKeys":["0","2","3"]}"#
                .to_string()
        )
    );
}

#[test]
fn structured_inputs_preserve_sparse_array_holes() {
    let program = compile(
        r#"
            [
              Object.keys(value),
              value[0] === undefined,
              0 in value,
              1 in value,
              value[1],
              JSON.stringify(value),
            ];
            "#,
    )
    .expect("source should compile");

    let value = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "value".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Hole,
                    StructuredValue::Number(StructuredNumber::Finite(2.0)),
                    StructuredValue::Hole,
                ]),
            )]),
            ..ExecutionOptions::default()
        },
    )
    .expect("program should run");

    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![StructuredValue::String("1".to_string())]),
            StructuredValue::Bool(true),
            StructuredValue::Bool(false),
            StructuredValue::Bool(true),
            StructuredValue::Number(StructuredNumber::Finite(2.0)),
            StructuredValue::String("[null,2,null]".to_string()),
        ])
    );
}

#[test]
fn object_spread_fails_closed_for_unsupported_sources() {
    let program = compile("({ ...1 });").expect("object spread should lower");
    let error = execute(&program, ExecutionOptions::default())
        .expect_err("unsupported object spread sources should fail closed at runtime");
    assert!(
        error
            .to_string()
            .contains("object spread currently only supports plain objects and arrays")
    );
}

#[test]
fn enforces_instruction_budget() {
    let program = compile("while (true) {}").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits {
                instruction_budget: 100,
                ..RuntimeLimits::default()
            },
            cancellation_token: None,
        },
    )
    .expect_err("infinite loop should exhaust budget");
    assert!(error.to_string().contains("instruction budget exhausted"));
}
