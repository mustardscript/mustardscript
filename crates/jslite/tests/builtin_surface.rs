use indexmap::IndexMap;

use jslite::{ExecutionOptions, StructuredValue, compile, execute};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

#[test]
fn array_helpers_cover_mutation_search_and_slicing() {
    let program = compile(
        r#"
        const values = [1, 2];
        const nan = 0 / 0;
        const pushed = values.push(3, 4);
        const popped = values.pop();
        [
          pushed,
          popped,
          values.slice(1, 3),
          values.join("-"),
          values.includes(2),
          values.includes(nan),
          [1, nan].includes(nan),
          values.indexOf(2),
          values.indexOf(9),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(4.0),
            number(4.0),
            StructuredValue::Array(vec![number(2.0), number(3.0)]),
            "1-2-3".into(),
            StructuredValue::Bool(true),
            StructuredValue::Bool(false),
            StructuredValue::Bool(true),
            number(1.0),
            number(-1.0),
        ])
    );
}

#[test]
fn string_helpers_cover_trimming_queries_and_case_changes() {
    let program = compile(
        r#"
        const value = "  MiXeD Example  ";
        [
          value.trim(),
          value.includes("XeD"),
          value.startsWith("Mi", 2),
          value.endsWith("ple  "),
          value.slice(2, -2),
          value.substring(8, 3),
          value.toLowerCase(),
          value.toUpperCase(),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            "MiXeD Example".into(),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            "MiXeD Example".into(),
            "iXeD ".into(),
            "  mixed example  ".into(),
            "  MIXED EXAMPLE  ".into(),
        ])
    );
}

#[test]
fn object_helpers_enumerate_plain_objects_and_arrays_deterministically() {
    let program = compile(
        r#"
        const object = { zebra: 1, alpha: 2 };
        const array = [4, 5];
        array.extra = 6;
        ({
          objectKeys: Object.keys(object),
          objectValues: Object.values(object),
          objectEntries: Object.entries(object),
          arrayKeys: Object.keys(array),
          arrayValues: Object.values(array),
          arrayEntries: Object.entries(array),
          hasOwnAlpha: Object.hasOwn(object, "alpha"),
          hasOwnMissing: Object.hasOwn(object, "missing"),
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            (
                "objectKeys".to_string(),
                StructuredValue::Array(vec!["alpha".into(), "zebra".into()]),
            ),
            (
                "objectValues".to_string(),
                StructuredValue::Array(vec![number(2.0), number(1.0)]),
            ),
            (
                "objectEntries".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Array(vec!["alpha".into(), number(2.0)]),
                    StructuredValue::Array(vec!["zebra".into(), number(1.0)]),
                ]),
            ),
            (
                "arrayKeys".to_string(),
                StructuredValue::Array(vec!["0".into(), "1".into(), "extra".into()]),
            ),
            (
                "arrayValues".to_string(),
                StructuredValue::Array(vec![number(4.0), number(5.0), number(6.0)]),
            ),
            (
                "arrayEntries".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Array(vec!["0".into(), number(4.0)]),
                    StructuredValue::Array(vec!["1".into(), number(5.0)]),
                    StructuredValue::Array(vec!["extra".into(), number(6.0)]),
                ]),
            ),
            ("hasOwnAlpha".to_string(), StructuredValue::Bool(true)),
            ("hasOwnMissing".to_string(), StructuredValue::Bool(false)),
        ]))
    );
}

#[test]
fn math_helpers_cover_numeric_transforms() {
    let program = compile(
        r#"
        [
          Math.pow(2, 5),
          Math.sqrt(81),
          Math.trunc(-3.9),
          Math.sign(-12),
          Math.sign(-0),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    match result {
        StructuredValue::Array(values) => {
            assert_eq!(values[0], number(32.0));
            assert_eq!(values[1], number(9.0));
            assert_eq!(values[2], number(-3.0));
            assert_eq!(values[3], number(-1.0));
            assert_eq!(values[4], StructuredValue::from(-0.0));
        }
        other => panic!("expected array result, got {other:?}"),
    }
}
