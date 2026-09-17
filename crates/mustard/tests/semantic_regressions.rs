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
