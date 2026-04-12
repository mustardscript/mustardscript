use crate::compile;

fn assert_validation_reject(source: &str, message: &str) {
    let error = compile(source).expect_err("source should fail validation");
    assert!(error.to_string().contains(message));
}

#[test]
fn rejects_forbidden_free_require() {
    let error = compile("require('fs');").expect_err("should reject forbidden global");
    let text = error.to_string();
    assert!(text.contains("forbidden ambient global `require`"));
}

#[test]
fn rejects_free_eval() {
    let error = compile("eval('1 + 1');").expect_err("should reject eval");
    let text = error.to_string();
    assert!(text.contains("forbidden ambient global `eval`"));
    assert!(text.contains("[0..4]"));
}

#[test]
fn rejects_free_function_constructor() {
    let error = compile("new Function('return 1;');").expect_err("should reject Function");
    let text = error.to_string();
    assert!(text.contains("forbidden ambient global `Function`"));
    assert!(text.contains("[4..12]"));
}

#[test]
fn rejects_free_arguments() {
    let error =
        compile("function wrap() { return arguments[0]; }").expect_err("should reject arguments");
    let text = error.to_string();
    assert!(text.contains("forbidden ambient global `arguments`"));
}

#[test]
fn rejects_default_parameters() {
    let error = compile("function wrap(value = 1) { return value; }")
        .expect_err("should reject default parameters");
    let text = error.to_string();
    assert!(text.contains("default parameters are not supported in v1"));
}

#[test]
fn rejects_default_destructuring() {
    let error =
        compile("const { value = 1 } = {};").expect_err("should reject default destructuring");
    let text = error.to_string();
    assert!(text.contains("default destructuring is not supported in v1"));
}

#[test]
fn rejects_module_syntax() {
    let error = compile("export const x = 1;").expect_err("module syntax should fail");
    assert!(error.to_string().contains("module syntax"));
}

#[test]
fn rejects_meta_properties_even_near_supported_spread_constructs() {
    assert_validation_reject(
        "new.target(...values);",
        "meta properties are not supported",
    );
}

#[test]
fn rejects_delete_operator() {
    let error = compile("delete record.value;").expect_err("delete should fail");
    let text = error.to_string();
    assert!(text.contains("delete is not supported in v1"));
    assert!(text.contains("[0..19]"));
}

#[test]
fn rejects_delete_on_array_elements() {
    assert_validation_reject("delete values[0];", "delete is not supported in v1");
}

#[test]
fn rejects_for_of_destructuring_assignment_targets() {
    let error = compile("let value = 0; for ([value] of [[1], [2]]) { value; }")
        .expect_err("destructuring assignment-target for...of should fail closed");
    assert!(
        error
            .to_string()
            .contains("destructuring assignment is not supported in v1")
    );
}

#[test]
fn rejects_destructuring_assignment() {
    assert_validation_reject(
        "[value] = [1];",
        "destructuring assignment is not supported in v1",
    );
}

#[test]
fn rejects_for_in_destructuring_assignment_targets() {
    let error = compile("let value = ''; for ([value] in { alpha: 1 }) { value; }")
        .expect_err("destructuring assignment-target for...in should fail closed");
    assert!(
        error
            .to_string()
            .contains("destructuring assignment is not supported in v1")
    );
}

#[test]
fn rejects_var_declarations() {
    assert_validation_reject("var value = 1;", "only let and const are supported");
}

#[test]
fn rejects_function_scoped_var_declarations() {
    assert_validation_reject(
        "function wrap() { var value = 1; return value; }",
        "only let and const are supported",
    );
}

#[test]
fn rejects_update_expressions() {
    assert_validation_reject(
        "let value = 1; value++;",
        "update expressions are not supported in v1",
    );
}

#[test]
fn rejects_instanceof_operator() {
    let error = compile("1 instanceof Number;").expect_err("instanceof should stay unsupported");
    assert!(
        error
            .to_string()
            .contains("unsupported binary operator in v1")
    );
}

#[test]
fn rejects_instanceof_with_guest_functions() {
    assert_validation_reject(
        "function Box() {} const value = {}; value instanceof Box;",
        "unsupported binary operator in v1",
    );
}

#[test]
fn rejects_additional_unsupported_assignment_operators() {
    for source in [
        "let value = 2; value %= 3;",
        "let value = 2; value **= 3;",
        "let value = 2; value &= 3;",
    ] {
        assert_validation_reject(source, "unsupported assignment operator in v1");
    }
}

#[test]
fn rejects_object_literal_accessors() {
    let error = compile("({ get value() { return 1; } });")
        .expect_err("object literal accessors should fail closed");
    assert!(
        error
            .to_string()
            .contains("object literal accessors are not supported in v1")
    );
}
