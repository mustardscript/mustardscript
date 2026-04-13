use super::*;

#[test]
fn lowering_errors_preserve_source_spans() {
    let program = compile("continue;").expect("source should compile");
    let error =
        lower_to_bytecode(&program).expect_err("continue outside a loop should fail lowering");
    let rendered = error.to_string();
    assert!(rendered.contains("`continue` used outside of a loop"));
    assert!(rendered.contains("[0..9]"));
}

#[test]
fn runtime_errors_include_guest_tracebacks() {
    let program = compile(
        r#"
            function outer() {
              return inner();
            }
            function inner() {
              const value = null;
              return value.answer;
            }
            outer();
            "#,
    )
    .expect("source should compile");
    let error = execute(&program, ExecutionOptions::default())
        .expect_err("nullish property access should fail");
    let rendered = error.to_string();
    assert!(rendered.contains("TypeError: cannot read properties of nullish value"));
    assert!(rendered.contains("at inner ["));
    assert!(rendered.contains("at outer ["));
    assert!(rendered.contains("at <script> ["));
    assert!(!rendered.contains(".rs"));
}
