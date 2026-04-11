use jslite::{ExecutionOptions, RuntimeLimits, StructuredValue, compile, execute};

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
