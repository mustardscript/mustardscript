use mustard::ir::BinaryOp;
use mustard::runtime::{Instruction, dump_program, load_program, start_bytecode};
use mustard::{
    CompileOptions, ExecutionOptions, StructuredValue, compile, compile_with_options, execute,
    lower_to_bytecode,
};

#[test]
fn loose_equality_fails_closed_in_source_and_serialized_bytecode() {
    for source in [
        "undefined == null;",
        "'1' == 1;",
        "1 != '1';",
        "if (false) { 0 == 0; }",
    ] {
        for lenient_mode in [false, true] {
            let error = compile_with_options(source, CompileOptions { lenient_mode })
                .expect_err("loose equality must not silently become strict equality");
            assert!(error.to_string().contains("loose equality"), "{error}");
            assert!(
                error.to_string().contains("["),
                "diagnostic must include a span"
            );
        }
    }
    for operator in [BinaryOp::Eq, BinaryOp::NotEq] {
        let mut bytecode = lower_to_bytecode(&compile("1 === 1;").unwrap()).unwrap();
        let instruction = bytecode.functions[bytecode.root]
            .code
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::Binary(_)))
            .unwrap();
        *instruction = Instruction::Binary(operator);
        let bytes = dump_program(&bytecode).unwrap();
        assert!(
            load_program(&bytes)
                .unwrap_err()
                .to_string()
                .contains("loose equality")
        );
        assert!(
            start_bytecode(&bytecode, ExecutionOptions::default())
                .unwrap_err()
                .to_string()
                .contains("loose equality")
        );
    }
    let program = compile("undefined !== null && '1' !== 1 && Number('1') === 1;").unwrap();
    assert_eq!(
        execute(&program, ExecutionOptions::default()).unwrap(),
        StructuredValue::Bool(true)
    );
}

#[test]
fn classic_for_copies_lexical_cells_before_the_first_test_and_each_update() {
    let source = r#"
        const body = [], updates = []; let initial;
        for (let [i, j] = [0, 10], get = () => i; i < 3; updates.push(() => i), i++, j++) {
            initial = get; body.push(() => [i, j]);
        }
        JSON.stringify([initial(), body.map(f => f()), updates.map(f => f())]);
    "#;
    let program = compile(source).unwrap();
    assert_eq!(
        execute(&program, ExecutionOptions::default()).unwrap(),
        StructuredValue::String("[0,[[0,10],[1,11],[2,12]],[1,2,3]]".into())
    );
}

#[test]
fn classic_for_preserves_header_tdz_const_and_continue_cleanup() {
    let source = r#"
        const f = [], errors = [];
        outer: for (let i = 0; i < 4; i++) {
            try { for (let j = 0; j < 2; j++) { f.push(() => [i,j]); continue outer; } }
            finally { i++; }
        }
        globalThis.later = 9;
        try { for (let first = later, later = 1; false;) {} } catch(e) { errors.push(e.name); }
        try { for (const i = 0; i < 1; i++) {} } catch(e) { errors.push(e.name); }
        JSON.stringify([f.map(fn => fn()), errors]);
    "#;
    let program = compile(source).unwrap();
    assert_eq!(
        execute(&program, ExecutionOptions::default()).unwrap(),
        StructuredValue::String("[[[1,0],[3,0]],[\"ReferenceError\",\"TypeError\"]]".into())
    );
}

#[test]
fn compound_statements_supply_script_completion_values() {
    for (source, expected) in [
        ("if(true) {42;}", 42.0),
        ("try {42;} catch(e) {0;}", 42.0),
        ("try {throw 1;} catch(e) {42;} finally {0;}", 42.0),
        ("switch(2) {default: 1; break; case 2: 42; break;}", 42.0),
        ("switch(1) {case 1: const x=42; x; break;}", 42.0),
        ("42; const ignored=0; {}", 42.0),
        ("for(let i=0;i<3;i++) {i;}", 2.0),
        ("label: {try {42; break label;} finally {0;}}", 42.0),
        ("label: {try {1;} finally {42; break label;}}", 42.0),
    ] {
        for lenient_mode in [false, true] {
            let program = compile_with_options(source, CompileOptions { lenient_mode }).unwrap();
            assert_eq!(
                execute(&program, ExecutionOptions::default()).unwrap(),
                StructuredValue::from(expected),
                "{source}"
            );
        }
    }
    for source in [
        "42; if(false) {1;}",
        "42; while(false) {}",
        "42; switch(0) {}",
        "42; try {} finally {1;}",
        "try {42; throw 1;} catch(e) {}",
        "label: {try {42;} finally {break label;}}",
    ] {
        let program = compile(source).unwrap();
        assert_eq!(
            execute(&program, ExecutionOptions::default()).unwrap(),
            StructuredValue::Undefined,
            "{source}"
        );
    }
    let program = compile("\"directive result\"; const ignored=0;").unwrap();
    assert_eq!(
        execute(&program, ExecutionOptions::default()).unwrap(),
        StructuredValue::String("directive result".into())
    );
}
