mod support;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

const SNAPSHOT_KEY: &[u8] = b"sidecar-protocol-test-key";
const PROTOCOL_VERSION: u32 = mustard_sidecar::PROTOCOL_VERSION;

fn request_binary(
    stdin: &mut impl Write,
    reader: &mut impl std::io::Read,
    header: Value,
    blob: &[u8],
) -> (Value, Vec<u8>) {
    support::write_binary_frame(stdin, header, blob);
    support::read_binary_frame(reader)
}

fn request_jsonl(stdin: &mut impl Write, reader: &mut impl BufRead, payload: Value) -> Value {
    writeln!(stdin, "{payload}").expect("request should write");
    let mut line = String::new();
    reader.read_line(&mut line).expect("response should read");
    serde_json::from_str(&line).expect("response should parse")
}

#[test]
fn sidecar_compiles_starts_and_resumes_over_binary_frames() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let stdout = child.stdout.take().expect("stdout should be available");
    let mut reader = BufReader::new(stdout);

    let (compile_response, _program_bytes) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 1,
            "source": "const value = fetch_data(5); value + 1;",
        }),
        &[],
    );
    assert!(compile_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(compile_response["protocol_version"], PROTOCOL_VERSION);
    let program_id = compile_response["result"]["program_id"]
        .as_str()
        .expect("program id should exist")
        .to_string();

    let (start_response, snapshot) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "start",
            "id": 2,
            "program_id": program_id,
            "options": {
                "inputs": {},
                "capabilities": ["fetch_data"],
            }
        }),
        &[],
    );
    assert!(start_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(start_response["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(start_response["result"]["step"]["type"], "suspended");
    assert_eq!(start_response["result"]["step"]["capability"], "fetch_data");
    let snapshot_id = start_response["result"]["snapshot_id"]
        .as_str()
        .expect("snapshot_id should exist")
        .to_string();
    let policy_id = start_response["result"]["policy_id"]
        .as_str()
        .expect("policy_id should exist")
        .to_string();

    let (resume_response, resume_blob) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "resume",
            "id": 3,
            "snapshot_id": snapshot_id,
            "policy_id": policy_id,
            "auth": {
                "snapshot_key_base64": STANDARD.encode(SNAPSHOT_KEY),
                "snapshot_key_digest": support::snapshot_key_digest(SNAPSHOT_KEY),
                "snapshot_token": support::snapshot_token(&snapshot, SNAPSHOT_KEY),
            },
            "payload": {
                "type": "value",
                "value": { "Number": { "Finite": 5.0 } }
            }
        }),
        &[],
    );
    assert!(resume_blob.is_empty());
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
fn sidecar_accepts_cancelled_resume_payload_over_binary_frames() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let stdout = child.stdout.take().expect("stdout should be available");
    let mut reader = BufReader::new(stdout);

    let (compile_response, _) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 1,
            "source": "const value = fetch_data(5); value + 1;",
        }),
        &[],
    );
    let program_id = compile_response["result"]["program_id"]
        .as_str()
        .expect("program id should exist")
        .to_string();
    let (start_response, snapshot) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "start",
            "id": 2,
            "program_id": program_id,
            "options": {
                "inputs": {},
                "capabilities": ["fetch_data"],
            }
        }),
        &[],
    );
    let snapshot_id = start_response["result"]["snapshot_id"]
        .as_str()
        .expect("snapshot_id should exist")
        .to_string();
    let policy_id = start_response["result"]["policy_id"]
        .as_str()
        .expect("policy_id should exist")
        .to_string();

    let (resume_response, _) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "resume",
            "id": 3,
            "snapshot_id": snapshot_id,
            "policy_id": policy_id,
            "auth": {
                "snapshot_key_base64": STANDARD.encode(SNAPSHOT_KEY),
                "snapshot_key_digest": support::snapshot_key_digest(SNAPSHOT_KEY),
                "snapshot_token": support::snapshot_token(&snapshot, SNAPSHOT_KEY),
            },
            "payload": {
                "type": "cancelled"
            }
        }),
        &[],
    );
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
fn sidecar_reports_invalid_binary_requests_with_a_nonzero_exit() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    stdin
        .write_all(&(1u32).to_le_bytes())
        .expect("header length should write");
    stdin
        .write_all(&(0u32).to_le_bytes())
        .expect("payload length should write");
    stdin.write_all(b"{").expect("invalid header should write");
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("sidecar should terminate after invalid input");
    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("invalid request header"));
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

    let (compile_response, _) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 1,
            "source": "while (true) {}",
        }),
        &[],
    );
    let program_id = compile_response["result"]["program_id"]
        .as_str()
        .expect("program id should exist")
        .to_string();

    support::write_binary_frame(
        &mut stdin,
        json!({
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
        }),
        &[],
    );

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

    let (compile_response, program) = request_binary(
        &mut fresh_stdin,
        &mut fresh_reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 3,
            "source": "const value = 2; value + 1;",
        }),
        &[],
    );
    assert!(compile_response["ok"].as_bool().unwrap_or(false));
    assert_eq!(compile_response["protocol_version"], PROTOCOL_VERSION);

    let (start_response, start_blob) = request_binary(
        &mut fresh_stdin,
        &mut fresh_reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "start",
            "id": 4,
            "options": {
                "inputs": {},
                "capabilities": [],
            }
        }),
        &program,
    );
    assert!(start_blob.is_empty());
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

    let (response, response_blob) = request_binary(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION + 1,
            "method": "compile",
            "id": 1,
            "source": "1;",
        }),
        &[],
    );
    assert!(response_blob.is_empty());
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
fn oversized_request_frames_fail_closed_before_protocol_parsing() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    stdin
        .write_all(&(0u32).to_le_bytes())
        .expect("header length should write");
    stdin
        .write_all(&((mustard_sidecar::MAX_REQUEST_FRAME_BYTES as u32) + 1).to_le_bytes())
        .expect("payload length should write");
    drop(stdin);

    let output = child.wait_with_output().expect("sidecar should exit");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("request frame exceeds maximum size"));
}

#[test]
fn sidecar_jsonl_debug_mode_remains_available() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .arg("--jsonl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sidecar should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be available");
    let stdout = child.stdout.take().expect("stdout should be available");
    let mut reader = BufReader::new(stdout);

    let compile = request_jsonl(
        &mut stdin,
        &mut reader,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": 1,
            "source": "const value = 2; value + 1;",
        }),
    );
    assert!(compile["ok"].as_bool().unwrap_or(false));
    assert_eq!(compile["protocol_version"], PROTOCOL_VERSION);
    assert!(compile["result"]["program_base64"].is_string());

    drop(stdin);
    let status = child.wait().expect("sidecar should exit cleanly");
    assert!(status.success());
}

#[test]
fn oversized_jsonl_request_lines_fail_closed_in_debug_mode() {
    let exe = env!("CARGO_BIN_EXE_mustard-sidecar");
    let mut child = Command::new(exe)
        .arg("--jsonl")
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
