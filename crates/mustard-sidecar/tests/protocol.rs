use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const SNAPSHOT_KEY: &[u8] = b"sidecar-protocol-test-key";
const PROTOCOL_VERSION: u32 = mustard_sidecar::PROTOCOL_VERSION;

type HmacSha256 = Hmac<Sha256>;

fn snapshot_id(snapshot_base64: &str) -> String {
    let snapshot = STANDARD
        .decode(snapshot_base64)
        .expect("snapshot base64 should decode");
    let digest = Sha256::digest(snapshot);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn snapshot_key_digest() -> String {
    let digest = Sha256::digest(SNAPSHOT_KEY);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn snapshot_token(snapshot_base64: &str) -> String {
    let snapshot_id = snapshot_id(snapshot_base64);
    let mut mac = HmacSha256::new_from_slice(SNAPSHOT_KEY).expect("snapshot key should be valid");
    mac.update(snapshot_id.as_bytes());
    let digest = mac.finalize().into_bytes();
    let mut token = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn authenticated_policy(snapshot_base64: &str, capabilities: &[&str]) -> Value {
    json!({
        "capabilities": capabilities,
        "limits": {},
        "snapshot_id": snapshot_id(snapshot_base64),
        "snapshot_key_base64": STANDARD.encode(SNAPSHOT_KEY),
        "snapshot_key_digest": snapshot_key_digest(),
        "snapshot_token": snapshot_token(snapshot_base64),
    })
}

#[test]
fn sidecar_compiles_starts_and_resumes() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let stdout = child.stdout.take().expect("stdout should be available");
    let mut reader = BufReader::new(stdout);

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 1,
            "source": "const value = fetch_data(5); value + 1;",
        })
    )
    .expect("compile request should write");

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("compile response should read");
    let compile_response: Value =
        serde_json::from_str(&line).expect("compile response should parse");
    assert!(compile_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(compile_response["protocol_version"], PROTOCOL_VERSION);
    let program_id = compile_response["result"]["program_id"]
        .as_str()
        .expect("program id should exist")
        .to_string();

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "start",
            "id": 2,
            "program_id": program_id,
            "options": {
                "inputs": {},
                "capabilities": ["fetch_data"],
            }
        })
    )
    .expect("start request should write");

    line.clear();
    reader
        .read_line(&mut line)
        .expect("start response should read");
    let start_response: Value = serde_json::from_str(&line).expect("start response should parse");
    assert!(start_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(start_response["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(start_response["result"]["step"]["type"], "suspended");
    assert_eq!(start_response["result"]["step"]["capability"], "fetch_data");
    let snapshot = start_response["result"]["step"]["snapshot_base64"]
        .as_str()
        .expect("snapshot base64 should exist")
        .to_string();
    let policy = authenticated_policy(&snapshot, &["fetch_data"]);

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "resume",
            "id": 3,
            "snapshot_base64": snapshot,
            "policy": policy,
            "payload": {
                "type": "value",
                "value": { "Number": { "Finite": 5.0 } }
            }
        })
    )
    .expect("resume request should write");

    line.clear();
    reader
        .read_line(&mut line)
        .expect("resume response should read");
    let resume_response: Value = serde_json::from_str(&line).expect("resume response should parse");
    assert!(resume_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(resume_response["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(resume_response["result"]["step"]["type"], "completed");
    assert_eq!(
        resume_response["result"]["step"]["value"]["Number"]["Finite"],
        6.0
    );

    drop(stdin);
    let status = child.wait().expect("sidecar should exit cleanly");
    assert!(status.success());
}

#[test]
fn sidecar_accepts_cancelled_resume_payload() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let stdout = child.stdout.take().expect("stdout should be available");
    let mut reader = BufReader::new(stdout);

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 1,
            "source": "const value = fetch_data(5); value + 1;",
        })
    )
    .expect("compile request should write");

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("compile response should read");
    let compile_response: Value =
        serde_json::from_str(&line).expect("compile response should parse");
    assert!(compile_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(compile_response["protocol_version"], PROTOCOL_VERSION);
    let program_id = compile_response["result"]["program_id"]
        .as_str()
        .expect("program id should exist")
        .to_string();

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "start",
            "id": 2,
            "program_id": program_id,
            "options": {
                "inputs": {},
                "capabilities": ["fetch_data"],
            }
        })
    )
    .expect("start request should write");

    line.clear();
    reader
        .read_line(&mut line)
        .expect("start response should read");
    let start_response: Value = serde_json::from_str(&line).expect("start response should parse");
    assert!(start_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(start_response["protocol_version"], PROTOCOL_VERSION);
    let snapshot = start_response["result"]["step"]["snapshot_base64"]
        .as_str()
        .expect("snapshot base64 should exist")
        .to_string();
    let policy = authenticated_policy(&snapshot, &["fetch_data"]);

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "resume",
            "id": 3,
            "snapshot_base64": snapshot,
            "policy": policy,
            "payload": {
                "type": "cancelled"
            }
        })
    )
    .expect("resume request should write");

    line.clear();
    reader
        .read_line(&mut line)
        .expect("resume response should read");
    let resume_response: Value = serde_json::from_str(&line).expect("resume response should parse");
    assert!(!resume_response["ok"].as_bool().unwrap_or(true));
    assert_eq!(resume_response["protocol_version"], PROTOCOL_VERSION);
    assert!(
        resume_response["error"]
            .as_str()
            .expect("error should exist")
            .contains("execution cancelled")
    );

    drop(stdin);
    let status = child.wait().expect("sidecar should exit cleanly");
    assert!(status.success());
}

#[test]
fn sidecar_reports_invalid_requests_with_a_nonzero_exit() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    writeln!(stdin, "{{").expect("invalid request should write");
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("sidecar should terminate after invalid input");
    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("invalid request"));
}

#[test]
fn sidecar_can_be_forcefully_terminated_and_restarted() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let stdout = child.stdout.take().expect("stdout should be available");
    let mut reader = BufReader::new(stdout);

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 1,
            "source": "while (true) {}",
        })
    )
    .expect("compile request should write");

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("compile response should read");
    let compile_response: Value =
        serde_json::from_str(&line).expect("compile response should parse");
    assert!(compile_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(compile_response["protocol_version"], PROTOCOL_VERSION);
    let program_id = compile_response["result"]["program_id"]
        .as_str()
        .expect("program id should exist")
        .to_string();

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "start",
            "id": 2,
            "program_id": program_id,
            "options": {
                "inputs": {},
                "capabilities": [],
                "limits": {
                    "instruction_budget": 1_000_000_000usize,
                }
            }
        })
    )
    .expect("start request should write");

    thread::sleep(Duration::from_millis(50));
    assert!(
        child.try_wait().expect("try_wait should succeed").is_none(),
        "runaway sidecar should still be executing before kill"
    );

    child.kill().expect("kill should succeed");
    let status = child.wait().expect("killed sidecar should exit");
    assert!(!status.success());

    let mut fresh = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("fresh sidecar should spawn");

    let mut fresh_stdin = fresh.stdin.take().expect("stdin should be available");
    let fresh_stdout = fresh.stdout.take().expect("stdout should be available");
    let mut fresh_reader = BufReader::new(fresh_stdout);

    writeln!(
        fresh_stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 3,
            "source": "const value = 2; value + 1;",
        })
    )
    .expect("compile request should write");

    line.clear();
    fresh_reader
        .read_line(&mut line)
        .expect("compile response should read");
    let compile_response: Value =
        serde_json::from_str(&line).expect("compile response should parse");
    assert!(compile_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(compile_response["protocol_version"], PROTOCOL_VERSION);
    let program = compile_response["result"]["program_base64"]
        .as_str()
        .expect("program base64 should exist")
        .to_string();

    writeln!(
        fresh_stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "start",
            "id": 4,
            "program_base64": program,
            "options": {
                "inputs": {},
                "capabilities": [],
            }
        })
    )
    .expect("start request should write");

    line.clear();
    fresh_reader
        .read_line(&mut line)
        .expect("start response should read");
    let start_response: Value = serde_json::from_str(&line).expect("start response should parse");
    assert!(start_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(start_response["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(start_response["result"]["step"]["type"], "completed");
    assert_eq!(
        start_response["result"]["step"]["value"]["Number"]["Finite"],
        3.0
    );

    drop(fresh_stdin);
    let status = fresh.wait().expect("fresh sidecar should exit cleanly");
    assert!(status.success());
}

#[test]
fn sidecar_rejects_unsupported_protocol_versions() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let stdout = child.stdout.take().expect("stdout should be available");
    let mut reader = BufReader::new(stdout);

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "protocol_version": PROTOCOL_VERSION + 1,
            "method": "compile",
            "id": 1,
            "source": "1;",
        })
    )
    .expect("request should write");

    let mut line = String::new();
    reader.read_line(&mut line).expect("response should read");
    let response: Value = serde_json::from_str(&line).expect("response should parse");
    assert!(!response["ok"].as_bool().unwrap_or(true));
    assert_eq!(response["protocol_version"], PROTOCOL_VERSION);
    assert!(
        response["error"]
            .as_str()
            .expect("error should exist")
            .contains("unsupported sidecar protocol version"),
    );

    drop(stdin);
    let status = child.wait().expect("sidecar should exit cleanly");
    assert!(status.success());
}

#[test]
fn oversized_request_lines_fail_closed_before_protocol_parsing() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let oversized = "x".repeat(mustard_sidecar::MAX_REQUEST_LINE_BYTES + 2);
    writeln!(stdin, "{oversized}").expect("oversized request should write");
    drop(stdin);

    let output = child.wait_with_output().expect("sidecar should exit");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("request line exceeds maximum size"));
}
