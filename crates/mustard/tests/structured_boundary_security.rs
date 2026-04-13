use mustard::{ExecutionOptions, StructuredValue, compile, execute, start};

const CYCLE_BOUNDARY_MESSAGE: &str = "cyclic values cannot cross the structured host boundary";
const SHARED_BOUNDARY_MESSAGE: &str = "shared references cannot cross the structured host boundary";
const DEPTH_BOUNDARY_MESSAGE: &str = "structured host boundary nesting limit exceeded";
const JSON_STRINGIFY_CYCLE_MESSAGE: &str = "Converting circular structure to JSON";
const SAFE_MESSAGE_PATH_FRAGMENTS: &[&str] = &["/Users/", "\\Users\\", "C:\\", "/home/"];

fn assert_cycle_boundary_error(error: impl std::fmt::Display) {
    let message = error.to_string();
    assert!(
        message.contains(CYCLE_BOUNDARY_MESSAGE),
        "expected cycle boundary error, got: {message}"
    );
    for fragment in SAFE_MESSAGE_PATH_FRAGMENTS {
        assert!(
            !message.contains(fragment),
            "message leaked host path fragment `{fragment}`: {message}"
        );
    }
}

fn assert_depth_boundary_error(error: impl std::fmt::Display) {
    let message = error.to_string();
    assert!(
        message.contains(DEPTH_BOUNDARY_MESSAGE),
        "expected depth boundary error, got: {message}"
    );
    for fragment in SAFE_MESSAGE_PATH_FRAGMENTS {
        assert!(
            !message.contains(fragment),
            "message leaked host path fragment `{fragment}`: {message}"
        );
    }
}

fn assert_shared_boundary_error(error: impl std::fmt::Display) {
    let message = error.to_string();
    assert!(
        message.contains(SHARED_BOUNDARY_MESSAGE),
        "expected shared-reference boundary error, got: {message}"
    );
    for fragment in SAFE_MESSAGE_PATH_FRAGMENTS {
        assert!(
            !message.contains(fragment),
            "message leaked host path fragment `{fragment}`: {message}"
        );
    }
}

fn deeply_nested_array(depth: usize) -> StructuredValue {
    let mut value = StructuredValue::from(0.0);
    for _ in 0..depth {
        value = StructuredValue::Array(vec![value]);
    }
    value
}

#[test]
fn cyclic_root_results_fail_closed_during_result_serialization() {
    let program = compile(
        r#"
        const value = {};
        value.self = value;
        value;
        "#,
    )
    .expect("source should compile");

    let error = execute(&program, ExecutionOptions::default())
        .expect_err("cyclic root results should fail closed");
    assert_cycle_boundary_error(error);
}

#[test]
fn cyclic_host_capability_arguments_fail_closed_before_suspension() {
    let program = compile(
        r#"
        const values = [];
        values.push(values);
        send(values);
        "#,
    )
    .expect("source should compile");

    let error = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["send".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect_err("cyclic host-call arguments should fail closed");
    assert_cycle_boundary_error(error);
}

#[test]
fn shared_root_results_fail_closed_instead_of_expanding_aliases() {
    let program = compile(
        r#"
        const shared = [1, 2, 3];
        [shared, shared];
        "#,
    )
    .expect("source should compile");

    let error = execute(&program, ExecutionOptions::default())
        .expect_err("shared root results should fail closed");
    assert_shared_boundary_error(error);
}

#[test]
fn shared_host_capability_arguments_fail_closed_before_suspension() {
    let program = compile(
        r#"
        const shared = { value: 1 };
        send([shared, shared]);
        "#,
    )
    .expect("source should compile");

    let error = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["send".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect_err("shared host-call arguments should fail closed");
    assert_shared_boundary_error(error);
}

#[test]
fn excessively_deep_host_inputs_fail_closed_before_execution() {
    let program = compile("value;").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: [("value".to_string(), deeply_nested_array(1_100))]
                .into_iter()
                .collect(),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("deep host inputs should fail closed");
    assert_depth_boundary_error(error);
}

#[test]
fn excessively_deep_root_results_fail_closed_during_result_serialization() {
    let program = compile(
        r#"
        let value = 0;
        for (let index = 0; index < 1100; index = index + 1) {
          value = [value];
        }
        value;
        "#,
    )
    .expect("source should compile");

    let error = execute(&program, ExecutionOptions::default())
        .expect_err("deep root results should fail closed");
    assert_depth_boundary_error(error);
}

#[test]
fn excessively_deep_host_capability_arguments_fail_closed_before_suspension() {
    let program = compile(
        r#"
        let value = 0;
        for (let index = 0; index < 1100; index = index + 1) {
          value = [value];
        }
        send(value);
        "#,
    )
    .expect("source should compile");

    let error = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["send".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect_err("deep host-call arguments should fail closed");
    assert_depth_boundary_error(error);
}

#[test]
fn json_stringify_reports_cyclic_guest_values_as_guest_safe_errors() {
    let program = compile(
        r#"
        const value = {};
        const items = [value];
        value.items = items;
        let result = "missing";
        try {
          JSON.stringify(value);
          result = "unreachable";
        } catch (error) {
          result = error.message;
        }
        result;
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default())
        .expect("JSON.stringify cycle failures should be catchable guest errors");
    assert_eq!(
        result,
        StructuredValue::String(JSON_STRINGIFY_CYCLE_MESSAGE.to_string())
    );
}
