use indexmap::IndexMap;

use jslite::{
    ExecutionOptions, ExecutionStep, ResumePayload, StructuredValue, compile, execute, resume,
    start,
};

#[test]
fn bigint_literals_cover_exact_integer_arithmetic_and_collection_semantics() {
    let program = compile(
        r#"
        const reserve = 9007199254740993n;
        const record = {};
        record[10n] = "ok";
        const set = new Set([1n, 1n, 2n]);
        const map = new Map([[1n, "one"], [2n, "two"]]);
        ({
          sum: String(reserve + 25n),
          diff: String(reserve - 5n),
          product: String(21n * 2n),
          quotient: String(25n / 3n),
          remainder: String(25n % 3n),
          type: typeof reserve,
          truthy: !!1n,
          falsy: !!0n,
          compare: [2n < 10n, 10n >= 10n, 10n === 10n, 10n === 11n],
          key: record["10"],
          setSize: set.size,
          mapValue: map.get(2n),
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            ("sum".to_string(), "9007199254741018".into()),
            ("diff".to_string(), "9007199254740988".into()),
            ("product".to_string(), "42".into()),
            ("quotient".to_string(), "8".into()),
            ("remainder".to_string(), "1".into()),
            ("type".to_string(), "bigint".into()),
            ("truthy".to_string(), true.into()),
            ("falsy".to_string(), false.into()),
            (
                "compare".to_string(),
                StructuredValue::Array(vec![true.into(), true.into(), true.into(), false.into()]),
            ),
            ("key".to_string(), "ok".into()),
            ("setSize".to_string(), 2.0.into()),
            ("mapValue".to_string(), "two".into()),
        ]))
    );
}

#[test]
fn bigint_values_survive_async_host_suspension() {
    let program = compile(
        r#"
        async function main() {
          const reserve = 9007199254740993n;
          const status = await fetch_step("A-9");
          return { status, total: String(reserve + 7n) };
        }
        main();
        "#,
    )
    .expect("source should compile");

    let suspended = match start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_step".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("start should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
    };

    let resumed = resume(suspended.snapshot, ResumePayload::Value("approved".into()))
        .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => assert_eq!(
            value,
            StructuredValue::Object(IndexMap::from([
                ("status".to_string(), "approved".into()),
                ("total".to_string(), "9007199254741000".into()),
            ]))
        ),
        ExecutionStep::Suspended(other) => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn bigint_mixed_number_edges_fail_closed() {
    let program = compile(
        r#"
        [
          (() => {
            try {
              return 1n + 1;
            } catch (error) {
              return error.message;
            }
          })(),
          (() => {
            try {
              return 1n < 2;
            } catch (error) {
              return error.message;
            }
          })(),
          (() => {
            try {
              return Number(1n);
            } catch (error) {
              return error.message;
            }
          })(),
          (() => {
            try {
              return +1n;
            } catch (error) {
              return error.message;
            }
          })(),
          (() => {
            try {
              return 2n ** 2;
            } catch (error) {
              return error.message;
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
            "cannot mix BigInt and Number values in arithmetic".into(),
            "cannot compare BigInt and Number values".into(),
            "cannot coerce BigInt values to numbers".into(),
            "unary plus is not supported for BigInt values".into(),
            "cannot mix BigInt and Number values in arithmetic".into(),
        ])
    );
}

#[test]
fn bigint_exponentiation_supports_non_negative_bigint_exponents_only() {
    let program = compile(
        r#"
        [
          String(2n ** 5n),
          String((-3n) ** 3n),
          (() => {
            try {
              return String(2n ** (-1n));
            } catch (error) {
              return error.message;
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
            "32".into(),
            "-27".into(),
            "BigInt exponent must be non-negative".into(),
        ])
    );
}

#[test]
fn bigint_json_and_host_boundary_fail_closed() {
    let json_program = compile(
        r#"
        [
          (() => {
            try {
              return JSON.stringify(1n);
            } catch (error) {
              return error.message;
            }
          })(),
          (() => {
            try {
              return JSON.stringify({ amount: 1n });
            } catch (error) {
              return error.message;
            }
          })(),
        ];
        "#,
    )
    .expect("source should compile");
    let json_result =
        execute(&json_program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        json_result,
        StructuredValue::Array(vec![
            "Do not know how to serialize a BigInt".into(),
            "Do not know how to serialize a BigInt".into(),
        ])
    );

    let top_level = compile("1n;").expect("source should compile");
    let error = execute(&top_level, ExecutionOptions::default()).expect_err("result should fail");
    assert!(
        error
            .to_string()
            .contains("BigInt values cannot cross the structured host boundary")
    );

    let boundary = compile("send_amount(1n);").expect("source should compile");
    let error = start(
        &boundary,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["send_amount".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect_err("capability call should fail before suspension");
    assert!(
        error
            .to_string()
            .contains("BigInt values cannot cross the structured host boundary")
    );
}
