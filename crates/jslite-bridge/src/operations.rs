use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use jslite::{
    BytecodeProgram, CancellationToken, ExecutionOptions, ResumeOptions, SnapshotInspection,
    compile, dump_program, inspect_snapshot as inspect_loaded_snapshot, load_snapshot,
    lower_to_bytecode, resume_with_options, start_bytecode,
};
use sha2::Sha256;

use crate::{
    codec::encode_step,
    dto::{ResumeDto, SnapshotPolicyDto, StartOptionsDto, StepDto},
};

type HmacSha256 = Hmac<Sha256>;

fn encode_snapshot_token(snapshot: &[u8], snapshot_key: &[u8]) -> Result<String> {
    let mut mac =
        HmacSha256::new_from_slice(snapshot_key).map_err(|_| anyhow!("invalid snapshot key"))?;
    mac.update(snapshot);
    let digest = mac.finalize().into_bytes();
    let mut token = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    Ok(token)
}

fn assert_authenticated_snapshot(snapshot_bytes: &[u8], policy: &SnapshotPolicyDto) -> Result<()> {
    let snapshot_key_base64 = policy
        .snapshot_key_base64
        .as_deref()
        .ok_or_else(|| anyhow!("raw snapshot restore requires snapshot_key_base64"))?;
    let snapshot_token = policy
        .snapshot_token
        .as_deref()
        .ok_or_else(|| anyhow!("raw snapshot restore requires snapshot_token"))?;
    let snapshot_key = STANDARD
        .decode(snapshot_key_base64)
        .map_err(|_| anyhow!("snapshot_key_base64 must be valid base64"))?;
    let expected = encode_snapshot_token(snapshot_bytes, &snapshot_key)?;
    if expected != snapshot_token {
        return Err(anyhow!(
            "raw snapshot restore rejected a tampered or unauthenticated snapshot"
        ));
    }
    Ok(())
}

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
    assert_authenticated_snapshot(snapshot_bytes, &policy)?;
    let mut snapshot = load_snapshot(snapshot_bytes)?;
    inspect_loaded_snapshot(&mut snapshot, policy.into_snapshot_policy()).map_err(Into::into)
}

pub fn resume_program(
    snapshot_bytes: &[u8],
    payload: ResumeDto,
    policy: SnapshotPolicyDto,
    cancellation_token: Option<CancellationToken>,
) -> Result<StepDto> {
    assert_authenticated_snapshot(snapshot_bytes, &policy)?;
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
