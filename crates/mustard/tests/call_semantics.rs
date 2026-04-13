use mustard::{ExecutionOptions, StructuredValue, compile, execute};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

#[test]
fn member_calls_bind_receivers_for_guest_functions() {
    let program = compile(
        r#"
        const method = function (delta) {
          return this.base + delta;
        };
        const obj = { base: 40, method: method };
        obj.method(2);
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(result, number(42.0));
}

#[test]
fn member_calls_do_not_override_lexical_receivers_for_arrow_functions() {
    let program = compile(
        r#"
        const method = () => this === globalThis;
        const obj = { method: method };
        obj.method();
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(result, StructuredValue::Bool(true));
}

#[test]
fn rest_parameters_bind_remaining_arguments() {
    let program = compile(
        r#"
        function collect(head, ...tail) {
          return [head, tail.length, tail[0], tail[1]];
        }
        collect(1, 2, 3);
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![number(1.0), number(2.0), number(2.0), number(3.0)])
    );
}

#[test]
fn rest_parameters_support_destructuring_patterns() {
    let program = compile(
        r#"
        const collect = (...[first, second]) => first + second;
        collect(4, 5, 6);
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(result, number(9.0));
}

#[test]
fn shadowed_arguments_bindings_still_work() {
    let program = compile(
        r#"
        function first(arguments) {
          return arguments[0];
        }
        first([7, 8]);
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(result, number(7.0));
}

#[test]
fn user_defined_constructors_remain_rejected() {
    let program = compile(
        r#"
        const Foo = function () {};
        new Foo();
        "#,
    )
    .expect("source should compile");

    let error = execute(&program, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("only conservative built-in constructors are supported in v1")
    );
}
