use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use jslite::CancellationToken;
use jslite_bridge::{
    ResumeDto, SnapshotPolicyDto, StartOptionsDto, compile_program_bytes, decode_program,
    encode_json, inspect_snapshot_bytes, parse_json, resume_program as bridge_resume_program,
    start_program as bridge_start_program,
};
use napi::bindgen_prelude::Buffer;
use napi::{Error, Result};
use napi_derive::napi;

fn cancellation_tokens() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static TOKENS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_cancellation_token_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    format!("cancel-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
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

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}

#[napi]
pub fn compile_program(source: String) -> Result<Buffer> {
    let bytes = compile_program_bytes(&source).map_err(to_napi_error)?;
    Ok(Buffer::from(bytes))
}

#[napi]
pub fn create_cancellation_token() -> Result<String> {
    let token_id = next_cancellation_token_id();
    let mut tokens = cancellation_tokens()
        .lock()
        .map_err(|_| to_napi_error("cancellation token registry is poisoned"))?;
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
pub fn start_program(
    program: Buffer,
    options_json: String,
    cancellation_token_id: Option<String>,
) -> Result<String> {
    let program = decode_program(program.as_ref()).map_err(to_napi_error)?;
    let options: StartOptionsDto = parse_json(&options_json).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let step =
        bridge_start_program(&program, options, cancellation_token).map_err(to_napi_error)?;
    encode_json(&step).map_err(to_napi_error)
}

#[napi]
pub fn inspect_snapshot(snapshot: Buffer, policy_json: String) -> Result<String> {
    let policy: SnapshotPolicyDto = parse_json(&policy_json).map_err(to_napi_error)?;
    let inspection = inspect_snapshot_bytes(snapshot.as_ref(), policy).map_err(to_napi_error)?;
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
    let policy: SnapshotPolicyDto = parse_json(&policy_json).map_err(to_napi_error)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let step = bridge_resume_program(snapshot.as_ref(), payload, policy, cancellation_token)
        .map_err(to_napi_error)?;
    encode_json(&step).map_err(to_napi_error)
}
