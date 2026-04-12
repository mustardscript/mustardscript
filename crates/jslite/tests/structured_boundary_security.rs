use jslite::{ExecutionOptions, StructuredValue, compile, execute, start};

const CYCLE_BOUNDARY_MESSAGE: &str = "cyclic values cannot cross the structured host boundary";
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
