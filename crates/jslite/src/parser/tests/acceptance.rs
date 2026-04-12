use crate::{compile, ir::Stmt};

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
