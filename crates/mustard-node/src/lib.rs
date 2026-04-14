mod boundary_binary;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use indexmap::IndexMap;
use rand::random;
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use mustard::{
    BytecodeProgram, CancellationToken, ExecutionOptions, ExecutionSnapshot, ExecutionStep,
    ResumeOptions, RuntimeDebugMetrics, RuntimeLimits, StructuredValue, apply_snapshot_policy,
    compile, dump_detached_snapshot, dump_program as encode_program_bytes, dump_snapshot,
    load_detached_snapshot, load_snapshot, lower_to_bytecode, resume_with_options_and_metrics,
    snapshot_inspection, start_shared_bytecode_with_metrics,
};
use mustard_bridge::{
    ResumeDto, RuntimeLimitsDto, SnapshotPolicyDto, StartOptionsDto, decode_program, encode_json,
    inspect_detached_snapshot_bytes, inspect_snapshot_bytes, parse_json,
    resume_detached_program as bridge_resume_detached_program,
    resume_program as bridge_resume_program,
    start_shared_program_detached as bridge_start_shared_program_detached,
};
use napi::bindgen_prelude::Buffer;
use napi::{Error, Result};
use napi_derive::napi;

use crate::boundary_binary::{
    decode_resume_payload_bytes, decode_start_options_bytes, decode_structured_inputs_bytes,
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
struct CompiledProgramEntry {
    program: Arc<BytecodeProgram>,
    serialized: Vec<u8>,
    ref_count: usize,
}

#[derive(Debug, Clone)]
struct ExecutionContextEntry {
    capabilities: Arc<Vec<String>>,
    limits: RuntimeLimits,
}

#[derive(Debug, Deserialize)]
struct ExecutionContextDto {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    limits: RuntimeLimitsDto,
}

#[derive(Debug, Clone, Copy)]
enum SnapshotHandleFormat {
    SelfContained,
    Detached,
}

#[derive(Debug)]
struct StoredSnapshotEntry {
    snapshot: ExecutionSnapshot,
    format: SnapshotHandleFormat,
}

struct SnapshotAuth<'a> {
    snapshot_id: &'a str,
    snapshot_key_base64: &'a str,
    snapshot_token: &'a str,
    snapshot_key_digest: &'a str,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NodeStepDto {
    Completed {
        value: StructuredValue,
        metrics: RuntimeDebugMetrics,
    },
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
        snapshot_handle: String,
        metrics: RuntimeDebugMetrics,
    },
}

fn cancellation_tokens() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static TOKENS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn compiled_programs() -> &'static Mutex<HashMap<String, CompiledProgramEntry>> {
    static PROGRAMS: OnceLock<Mutex<HashMap<String, CompiledProgramEntry>>> = OnceLock::new();
    PROGRAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn execution_contexts() -> &'static Mutex<HashMap<String, ExecutionContextEntry>> {
    static CONTEXTS: OnceLock<Mutex<HashMap<String, ExecutionContextEntry>>> = OnceLock::new();
    CONTEXTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stored_snapshots() -> &'static Mutex<HashMap<String, StoredSnapshotEntry>> {
    static SNAPSHOTS: OnceLock<Mutex<HashMap<String, StoredSnapshotEntry>>> = OnceLock::new();
    SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_cancellation_token_id(tokens: &HashMap<String, Arc<AtomicBool>>) -> String {
    loop {
        let candidate = format!("cancel-{:032x}", random::<u128>());
        if !tokens.contains_key(&candidate) {
            return candidate;
        }
    }
}

fn next_program_handle_id(programs: &HashMap<String, CompiledProgramEntry>) -> String {
    loop {
        let candidate = format!("program-{:032x}", random::<u128>());
        if !programs.contains_key(&candidate) {
            return candidate;
        }
    }
}

fn next_execution_context_handle_id(contexts: &HashMap<String, ExecutionContextEntry>) -> String {
    loop {
        let candidate = format!("context-{:032x}", random::<u128>());
        if !contexts.contains_key(&candidate) {
            return candidate;
        }
    }
}

fn next_snapshot_handle_id(snapshots: &HashMap<String, StoredSnapshotEntry>) -> String {
    loop {
        let candidate = format!("snapshot-{:032x}", random::<u128>());
        if !snapshots.contains_key(&candidate) {
            return candidate;
        }
    }
}

fn lookup_cancellation_token(token_id: Option<String>) -> Result<Option<CancellationToken>> {
    let Some(token_id) = token_id else {
        return Ok(None);
    };
    let tokens = cancellation_tokens()
        .lock()
        .map_err(|_| to_napi_error("cancellation token registry is poisoned"))?;
    let shared = tokens
        .get(&token_id)
        .cloned()
        .ok_or_else(|| to_napi_error(format!("unknown cancellation token `{token_id}`")))?;
    Ok(Some(CancellationToken::from_shared(shared)))
}

fn insert_program(program: BytecodeProgram) -> Result<String> {
    let mut programs = compiled_programs()
        .lock()
        .map_err(|_| to_napi_error("compiled program registry is poisoned"))?;
    let handle = next_program_handle_id(&programs);
    let serialized = encode_program_bytes(&program).map_err(to_napi_error)?;
    programs.insert(
        handle.clone(),
        CompiledProgramEntry {
            program: Arc::new(program),
            serialized,
            ref_count: 1,
        },
    );
    Ok(handle)
}

fn insert_execution_context(context: ExecutionContextEntry) -> Result<String> {
    let mut contexts = execution_contexts()
        .lock()
        .map_err(|_| to_napi_error("execution context registry is poisoned"))?;
    let handle = next_execution_context_handle_id(&contexts);
    contexts.insert(handle.clone(), context);
    Ok(handle)
}

fn insert_snapshot(snapshot: ExecutionSnapshot, format: SnapshotHandleFormat) -> Result<String> {
    let mut snapshots = stored_snapshots()
        .lock()
        .map_err(|_| to_napi_error("snapshot registry is poisoned"))?;
    let handle = next_snapshot_handle_id(&snapshots);
    snapshots.insert(handle.clone(), StoredSnapshotEntry { snapshot, format });
    Ok(handle)
}

fn release_execution_context_internal(context_handle: &str) -> Result<()> {
    let mut contexts = execution_contexts()
        .lock()
        .map_err(|_| to_napi_error("execution context registry is poisoned"))?;
    contexts.remove(context_handle);
    Ok(())
}

fn release_snapshot_handle_internal(snapshot_handle: &str) -> Result<()> {
    let mut snapshots = stored_snapshots()
        .lock()
        .map_err(|_| to_napi_error("snapshot registry is poisoned"))?;
    snapshots.remove(snapshot_handle);
    Ok(())
}

fn take_snapshot(snapshot_handle: &str) -> Result<StoredSnapshotEntry> {
    let mut snapshots = stored_snapshots()
        .lock()
        .map_err(|_| to_napi_error("snapshot registry is poisoned"))?;
    snapshots
        .remove(snapshot_handle)
        .ok_or_else(|| to_napi_error(format!("unknown snapshot handle `{snapshot_handle}`")))
}

fn with_snapshot<T, F>(snapshot_handle: &str, f: F) -> Result<T>
where
    F: FnOnce(&ExecutionSnapshot) -> Result<T>,
{
    let snapshots = stored_snapshots()
        .lock()
        .map_err(|_| to_napi_error("snapshot registry is poisoned"))?;
    let entry = snapshots
        .get(snapshot_handle)
        .ok_or_else(|| to_napi_error(format!("unknown snapshot handle `{snapshot_handle}`")))?;
    f(&entry.snapshot)
}

fn with_snapshot_entry<T, F>(snapshot_handle: &str, f: F) -> Result<T>
where
    F: FnOnce(&StoredSnapshotEntry) -> Result<T>,
{
    let snapshots = stored_snapshots()
        .lock()
        .map_err(|_| to_napi_error("snapshot registry is poisoned"))?;
    let entry = snapshots
        .get(snapshot_handle)
        .ok_or_else(|| to_napi_error(format!("unknown snapshot handle `{snapshot_handle}`")))?;
    f(entry)
}

fn lookup_program(handle: &str) -> Result<Arc<BytecodeProgram>> {
    let programs = compiled_programs()
        .lock()
        .map_err(|_| to_napi_error("compiled program registry is poisoned"))?;
    programs
        .get(handle)
        .map(|entry| Arc::clone(&entry.program))
        .ok_or_else(|| to_napi_error(format!("unknown compiled program handle `{handle}`")))
}

fn lookup_serialized_program(handle: &str) -> Result<Vec<u8>> {
    let programs = compiled_programs()
        .lock()
        .map_err(|_| to_napi_error("compiled program registry is poisoned"))?;
    programs
        .get(handle)
        .map(|entry| entry.serialized.clone())
        .ok_or_else(|| to_napi_error(format!("unknown compiled program handle `{handle}`")))
}

fn lookup_execution_context(handle: &str) -> Result<ExecutionContextEntry> {
    let contexts = execution_contexts()
        .lock()
        .map_err(|_| to_napi_error("execution context registry is poisoned"))?;
    contexts
        .get(handle)
        .cloned()
        .ok_or_else(|| to_napi_error(format!("unknown execution context handle `{handle}`")))
}

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}

fn execution_options(
    options: StartOptionsDto,
    cancellation_token: Option<CancellationToken>,
) -> ExecutionOptions {
    ExecutionOptions {
        inputs: options.inputs.into_iter().collect(),
        capabilities: options.capabilities,
        limits: options.limits.into_runtime_limits(),
        cancellation_token,
    }
}

fn snapshot_identity_hex(snapshot: &[u8]) -> String {
    let digest = Sha256::digest(snapshot);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn snapshot_key_digest_hex(snapshot_key: &[u8]) -> String {
    let digest = Sha256::digest(snapshot_key);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn encode_snapshot_token(snapshot_id: &str, snapshot_key: &[u8]) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(snapshot_key)
        .map_err(|_| to_napi_error("invalid snapshot key"))?;
    mac.update(snapshot_id.as_bytes());
    let digest = mac.finalize().into_bytes();
    let mut token = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    Ok(token)
}

fn parse_execution_context(policy_json: String) -> Result<ExecutionContextEntry> {
    let context: ExecutionContextDto = parse_json(&policy_json).map_err(to_napi_error)?;
    Ok(ExecutionContextEntry {
        capabilities: Arc::new(context.capabilities),
        limits: context.limits.into_runtime_limits(),
    })
}

fn assert_authenticated_snapshot(snapshot: &[u8], auth: SnapshotAuth<'_>) -> Result<()> {
    let snapshot_key = STANDARD
        .decode(auth.snapshot_key_base64)
        .map_err(|_| to_napi_error("snapshot_key_base64 must be valid base64"))?;
    let expected_snapshot_id = snapshot_identity_hex(snapshot);
    if expected_snapshot_id != auth.snapshot_id {
        return Err(to_napi_error(
            "raw snapshot restore rejected a tampered or unauthenticated snapshot",
        ));
    }
    if snapshot_key_digest_hex(&snapshot_key) != auth.snapshot_key_digest {
        return Err(to_napi_error(
            "raw snapshot restore rejected a mismatched snapshot key digest",
        ));
    }
    let expected = encode_snapshot_token(auth.snapshot_id, &snapshot_key)?;
    if expected != auth.snapshot_token {
        return Err(to_napi_error(
            "raw snapshot restore rejected a tampered or unauthenticated snapshot",
        ));
    }
    Ok(())
}

fn elapsed_nanos(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

fn node_step_with_snapshot_handle(
    step: ExecutionStep,
    metrics: RuntimeDebugMetrics,
    format: SnapshotHandleFormat,
) -> Result<NodeStepDto> {
    match step {
        ExecutionStep::Completed(value) => Ok(NodeStepDto::Completed { value, metrics }),
        ExecutionStep::Suspended(suspension) => {
            let handle = insert_snapshot(suspension.snapshot, format)?;
            Ok(NodeStepDto::Suspended {
                capability: suspension.capability,
                args: suspension.args,
                snapshot_handle: handle,
                metrics,
            })
        }
    }
}

fn release_node_step_snapshot_handle(step: &NodeStepDto) {
    if let NodeStepDto::Suspended {
        snapshot_handle, ..
    } = step
    {
        let _ = release_snapshot_handle_internal(snapshot_handle);
    }
}

fn encode_step_with_snapshot_handle(
    step: ExecutionStep,
    metrics: RuntimeDebugMetrics,
    format: SnapshotHandleFormat,
) -> Result<String> {
    let step = node_step_with_snapshot_handle(step, metrics, format)?;
    let result = encode_json(&step).map_err(to_napi_error);
    if result.is_err() {
        release_node_step_snapshot_handle(&step);
    }
    result
}

fn encode_profiled_step_with_snapshot_handle(
    step: ExecutionStep,
    metrics: RuntimeDebugMetrics,
    format: SnapshotHandleFormat,
    parse_ns: u64,
    execute_ns: u64,
) -> Result<String> {
    let step = node_step_with_snapshot_handle(step, metrics, format)?;
    let encode_started = Instant::now();
    let step_json = encode_json(&step).map_err(to_napi_error);
    let encode_ns = elapsed_nanos(encode_started);
    match step_json {
        Ok(step_json) => Ok(format!(
            "{{\"step\":{step_json},\"profile\":{{\"parse_ns\":{parse_ns},\"execute_ns\":{execute_ns},\"encode_ns\":{encode_ns}}}}}"
        )),
        Err(error) => {
            release_node_step_snapshot_handle(&step);
            Err(error)
        }
    }
}

fn policy_from_json(policy_json: String) -> Result<SnapshotPolicyDto> {
    parse_json(&policy_json).map_err(to_napi_error)
}

fn authenticated_snapshot_policy(
    snapshot_bytes: &[u8],
    policy: SnapshotPolicyDto,
) -> Result<mustard::SnapshotPolicy> {
    let snapshot_id = policy
        .snapshot_id
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_id"))?;
    let snapshot_key_base64 = policy
        .snapshot_key_base64
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_base64"))?;
    let snapshot_token = policy
        .snapshot_token
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_token"))?;
    let snapshot_key_digest = policy
        .snapshot_key_digest
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_digest"))?;
    assert_authenticated_snapshot(
        snapshot_bytes,
        SnapshotAuth {
            snapshot_id,
            snapshot_key_base64,
            snapshot_token,
            snapshot_key_digest,
        },
    )?;
    policy.into_snapshot_policy().map_err(to_napi_error)
}

fn load_snapshot_handle_impl(
    snapshot_bytes: &[u8],
    policy: SnapshotPolicyDto,
    format: SnapshotHandleFormat,
) -> Result<String> {
    let snapshot_policy = authenticated_snapshot_policy(snapshot_bytes, policy)?;
    let mut snapshot = match format {
        SnapshotHandleFormat::SelfContained => {
            load_snapshot(snapshot_bytes).map_err(to_napi_error)?
        }
        SnapshotHandleFormat::Detached => {
            return Err(to_napi_error(
                "detached snapshot loads require a compiled program binding",
            ));
        }
    };
    apply_snapshot_policy(&mut snapshot, snapshot_policy).map_err(to_napi_error)?;
    insert_snapshot(snapshot, format)
}

fn load_detached_snapshot_handle_impl(
    snapshot_bytes: &[u8],
    program: Arc<BytecodeProgram>,
    policy: SnapshotPolicyDto,
) -> Result<String> {
    let snapshot_policy = authenticated_snapshot_policy(snapshot_bytes, policy)?;
    let mut snapshot = load_detached_snapshot(snapshot_bytes, program).map_err(to_napi_error)?;
    apply_snapshot_policy(&mut snapshot, snapshot_policy).map_err(to_napi_error)?;
    insert_snapshot(snapshot, SnapshotHandleFormat::Detached)
}

fn execution_options_from_context(
    context: &ExecutionContextEntry,
    inputs: IndexMap<String, StructuredValue>,
    cancellation_token: Option<CancellationToken>,
) -> ExecutionOptions {
    ExecutionOptions {
        inputs,
        capabilities: context.capabilities.as_ref().clone(),
        limits: context.limits,
        cancellation_token,
    }
}

fn load_snapshot_handle_from_context(
    snapshot_bytes: &[u8],
    context: &ExecutionContextEntry,
    auth: SnapshotAuth<'_>,
    format: SnapshotHandleFormat,
) -> Result<String> {
    assert_authenticated_snapshot(snapshot_bytes, auth)?;
    let snapshot_policy = mustard::SnapshotPolicy {
        capabilities: context.capabilities.as_ref().clone(),
        limits: context.limits,
    };
    let mut snapshot = match format {
        SnapshotHandleFormat::SelfContained => {
            load_snapshot(snapshot_bytes).map_err(to_napi_error)?
        }
        SnapshotHandleFormat::Detached => {
            return Err(to_napi_error(
                "detached snapshot loads require a compiled program binding",
            ));
        }
    };
    apply_snapshot_policy(&mut snapshot, snapshot_policy).map_err(to_napi_error)?;
    insert_snapshot(snapshot, format)
}

fn load_detached_snapshot_handle_from_context(
    snapshot_bytes: &[u8],
    program: Arc<BytecodeProgram>,
    context: &ExecutionContextEntry,
    auth: SnapshotAuth<'_>,
) -> Result<String> {
    assert_authenticated_snapshot(snapshot_bytes, auth)?;
    let snapshot_policy = mustard::SnapshotPolicy {
        capabilities: context.capabilities.as_ref().clone(),
        limits: context.limits,
    };
    let mut snapshot = load_detached_snapshot(snapshot_bytes, program).map_err(to_napi_error)?;
    apply_snapshot_policy(&mut snapshot, snapshot_policy).map_err(to_napi_error)?;
    insert_snapshot(snapshot, SnapshotHandleFormat::Detached)
}

#[napi]
pub fn compile_program(source: String) -> Result<String> {
    let parsed = compile(&source).map_err(to_napi_error)?;
    let bytecode = lower_to_bytecode(&parsed).map_err(to_napi_error)?;
    insert_program(bytecode)
}

#[napi]
pub fn create_execution_context(policy_json: String) -> Result<String> {
    insert_execution_context(parse_execution_context(policy_json)?)
}

#[napi]
pub fn load_program(program: Buffer) -> Result<String> {
    let program = decode_program(program.as_ref()).map_err(to_napi_error)?;
    insert_program(program)
}

#[napi]
pub fn dump_program(program_handle: String) -> Result<Buffer> {
    Ok(Buffer::from(lookup_serialized_program(&program_handle)?))
}

#[napi]
pub fn retain_program(program_handle: String) -> Result<String> {
    let mut programs = compiled_programs()
        .lock()
        .map_err(|_| to_napi_error("compiled program registry is poisoned"))?;
    let entry = programs.get_mut(&program_handle).ok_or_else(|| {
        to_napi_error(format!(
            "unknown compiled program handle `{program_handle}`"
        ))
    })?;
    entry.ref_count = entry
        .ref_count
        .checked_add(1)
        .ok_or_else(|| to_napi_error("compiled program handle retain count overflow"))?;
    Ok(program_handle)
}

#[napi]
pub fn release_program(program_handle: String) -> Result<()> {
    let mut programs = compiled_programs()
        .lock()
        .map_err(|_| to_napi_error("compiled program registry is poisoned"))?;
    let should_remove = match programs.get_mut(&program_handle) {
        Some(entry) => {
            if entry.ref_count > 1 {
                entry.ref_count -= 1;
                false
            } else {
                true
            }
        }
        None => false,
    };
    if should_remove {
        programs.remove(&program_handle);
    }
    Ok(())
}

#[napi]
pub fn release_execution_context(context_handle: String) -> Result<()> {
    release_execution_context_internal(&context_handle)
}

#[napi]
pub fn release_snapshot_handle(snapshot_handle: String) -> Result<()> {
    release_snapshot_handle_internal(&snapshot_handle)
}

#[napi]
pub fn dump_snapshot_handle(snapshot_handle: String) -> Result<Buffer> {
    with_snapshot_entry(&snapshot_handle, |entry| {
        let bytes = match entry.format {
            SnapshotHandleFormat::SelfContained => {
                dump_snapshot(&entry.snapshot).map_err(to_napi_error)?
            }
            SnapshotHandleFormat::Detached => {
                dump_detached_snapshot(&entry.snapshot).map_err(to_napi_error)?
            }
        };
        Ok(Buffer::from(bytes))
    })
}

#[napi]
pub fn create_cancellation_token() -> Result<String> {
    let mut tokens = cancellation_tokens()
        .lock()
        .map_err(|_| to_napi_error("cancellation token registry is poisoned"))?;
    let token_id = next_cancellation_token_id(&tokens);
    tokens.insert(token_id.clone(), Arc::new(AtomicBool::new(false)));
    Ok(token_id)
}

#[napi]
pub fn cancel_cancellation_token(token_id: String) -> Result<()> {
    let tokens = cancellation_tokens()
        .lock()
        .map_err(|_| to_napi_error("cancellation token registry is poisoned"))?;
    let token = tokens
        .get(&token_id)
        .ok_or_else(|| to_napi_error(format!("unknown cancellation token `{token_id}`")))?;
    token.store(true, Ordering::SeqCst);
    Ok(())
}

#[napi]
pub fn release_cancellation_token(token_id: String) -> Result<()> {
    let mut tokens = cancellation_tokens()
        .lock()
        .map_err(|_| to_napi_error("cancellation token registry is poisoned"))?;
    tokens.remove(&token_id);
    Ok(())
}

#[napi]
pub fn snapshot_identity(snapshot: Buffer) -> Result<String> {
    Ok(snapshot_identity_hex(snapshot.as_ref()))
}

#[napi]
pub fn start_program(
    program_handle: String,
    options_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let options: StartOptionsDto = parse_json(&options_json).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let step = bridge_start_shared_program_detached(program, options, cancellation_token)
        .map_err(to_napi_error)?;
    encode_json(&step).map_err(to_napi_error)
}

#[napi]
pub fn start_program_with_snapshot_handle(
    program_handle: String,
    options_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let options: StartOptionsDto = parse_json(&options_json).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let (step, metrics) =
        start_shared_bytecode_with_metrics(program, execution_options(options, cancellation_token))
            .map_err(to_napi_error)?;
    encode_step_with_snapshot_handle(step, metrics, SnapshotHandleFormat::Detached)
}

#[napi]
pub fn start_program_with_snapshot_handle_buffer(
    program_handle: String,
    options_buffer: Buffer,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let options = decode_start_options_bytes(options_buffer.as_ref()).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let (step, metrics) =
        start_shared_bytecode_with_metrics(program, execution_options(options, cancellation_token))
            .map_err(to_napi_error)?;
    encode_step_with_snapshot_handle(step, metrics, SnapshotHandleFormat::Detached)
}

#[napi]
pub fn start_program_with_execution_context_handle(
    program_handle: String,
    context_handle: String,
    inputs_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let context = lookup_execution_context(&context_handle)?;
    let inputs = parse_json(&inputs_json).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let (step, metrics) = start_shared_bytecode_with_metrics(
        program,
        execution_options_from_context(&context, inputs, cancellation_token),
    )
    .map_err(to_napi_error)?;
    encode_step_with_snapshot_handle(step, metrics, SnapshotHandleFormat::Detached)
}

#[napi]
pub fn start_program_with_execution_context_handle_buffer(
    program_handle: String,
    context_handle: String,
    inputs_buffer: Buffer,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let context = lookup_execution_context(&context_handle)?;
    let inputs = decode_structured_inputs_bytes(inputs_buffer.as_ref()).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let (step, metrics) = start_shared_bytecode_with_metrics(
        program,
        execution_options_from_context(&context, inputs, cancellation_token),
    )
    .map_err(to_napi_error)?;
    encode_step_with_snapshot_handle(step, metrics, SnapshotHandleFormat::Detached)
}

#[napi]
pub fn profile_start_program_with_snapshot_handle(
    program_handle: String,
    options_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let parse_started = Instant::now();
    let options: StartOptionsDto = parse_json(&options_json).map_err(to_napi_error)?;
    let parse_ns = elapsed_nanos(parse_started);
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let execute_started = Instant::now();
    let (step, metrics) =
        start_shared_bytecode_with_metrics(program, execution_options(options, cancellation_token))
            .map_err(to_napi_error)?;
    let execute_ns = elapsed_nanos(execute_started);
    encode_profiled_step_with_snapshot_handle(
        step,
        metrics,
        SnapshotHandleFormat::Detached,
        parse_ns,
        execute_ns,
    )
}

#[napi]
pub fn profile_start_program_with_snapshot_handle_buffer(
    program_handle: String,
    options_buffer: Buffer,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let parse_started = Instant::now();
    let options = decode_start_options_bytes(options_buffer.as_ref()).map_err(to_napi_error)?;
    let parse_ns = elapsed_nanos(parse_started);
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let execute_started = Instant::now();
    let (step, metrics) =
        start_shared_bytecode_with_metrics(program, execution_options(options, cancellation_token))
            .map_err(to_napi_error)?;
    let execute_ns = elapsed_nanos(execute_started);
    encode_profiled_step_with_snapshot_handle(
        step,
        metrics,
        SnapshotHandleFormat::Detached,
        parse_ns,
        execute_ns,
    )
}

#[napi]
pub fn profile_start_program_with_execution_context_handle(
    program_handle: String,
    context_handle: String,
    inputs_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let context = lookup_execution_context(&context_handle)?;
    let parse_started = Instant::now();
    let inputs = parse_json(&inputs_json).map_err(to_napi_error)?;
    let parse_ns = elapsed_nanos(parse_started);
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let execute_started = Instant::now();
    let (step, metrics) = start_shared_bytecode_with_metrics(
        program,
        execution_options_from_context(&context, inputs, cancellation_token),
    )
    .map_err(to_napi_error)?;
    let execute_ns = elapsed_nanos(execute_started);
    encode_profiled_step_with_snapshot_handle(
        step,
        metrics,
        SnapshotHandleFormat::Detached,
        parse_ns,
        execute_ns,
    )
}

#[napi]
pub fn profile_start_program_with_execution_context_handle_buffer(
    program_handle: String,
    context_handle: String,
    inputs_buffer: Buffer,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let context = lookup_execution_context(&context_handle)?;
    let parse_started = Instant::now();
    let inputs = decode_structured_inputs_bytes(inputs_buffer.as_ref()).map_err(to_napi_error)?;
    let parse_ns = elapsed_nanos(parse_started);
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let execute_started = Instant::now();
    let (step, metrics) = start_shared_bytecode_with_metrics(
        program,
        execution_options_from_context(&context, inputs, cancellation_token),
    )
    .map_err(to_napi_error)?;
    let execute_ns = elapsed_nanos(execute_started);
    encode_profiled_step_with_snapshot_handle(
        step,
        metrics,
        SnapshotHandleFormat::Detached,
        parse_ns,
        execute_ns,
    )
}

#[napi]
pub fn inspect_snapshot(snapshot: Buffer, policy_json: String) -> Result<String> {
    let policy = policy_from_json(policy_json)?;
    let snapshot_id = policy
        .snapshot_id
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_id"))?;
    let snapshot_key_base64 = policy
        .snapshot_key_base64
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_base64"))?;
    let snapshot_token = policy
        .snapshot_token
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_token"))?;
    let snapshot_key_digest = policy
        .snapshot_key_digest
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_digest"))?;
    assert_authenticated_snapshot(
        snapshot.as_ref(),
        SnapshotAuth {
            snapshot_id,
            snapshot_key_base64,
            snapshot_token,
            snapshot_key_digest,
        },
    )?;
    let inspection = inspect_snapshot_bytes(snapshot.as_ref(), policy).map_err(to_napi_error)?;
    encode_json(&inspection).map_err(to_napi_error)
}

#[napi]
pub fn inspect_detached_snapshot(
    program_handle: String,
    snapshot: Buffer,
    policy_json: String,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let policy = policy_from_json(policy_json)?;
    let snapshot_id = policy
        .snapshot_id
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_id"))?;
    let snapshot_key_base64 = policy
        .snapshot_key_base64
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_base64"))?;
    let snapshot_token = policy
        .snapshot_token
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_token"))?;
    let snapshot_key_digest = policy
        .snapshot_key_digest
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_digest"))?;
    assert_authenticated_snapshot(
        snapshot.as_ref(),
        SnapshotAuth {
            snapshot_id,
            snapshot_key_base64,
            snapshot_token,
            snapshot_key_digest,
        },
    )?;
    let inspection = inspect_detached_snapshot_bytes(snapshot.as_ref(), program, policy)
        .map_err(to_napi_error)?;
    encode_json(&inspection).map_err(to_napi_error)
}

#[napi]
pub fn load_snapshot_handle(snapshot: Buffer, policy_json: String) -> Result<String> {
    let policy = policy_from_json(policy_json)?;
    load_snapshot_handle_impl(
        snapshot.as_ref(),
        policy,
        SnapshotHandleFormat::SelfContained,
    )
}

#[napi]
pub fn load_detached_snapshot_handle(
    program_handle: String,
    snapshot: Buffer,
    policy_json: String,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let policy = policy_from_json(policy_json)?;
    load_detached_snapshot_handle_impl(snapshot.as_ref(), program, policy)
}

#[napi]
pub fn load_snapshot_handle_with_execution_context(
    context_handle: String,
    snapshot: Buffer,
    snapshot_id: String,
    snapshot_key_base64: String,
    snapshot_key_digest: String,
    snapshot_token: String,
) -> Result<String> {
    let context = lookup_execution_context(&context_handle)?;
    load_snapshot_handle_from_context(
        snapshot.as_ref(),
        &context,
        SnapshotAuth {
            snapshot_id: &snapshot_id,
            snapshot_key_base64: &snapshot_key_base64,
            snapshot_key_digest: &snapshot_key_digest,
            snapshot_token: &snapshot_token,
        },
        SnapshotHandleFormat::SelfContained,
    )
}

#[napi]
pub fn load_detached_snapshot_handle_with_execution_context(
    program_handle: String,
    context_handle: String,
    snapshot: Buffer,
    snapshot_id: String,
    snapshot_key_base64: String,
    snapshot_key_digest: String,
    snapshot_token: String,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let context = lookup_execution_context(&context_handle)?;
    load_detached_snapshot_handle_from_context(
        snapshot.as_ref(),
        program,
        &context,
        SnapshotAuth {
            snapshot_id: &snapshot_id,
            snapshot_key_base64: &snapshot_key_base64,
            snapshot_key_digest: &snapshot_key_digest,
            snapshot_token: &snapshot_token,
        },
    )
}

#[napi]
pub fn inspect_snapshot_handle(snapshot_handle: String) -> Result<String> {
    let inspection = with_snapshot(&snapshot_handle, |snapshot| {
        snapshot_inspection(snapshot).map_err(to_napi_error)
    })?;
    encode_json(&inspection).map_err(to_napi_error)
}

#[napi]
pub fn resume_program(
    snapshot: Buffer,
    payload_json: String,
    policy_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let payload: ResumeDto = parse_json(&payload_json).map_err(to_napi_error)?;
    let policy = policy_from_json(policy_json)?;
    let snapshot_id = policy
        .snapshot_id
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_id"))?;
    let snapshot_key_base64 = policy
        .snapshot_key_base64
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_base64"))?;
    let snapshot_token = policy
        .snapshot_token
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_token"))?;
    let snapshot_key_digest = policy
        .snapshot_key_digest
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_digest"))?;
    assert_authenticated_snapshot(
        snapshot.as_ref(),
        SnapshotAuth {
            snapshot_id,
            snapshot_key_base64,
            snapshot_token,
            snapshot_key_digest,
        },
    )?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let step = bridge_resume_program(snapshot.as_ref(), payload, policy, cancellation_token)
        .map_err(to_napi_error)?;
    encode_json(&step).map_err(to_napi_error)
}

#[napi]
pub fn resume_detached_program(
    program_handle: String,
    snapshot: Buffer,
    payload_json: String,
    policy_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = lookup_program(&program_handle)?;
    let payload: ResumeDto = parse_json(&payload_json).map_err(to_napi_error)?;
    let policy = policy_from_json(policy_json)?;
    let snapshot_id = policy
        .snapshot_id
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_id"))?;
    let snapshot_key_base64 = policy
        .snapshot_key_base64
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_base64"))?;
    let snapshot_token = policy
        .snapshot_token
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_token"))?;
    let snapshot_key_digest = policy
        .snapshot_key_digest
        .as_deref()
        .ok_or_else(|| to_napi_error("raw snapshot restore requires snapshot_key_digest"))?;
    assert_authenticated_snapshot(
        snapshot.as_ref(),
        SnapshotAuth {
            snapshot_id,
            snapshot_key_base64,
            snapshot_token,
            snapshot_key_digest,
        },
    )?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let step = bridge_resume_detached_program(
        snapshot.as_ref(),
        program,
        payload,
        policy,
        cancellation_token,
    )
    .map_err(to_napi_error)?;
    encode_json(&step).map_err(to_napi_error)
}

#[napi]
pub fn resume_snapshot_handle(
    snapshot_handle: String,
    payload_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let payload: ResumeDto = parse_json(&payload_json).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let entry = take_snapshot(&snapshot_handle)?;
    let (step, metrics) = resume_with_options_and_metrics(
        entry.snapshot,
        payload.into_resume_payload(),
        ResumeOptions {
            cancellation_token,
            snapshot_policy: None,
        },
    )
    .map_err(to_napi_error)?;
    encode_step_with_snapshot_handle(step, metrics, entry.format)
}

#[napi]
pub fn resume_snapshot_handle_buffer(
    snapshot_handle: String,
    payload_buffer: Buffer,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let payload = decode_resume_payload_bytes(payload_buffer.as_ref()).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let entry = take_snapshot(&snapshot_handle)?;
    let (step, metrics) = resume_with_options_and_metrics(
        entry.snapshot,
        payload.into_resume_payload(),
        ResumeOptions {
            cancellation_token,
            snapshot_policy: None,
        },
    )
    .map_err(to_napi_error)?;
    encode_step_with_snapshot_handle(step, metrics, entry.format)
}

#[napi]
pub fn profile_resume_snapshot_handle(
    snapshot_handle: String,
    payload_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let parse_started = Instant::now();
    let payload: ResumeDto = parse_json(&payload_json).map_err(to_napi_error)?;
    let parse_ns = elapsed_nanos(parse_started);
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let entry = take_snapshot(&snapshot_handle)?;
    let execute_started = Instant::now();
    let (step, metrics) = resume_with_options_and_metrics(
        entry.snapshot,
        payload.into_resume_payload(),
        ResumeOptions {
            cancellation_token,
            snapshot_policy: None,
        },
    )
    .map_err(to_napi_error)?;
    let execute_ns = elapsed_nanos(execute_started);
    encode_profiled_step_with_snapshot_handle(step, metrics, entry.format, parse_ns, execute_ns)
}

#[napi]
pub fn profile_resume_snapshot_handle_buffer(
    snapshot_handle: String,
    payload_buffer: Buffer,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let parse_started = Instant::now();
    let payload = decode_resume_payload_bytes(payload_buffer.as_ref()).map_err(to_napi_error)?;
    let parse_ns = elapsed_nanos(parse_started);
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let entry = take_snapshot(&snapshot_handle)?;
    let execute_started = Instant::now();
    let (step, metrics) = resume_with_options_and_metrics(
        entry.snapshot,
        payload.into_resume_payload(),
        ResumeOptions {
            cancellation_token,
            snapshot_policy: None,
        },
    )
    .map_err(to_napi_error)?;
    let execute_ns = elapsed_nanos(execute_started);
    encode_profiled_step_with_snapshot_handle(step, metrics, entry.format, parse_ns, execute_ns)
}
