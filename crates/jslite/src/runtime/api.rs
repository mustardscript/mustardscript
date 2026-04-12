use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    cancellation::CancellationToken,
    diagnostic::{DiagnosticKind, JsliteError, JsliteResult},
    ir::CompiledProgram,
    limits::RuntimeLimits,
    structured::StructuredValue,
};

use super::{
    Runtime, bytecode::BytecodeProgram, lower_to_bytecode, validation::validate_bytecode_program,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    pub inputs: IndexMap<String, StructuredValue>,
    pub capabilities: Vec<String>,
    pub limits: RuntimeLimits,
    #[serde(skip, default)]
    pub cancellation_token: Option<CancellationToken>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostError {
    pub name: String,
    pub message: String,
    pub code: Option<String>,
    pub details: Option<StructuredValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResumePayload {
    Value(StructuredValue),
    Error(HostError),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPolicy {
    pub capabilities: Vec<String>,
    pub limits: RuntimeLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInspection {
    pub capability: String,
    pub args: Vec<StructuredValue>,
}

#[derive(Debug, Clone, Default)]
pub struct ResumeOptions {
    pub cancellation_token: Option<CancellationToken>,
    pub snapshot_policy: Option<SnapshotPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSnapshot {
    pub(super) runtime: Runtime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suspension {
    pub capability: String,
    pub args: Vec<StructuredValue>,
    pub snapshot: ExecutionSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStep {
    Completed(StructuredValue),
    Suspended(Box<Suspension>),
}

pub fn execute(
    program: &CompiledProgram,
    options: ExecutionOptions,
) -> JsliteResult<StructuredValue> {
    match start(program, options)? {
        ExecutionStep::Completed(value) => Ok(value),
        ExecutionStep::Suspended(suspension) => Err(JsliteError::runtime(format!(
            "execution suspended on capability `{}`; use start()/resume() for iterative execution",
            suspension.capability
        ))),
    }
}

pub fn start(program: &CompiledProgram, options: ExecutionOptions) -> JsliteResult<ExecutionStep> {
    let bytecode = lower_to_bytecode(program)?;
    start_bytecode(&bytecode, options)
}

pub fn start_bytecode(
    program: &BytecodeProgram,
    options: ExecutionOptions,
) -> JsliteResult<ExecutionStep> {
    validate_bytecode_program(program)?;
    let mut runtime = Runtime::new(program.clone(), options)?;
    runtime.run_root()
}

pub fn resume(snapshot: ExecutionSnapshot, payload: ResumePayload) -> JsliteResult<ExecutionStep> {
    resume_with_options(snapshot, payload, ResumeOptions::default())
}

pub fn resume_with_options(
    snapshot: ExecutionSnapshot,
    payload: ResumePayload,
    options: ResumeOptions,
) -> JsliteResult<ExecutionStep> {
    let mut runtime = snapshot.runtime;
    runtime.apply_resume_options(options)?;
    runtime.resume(payload)
}

pub fn inspect_snapshot(
    snapshot: &mut ExecutionSnapshot,
    policy: SnapshotPolicy,
) -> JsliteResult<SnapshotInspection> {
    snapshot.runtime.apply_snapshot_policy(policy)?;
    let request = snapshot
        .runtime
        .suspended_host_call
        .as_ref()
        .ok_or_else(|| JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot is not suspended on a host capability".to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    Ok(SnapshotInspection {
        capability: request.capability.clone(),
        args: request.args.clone(),
    })
}
