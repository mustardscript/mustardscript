use crate::{
    compile,
    ir::{ArrayElement, AssignOp, CallArgument, Expr, Stmt},
};

#[test]
fn parses_basic_function_and_if() {
    let program = compile(
        r#"
        const add = (a, b) => {
          if (a > b) {
            return a + b;
          }
          return a ?? b;
        };
        "#,
    )
    .expect("program should compile");

    assert_eq!(program.script.body.len(), 1);
    match &program.script.body[0] {
        Stmt::VariableDecl { declarators, .. } => {
            assert_eq!(declarators.len(), 1);
        }
        other => panic!("unexpected stmt: {other:?}"),
    }
}

#[test]
fn allows_shadowed_require() {
    compile("const require = () => 1; require();").expect("shadowed require should compile");
}

#[test]
fn allows_shadowed_function_identifier() {
    compile("const Function = (value) => value; Function(1);")
        .expect("shadowed Function should compile");
}

#[test]
fn allows_shadowed_arguments_identifier() {
    compile("const arguments = [1]; arguments[0];").expect("shadowed arguments should compile");
}

#[test]
fn parses_array_for_of_with_const_binding() {
    let program = compile(
        r#"
        let total = 0;
        for (const value of [1, 2, 3]) {
          total += value;
        }
        total;
        "#,
    )
    .expect("for...of over arrays should compile");

    assert!(matches!(program.script.body[1], Stmt::ForOf { .. }));
}

#[test]
fn parses_for_of_with_assignment_targets() {
    let program = compile(
        r#"
        let value = 0;
        const boxes = [{ current: 0 }, { current: 0 }];
        let index = 0;
        for (value of [1, 2]) {
          index += value;
        }
        for (boxes[index - 3].current of [3, 4]) {
          index += 1;
        }
        value + boxes[0].current + boxes[1].current + index;
        "#,
    )
    .expect("for...of assignment-target headers should compile");

    assert!(matches!(program.script.body[3], Stmt::ForOf { .. }));
    assert!(matches!(program.script.body[4], Stmt::ForOf { .. }));
}

#[test]
fn parses_for_in_with_binding_and_assignment_targets() {
    let program = compile(
        r#"
        let total = 0;
        const boxes = [{ current: 0 }, { current: 0 }];
        let index = 0;
        for (const key in { beta: 2, alpha: 1 }) {
          total += key.length;
        }
        for (boxes[index].current in [3, 4]) {
          index += 1;
        }
        total + boxes[0].current + boxes[1].current + index;
        "#,
    )
    .expect("for...in should compile");

    assert!(matches!(program.script.body[3], Stmt::ForIn { .. }));
    assert!(matches!(program.script.body[4], Stmt::ForIn { .. }));
}

#[test]
fn parses_for_await_of_inside_async_functions() {
    compile(
        r#"
        async function run(values, boxRef) {
          for await (const value of values) {
            boxRef.total += value;
          }
          for await (boxRef.current of values) {
            boxRef.total += boxRef.current;
          }
          return boxRef.total;
        }
        "#,
    )
    .expect("for await...of should compile inside async functions");
}

#[test]
fn parses_default_parameters_and_default_destructuring() {
    compile(
        r#"
        function wrap(value = 1, { label = "ok" } = {}) {
          return [value, label];
        }
        wrap();
        "#,
    )
    .expect("default parameters and destructuring should compile");
}

#[test]
fn parses_destructuring_assignment_targets_and_update_expressions() {
    compile(
        r#"
        let value = 0;
        let boxRef = { current: 1 };
        [value, boxRef.current] = [2, 3];
        ({ value, current: boxRef.current = 4 } = { value: 5 });
        ++value;
        boxRef.current--;
        "#,
    )
    .expect("destructuring assignment and update expressions should compile");
}

#[test]
fn parses_sequence_and_exponentiation_expressions() {
    compile(
        r#"
        let total = 0;
        const value = (total = total + 1, total = total + 2, 2 ** 3 ** 2);
        [value, total];
        "#,
    )
    .expect("sequence expressions and exponentiation should compile");
}

#[test]
fn parses_remainder_and_exponent_assignment_expressions() {
    compile(
        r#"
        let left = 10;
        let right = 2;
        left %= 3;
        right **= 3;
        [left, right];
        "#,
    )
    .expect("compound remainder and exponent assignments should compile");
}

#[test]
fn parses_in_operator_expressions() {
    compile(
        r#"
        const object = { alpha: undefined };
        const array = [1, 2];
        ["alpha" in object, 1 in array, "push" in array];
        "#,
    )
    .expect("in operator expressions should compile");
}

#[test]
fn parses_instanceof_expressions() {
    compile(
        r#"
        function Box() {}
        [([] instanceof Array), (new Map() instanceof Object), ({} instanceof Box)];
        "#,
    )
    .expect("instanceof should compile for the documented constructor surface");
}

#[test]
fn parses_object_literals_with_computed_keys_methods_and_spread() {
    compile(
        r#"
        const key = "value";
        const extra = [3];
        extra.label = "ok";
        ({
          [key]: 1,
          total(step) {
            return this[key] + step;
          },
          ...null,
          ...extra,
          beta: 4,
        });
        "#,
    )
    .expect("object literal computed keys, methods, and spread should compile");
}

#[test]
fn parses_sparse_array_literals() {
    compile(
        r#"
        const values = [1, , 3];
        [values.length, values[1], 1 in values];
        "#,
    )
    .expect("sparse array literals should compile");
}

#[test]
fn lowers_array_spread_into_ir() {
    let program = compile("[1, ...values, 3];").expect("array spread should compile");

    let Stmt::Expression { expression, .. } = &program.script.body[0] else {
        panic!("unexpected stmt: {:?}", program.script.body[0]);
    };
    let Expr::Array { elements, .. } = expression else {
        panic!("unexpected expr: {expression:?}");
    };

    assert!(matches!(elements[0], ArrayElement::Value(_)));
    assert!(matches!(elements[1], ArrayElement::Spread { .. }));
    assert!(matches!(elements[2], ArrayElement::Value(_)));
}

#[test]
fn lowers_spread_call_arguments_into_ir() {
    let program = compile("run(1, ...values, 3);").expect("spread call arguments should compile");

    let Stmt::Expression { expression, .. } = &program.script.body[0] else {
        panic!("unexpected stmt: {:?}", program.script.body[0]);
    };
    let Expr::Call { arguments, .. } = expression else {
        panic!("unexpected expr: {expression:?}");
    };

    assert!(matches!(arguments[0], CallArgument::Value(_)));
    assert!(matches!(arguments[1], CallArgument::Spread { .. }));
    assert!(matches!(arguments[2], CallArgument::Value(_)));
}

#[test]
fn lowers_spread_constructor_arguments_into_ir() {
    let program =
        compile("new Box(1, ...values, 3);").expect("spread constructor arguments should compile");

    let Stmt::Expression { expression, .. } = &program.script.body[0] else {
        panic!("unexpected stmt: {:?}", program.script.body[0]);
    };
    let Expr::New { arguments, .. } = expression else {
        panic!("unexpected expr: {expression:?}");
    };

    assert!(matches!(arguments[0], CallArgument::Value(_)));
    assert!(matches!(arguments[1], CallArgument::Spread { .. }));
    assert!(matches!(arguments[2], CallArgument::Value(_)));
}

#[test]
fn lowers_spread_optional_call_arguments_into_ir() {
    let program =
        compile("maybeRun?.(1, ...values, 3);").expect("optional spread call should compile");

    let Stmt::Expression { expression, .. } = &program.script.body[0] else {
        panic!("unexpected stmt: {:?}", program.script.body[0]);
    };
    let Expr::Call {
        arguments,
        optional,
        ..
    } = expression
    else {
        panic!("unexpected expr: {expression:?}");
    };

    assert!(*optional);
    assert!(matches!(arguments[0], CallArgument::Value(_)));
    assert!(matches!(arguments[1], CallArgument::Spread { .. }));
    assert!(matches!(arguments[2], CallArgument::Value(_)));
}

#[test]
fn parses_logical_assignment_operators_into_ir() {
    let program = compile(
        r#"
        let truthy = 1;
        let falsy = 0;
        truthy ||= 2;
        falsy &&= 3;
        "#,
    )
    .expect("logical assignment operators should parse");

    match &program.script.body[2] {
        Stmt::Expression {
            expression:
                Expr::Assignment {
                    operator: AssignOp::OrAssign,
                    ..
                },
            ..
        } => {}
        other => panic!("expected ||= assignment expression, got {other:?}"),
    }

    match &program.script.body[3] {
        Stmt::Expression {
            expression:
                Expr::Assignment {
                    operator: AssignOp::AndAssign,
                    ..
                },
            ..
        } => {}
        other => panic!("expected &&= assignment expression, got {other:?}"),
    }
}
