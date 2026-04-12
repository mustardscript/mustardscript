use anyhow::Result;
use jslite::{
    BytecodeProgram, CancellationToken, ExecutionOptions, ResumeOptions, SnapshotInspection,
    compile, dump_program, inspect_snapshot as inspect_loaded_snapshot, load_snapshot,
    lower_to_bytecode, resume_with_options, start_bytecode,
};

use crate::{
    codec::encode_step,
    dto::{ResumeDto, SnapshotPolicyDto, StartOptionsDto, StepDto},
};

pub fn compile_program_bytes(source: &str) -> Result<Vec<u8>> {
    let parsed = compile(source)?;
    let bytecode = lower_to_bytecode(&parsed)?;
    dump_program(&bytecode).map_err(Into::into)
}

pub fn start_program(
    program: &BytecodeProgram,
    options: StartOptionsDto,
    cancellation_token: Option<CancellationToken>,
) -> Result<StepDto> {
    let step = start_bytecode(
        program,
        ExecutionOptions {
            inputs: options.inputs.into_iter().collect(),
            capabilities: options.capabilities,
            limits: options.limits.into_runtime_limits(),
            cancellation_token,
        },
    )?;
    encode_step(step)
}

pub fn inspect_snapshot_bytes(
    snapshot_bytes: &[u8],
    policy: SnapshotPolicyDto,
) -> Result<SnapshotInspection> {
    let mut snapshot = load_snapshot(snapshot_bytes)?;
    inspect_loaded_snapshot(&mut snapshot, policy.into_snapshot_policy()).map_err(Into::into)
}

pub fn resume_program(
    snapshot_bytes: &[u8],
    payload: ResumeDto,
    policy: SnapshotPolicyDto,
    cancellation_token: Option<CancellationToken>,
) -> Result<StepDto> {
    let snapshot = load_snapshot(snapshot_bytes)?;
    let step = resume_with_options(
        snapshot,
        payload.into_resume_payload(),
        ResumeOptions {
            cancellation_token,
            snapshot_policy: Some(policy.into_snapshot_policy()),
        },
    )?;
    encode_step(step)
}
