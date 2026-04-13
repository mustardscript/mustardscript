use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

use crate::{
    cancellation::CancellationToken,
    diagnostic::{DiagnosticKind, MustardError, MustardResult},
    ir::CompiledProgram,
    limits::RuntimeLimits,
    structured::StructuredValue,
};

use super::{
    Runtime,
    bytecode::BytecodeProgram,
    lower_to_bytecode,
    validation::{validate_bytecode_program, validate_snapshot},
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
    pub metrics: RuntimeDebugMetrics,
}

#[derive(Debug, Clone, Default)]
pub struct ResumeOptions {
    pub cancellation_token: Option<CancellationToken>,
    pub snapshot_policy: Option<SnapshotPolicy>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDebugMetrics {
    pub gc_collections: u64,
    pub gc_total_time_ns: u64,
    pub gc_reclaimed_bytes: u64,
    pub gc_reclaimed_allocations: u64,
    pub accounting_refreshes: u64,
}

#[derive(Debug, Clone)]
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

#[derive(Serialize)]
struct SerializableExecutionSnapshot<'a> {
    runtime: &'a Runtime,
}

#[derive(Deserialize)]
struct DeserializableExecutionSnapshot {
    runtime: Runtime,
}

impl ExecutionSnapshot {
    pub(in crate::runtime) fn capture(runtime: &mut Runtime) -> Self {
        let placeholder = Runtime::blank(Arc::clone(&runtime.program), runtime.limits, None);
        Self {
            runtime: std::mem::replace(runtime, placeholder),
        }
    }

    pub(in crate::runtime) fn restore_loaded_runtime(runtime: Runtime) -> MustardResult<Self> {
        let mut snapshot = Self { runtime };
        validate_snapshot(&snapshot)?;
        snapshot.runtime.recompute_accounting_after_load()?;
        snapshot.runtime.snapshot_policy_required = true;
        Ok(snapshot)
    }
}

impl Serialize for ExecutionSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SerializableExecutionSnapshot {
            runtime: &self.runtime,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExecutionSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let decoded = DeserializableExecutionSnapshot::deserialize(deserializer)?;
        Self::restore_loaded_runtime(decoded.runtime).map_err(serde::de::Error::custom)
    }
}

pub fn execute(
    program: &CompiledProgram,
    options: ExecutionOptions,
) -> MustardResult<StructuredValue> {
    match start(program, options)? {
        ExecutionStep::Completed(value) => Ok(value),
        ExecutionStep::Suspended(suspension) => Err(MustardError::runtime(format!(
            "execution suspended on capability `{}`; use start()/resume() for iterative execution",
            suspension.capability
        ))),
    }
}

pub fn start(program: &CompiledProgram, options: ExecutionOptions) -> MustardResult<ExecutionStep> {
    let bytecode = lower_to_bytecode(program)?;
    start_validated_bytecode(&bytecode, options)
}

pub fn start_bytecode(
    program: &BytecodeProgram,
    options: ExecutionOptions,
) -> MustardResult<ExecutionStep> {
    validate_bytecode_program(program)?;
    start_validated_bytecode(program, options)
}

pub fn start_validated_bytecode(
    program: &BytecodeProgram,
    options: ExecutionOptions,
) -> MustardResult<ExecutionStep> {
    start_shared_bytecode(Arc::new(program.clone()), options)
}

pub fn start_shared_bytecode(
    program: Arc<BytecodeProgram>,
    options: ExecutionOptions,
) -> MustardResult<ExecutionStep> {
    let mut runtime = Runtime::new(program, options)?;
    runtime.run_root()
}

pub fn start_shared_bytecode_with_metrics(
    program: Arc<BytecodeProgram>,
    options: ExecutionOptions,
) -> MustardResult<(ExecutionStep, RuntimeDebugMetrics)> {
    let mut runtime = Runtime::new(program, options)?;
    let step = runtime.run_root()?;
    let metrics = metrics_after_step(&runtime, &step);
    Ok((step, metrics))
}

pub fn resume(snapshot: ExecutionSnapshot, payload: ResumePayload) -> MustardResult<ExecutionStep> {
    resume_with_options(snapshot, payload, ResumeOptions::default())
}

pub fn resume_with_options(
    snapshot: ExecutionSnapshot,
    payload: ResumePayload,
    options: ResumeOptions,
) -> MustardResult<ExecutionStep> {
    let mut runtime = snapshot.runtime;
    runtime.apply_resume_options(options)?;
    runtime.resume(payload)
}

pub fn resume_with_options_and_metrics(
    snapshot: ExecutionSnapshot,
    payload: ResumePayload,
    options: ResumeOptions,
) -> MustardResult<(ExecutionStep, RuntimeDebugMetrics)> {
    let mut runtime = snapshot.runtime;
    runtime.apply_resume_options(options)?;
    let step = runtime.resume(payload)?;
    let metrics = metrics_after_step(&runtime, &step);
    Ok((step, metrics))
}

pub fn apply_snapshot_policy(
    snapshot: &mut ExecutionSnapshot,
    policy: SnapshotPolicy,
) -> MustardResult<()> {
    snapshot.runtime.apply_snapshot_policy(policy)
}

pub fn snapshot_inspection(snapshot: &ExecutionSnapshot) -> MustardResult<SnapshotInspection> {
    let request = snapshot
        .runtime
        .suspended_host_call
        .as_ref()
        .ok_or_else(|| MustardError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot is not suspended on a host capability".to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    Ok(SnapshotInspection {
        capability: request.capability.clone(),
        args: request.args.clone(),
        metrics: snapshot.runtime.debug_metrics(),
    })
}

pub fn inspect_snapshot(
    snapshot: &mut ExecutionSnapshot,
    policy: SnapshotPolicy,
) -> MustardResult<SnapshotInspection> {
    apply_snapshot_policy(snapshot, policy)?;
    snapshot_inspection(snapshot)
}

fn metrics_after_step(runtime: &Runtime, step: &ExecutionStep) -> RuntimeDebugMetrics {
    match step {
        ExecutionStep::Completed(_) => runtime.debug_metrics(),
        ExecutionStep::Suspended(suspension) => suspension.snapshot.runtime.debug_metrics(),
    }
}
