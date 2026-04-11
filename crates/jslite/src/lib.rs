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
pub use runtime::{BytecodeProgram, ExecutionOptions, execute, lower_to_bytecode};
pub use structured::StructuredValue;
