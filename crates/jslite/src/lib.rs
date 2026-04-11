pub mod diagnostic;
pub mod ir;
pub mod limits;
pub mod parser;
pub mod runtime;
pub mod span;
pub mod structured;

pub use diagnostic::{Diagnostic, DiagnosticKind, JsliteError, JsliteResult};
pub use ir::CompiledProgram;
pub use limits::RuntimeLimits;
pub use parser::compile;
pub use runtime::{
    BytecodeProgram, ExecutionOptions, ExecutionSnapshot, ExecutionStep, HostError, ResumePayload,
    Suspension, dump_program, dump_snapshot, execute, load_program, load_snapshot,
    lower_to_bytecode, resume, start, start_bytecode,
};
pub use structured::StructuredValue;
