use super::*;
use crate::compile;

mod async_host;
mod collections;
mod diagnostics;
mod exceptions;
mod execution;
mod gc;
mod serialization;

fn test_function(code: Vec<Instruction>) -> FunctionPrototype {
    FunctionPrototype {
        name: None,
        params: Vec::new(),
        rest: None,
        code,
        is_async: false,
        is_arrow: false,
        span: SourceSpan::new(0, 0),
    }
}

fn invalid_program(code: Vec<Instruction>) -> BytecodeProgram {
    BytecodeProgram {
        functions: vec![test_function(code)],
        root: 0,
    }
}

fn run(source: &str) -> StructuredValue {
    let program = compile(source).expect("source should compile");
    execute(&program, ExecutionOptions::default()).expect("program should run")
}
