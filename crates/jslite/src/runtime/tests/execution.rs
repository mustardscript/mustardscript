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
