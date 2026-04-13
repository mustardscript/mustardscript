use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use rand::random;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
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

type HmacSha256 = Hmac<Sha256>;

fn cancellation_tokens() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static TOKENS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn used_progress_snapshots() -> &'static Mutex<HashSet<String>> {
    static TOKENS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    TOKENS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn next_cancellation_token_id(tokens: &HashMap<String, Arc<AtomicBool>>) -> String {
    loop {
        let candidate = format!("cancel-{:032x}", random::<u128>());
        if !tokens.contains_key(&candidate) {
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

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
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

fn assert_authenticated_snapshot(snapshot: &[u8], policy: &SnapshotPolicyDto) -> Result<()> {
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
    let snapshot_key = STANDARD
        .decode(snapshot_key_base64)
        .map_err(|_| to_napi_error("snapshot_key_base64 must be valid base64"))?;
    let expected_snapshot_id = snapshot_identity_hex(snapshot);
    if expected_snapshot_id != snapshot_id {
        return Err(to_napi_error(
            "raw snapshot restore rejected a tampered or unauthenticated snapshot",
        ));
    }
    if snapshot_key_digest_hex(&snapshot_key) != snapshot_key_digest {
        return Err(to_napi_error(
            "raw snapshot restore rejected a mismatched snapshot key digest",
        ));
    }
    let expected = encode_snapshot_token(snapshot_id, &snapshot_key)?;
    if expected != snapshot_token {
        return Err(to_napi_error(
            "raw snapshot restore rejected a tampered or unauthenticated snapshot",
        ));
    }
    Ok(())
}

#[napi]
pub fn compile_program(source: String) -> Result<Buffer> {
    let bytes = compile_program_bytes(&source).map_err(to_napi_error)?;
    Ok(Buffer::from(bytes))
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
pub fn is_progress_snapshot_used(snapshot_identity: String) -> Result<bool> {
    let tokens = used_progress_snapshots()
        .lock()
        .map_err(|_| to_napi_error("progress snapshot registry is poisoned"))?;
    Ok(tokens.contains(&snapshot_identity))
}

#[napi]
pub fn claim_progress_snapshot(snapshot_identity: String) -> Result<bool> {
    let mut tokens = used_progress_snapshots()
        .lock()
        .map_err(|_| to_napi_error("progress snapshot registry is poisoned"))?;
    Ok(tokens.insert(snapshot_identity))
}

#[napi]
pub fn release_progress_snapshot(snapshot_identity: String) -> Result<()> {
    let mut tokens = used_progress_snapshots()
        .lock()
        .map_err(|_| to_napi_error("progress snapshot registry is poisoned"))?;
    tokens.remove(&snapshot_identity);
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
    assert_authenticated_snapshot(snapshot.as_ref(), &policy)?;
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
    assert_authenticated_snapshot(snapshot.as_ref(), &policy)?;
    let cancellation_token = lookup_cancellation_token(cancellation_token_id)?;
    let step = bridge_resume_program(snapshot.as_ref(), payload, policy, cancellation_token)
        .map_err(to_napi_error)?;
    encode_json(&step).map_err(to_napi_error)
}
