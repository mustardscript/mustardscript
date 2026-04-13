mod support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mustard_sidecar::SidecarSession;
use serde_json::{Value, json};

const SNAPSHOT_KEY: &[u8] = b"sidecar-protocol-state-key";
const PROTOCOL_VERSION: u32 = mustard_sidecar::PROTOCOL_VERSION;

fn request(session: &mut SidecarSession, payload: Value, blob: &[u8]) -> (Value, Vec<u8>) {
    support::request_binary(session, payload, blob)
}

fn compile_program(session: &mut SidecarSession, source: &str, id: u64) -> (Vec<u8>, String) {
    let (response, blob) = request(
        session,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "compile",
            "id": id,
            "source": source,
        }),
        &[],
    );
    assert!(response["ok"].as_bool().unwrap_or(false));
    assert_eq!(response["id"], id);
    (
        blob,
        response["result"]["program_id"]
            .as_str()
            .expect("program id should exist")
            .to_string(),
    )
}

fn snapshot_key_digest() -> String {
    support::snapshot_key_digest(SNAPSHOT_KEY)
}

fn start_program(
    session: &mut SidecarSession,
    program: Option<&[u8]>,
    program_id: Option<&str>,
    capabilities: &[&str],
    id: u64,
) -> (Value, Vec<u8>) {
    let mut payload = json!({
        "protocol_version": PROTOCOL_VERSION,
        "method": "start",
        "id": id,
        "options": {
            "inputs": {},
            "capabilities": capabilities,
            "limits": {},
        }
    });
    if let Some(program_id) = program_id {
        payload["program_id"] = Value::String(program_id.to_string());
    }
    request(session, payload, program.unwrap_or_default())
}

fn resume_snapshot(
    session: &mut SidecarSession,
    snapshot: &[u8],
    capabilities: &[&str],
    payload: Value,
    id: u64,
) -> Value {
    request(
        session,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "resume",
            "id": id,
            "policy": {
                "capabilities": capabilities,
                "limits": {},
                "snapshot_id": support::digest_hex(snapshot),
                "snapshot_key_base64": STANDARD.encode(SNAPSHOT_KEY),
                "snapshot_key_digest": snapshot_key_digest(),
                "snapshot_token": support::snapshot_token(snapshot, SNAPSHOT_KEY),
            },
            "payload": payload,
        }),
        snapshot,
    )
    .0
}

fn resume_snapshot_cached(
    session: &mut SidecarSession,
    snapshot_id: &str,
    snapshot: &[u8],
    policy_id: &str,
    payload: Value,
    id: u64,
) -> Value {
    request(
        session,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "method": "resume",
            "id": id,
            "snapshot_id": snapshot_id,
            "policy_id": policy_id,
            "auth": {
                "snapshot_key_base64": STANDARD.encode(SNAPSHOT_KEY),
                "snapshot_key_digest": snapshot_key_digest(),
                "snapshot_token": support::snapshot_token(snapshot, SNAPSHOT_KEY),
            },
            "payload": payload,
        }),
        &[],
    )
    .0
}

fn finite_number(value: f64) -> Value {
    json!({
        "Number": {
            "Finite": value,
        }
    })
}

#[test]
fn duplicate_ids_are_echoed_without_coupling_unrelated_requests() {
    let mut session = SidecarSession::new();
    let (first_program, _) = compile_program(&mut session, "const value = 1; value + 1;", 7);
    let (second_program, _) = compile_program(&mut session, "const value = 5; value + 1;", 7);

    let (first, _) = start_program(&mut session, Some(&first_program), None, &[], 7);
    let (second, _) = start_program(&mut session, Some(&second_program), None, &[], 7);

    assert!(first["ok"].as_bool().unwrap_or(false));
    assert!(second["ok"].as_bool().unwrap_or(false));
    assert_eq!(first["id"], 7);
    assert_eq!(second["id"], 7);
    assert_eq!(first["result"]["step"]["type"], "completed");
    assert_eq!(second["result"]["step"]["type"], "completed");
    assert_eq!(first["result"]["step"]["value"]["Number"]["Finite"], 2.0);
    assert_eq!(second["result"]["step"]["value"]["Number"]["Finite"], 6.0);
}

#[test]
fn same_program_blob_can_start_multiple_independent_suspended_executions() {
    let mut session = SidecarSession::new();
    let (program, _) = compile_program(&mut session, "const value = fetch_data(4); value + 2;", 1);

    let (first, first_snapshot) =
        start_program(&mut session, Some(&program), None, &["fetch_data"], 2);
    let (second, second_snapshot) =
        start_program(&mut session, Some(&program), None, &["fetch_data"], 3);

    assert_eq!(first["result"]["step"]["type"], "suspended");
    assert_eq!(second["result"]["step"]["type"], "suspended");

    let second_resumed = resume_snapshot(
        &mut session,
        &second_snapshot,
        &["fetch_data"],
        json!({
            "type": "value",
            "value": finite_number(9.0),
        }),
        4,
    );
    let first_resumed = resume_snapshot(
        &mut session,
        &first_snapshot,
        &["fetch_data"],
        json!({
            "type": "value",
            "value": finite_number(4.0),
        }),
        5,
    );

    assert!(second_resumed["ok"].as_bool().unwrap_or(false));
    assert!(first_resumed["ok"].as_bool().unwrap_or(false));
    assert_eq!(
        second_resumed["result"]["step"]["value"]["Number"]["Finite"],
        11.0
    );
    assert_eq!(
        first_resumed["result"]["step"]["value"]["Number"]["Finite"],
        6.0
    );
}

#[test]
fn replaying_the_same_snapshot_is_deterministic_because_resume_is_stateless() {
    let mut session = SidecarSession::new();
    let (program, _) = compile_program(&mut session, "const value = fetch_data(5); value + 3;", 1);
    let (_start, snapshot) = start_program(&mut session, Some(&program), None, &["fetch_data"], 2);

    let first = resume_snapshot(
        &mut session,
        &snapshot,
        &["fetch_data"],
        json!({
            "type": "value",
            "value": finite_number(5.0),
        }),
        3,
    );
    let replay = resume_snapshot(
        &mut session,
        &snapshot,
        &["fetch_data"],
        json!({
            "type": "value",
            "value": finite_number(5.0),
        }),
        4,
    );

    assert!(first["ok"].as_bool().unwrap_or(false));
    assert!(replay["ok"].as_bool().unwrap_or(false));
    assert_eq!(first["result"]["step"], replay["result"]["step"]);
}

#[test]
fn cached_snapshot_and_policy_ids_resume_without_resending_full_state() {
    let mut session = SidecarSession::new();
    let (program, _) = compile_program(&mut session, "const value = fetch_data(8); value + 2;", 1);
    let (start, snapshot) = start_program(&mut session, Some(&program), None, &["fetch_data"], 2);
    let snapshot_id = start["result"]["snapshot_id"]
        .as_str()
        .expect("snapshot_id should exist");
    let policy_id = start["result"]["policy_id"]
        .as_str()
        .expect("policy_id should exist");

    let resumed = resume_snapshot_cached(
        &mut session,
        snapshot_id,
        &snapshot,
        policy_id,
        json!({
            "type": "value",
            "value": finite_number(8.0),
        }),
        3,
    );

    assert!(resumed["ok"].as_bool().unwrap_or(false));
    assert_eq!(resumed["result"]["step"]["type"], "completed");
    assert_eq!(resumed["result"]["step"]["value"]["Number"]["Finite"], 10.0);
}

#[test]
fn cached_resume_fails_closed_for_unknown_snapshot_or_policy_id() {
    let mut session = SidecarSession::new();
    let response = resume_snapshot_cached(
        &mut session,
        "missing-snapshot",
        b"AAAA",
        "missing-policy",
        json!({
            "type": "value",
            "value": finite_number(1.0),
        }),
        1,
    );

    assert!(!response["ok"].as_bool().unwrap_or(true));
    let error = response["error"].as_str().expect("error should exist");
    assert!(
        error.contains("unknown snapshot_id") || error.contains("unknown policy_id"),
        "unexpected error: {error}"
    );
}

#[test]
fn resume_fails_closed_when_policy_does_not_reauthorize_the_suspended_capability() {
    let mut session = SidecarSession::new();
    let (program, _) = compile_program(&mut session, "const value = fetch_data(2); value + 1;", 1);
    let (_start, snapshot) = start_program(&mut session, Some(&program), None, &["fetch_data"], 2);

    let response = resume_snapshot(
        &mut session,
        &snapshot,
        &["other_capability"],
        json!({
            "type": "value",
            "value": finite_number(2.0),
        }),
        3,
    );

    assert!(!response["ok"].as_bool().unwrap_or(true));
    assert_eq!(response["id"], 3);
    assert!(
        response["error"]
            .as_str()
            .expect("error should exist")
            .contains("snapshot policy rejected unauthorized capability `fetch_data`")
    );
}

#[test]
fn compiled_program_ids_start_without_resending_program_bytes() {
    let mut session = SidecarSession::new();
    let (_, program_id) = compile_program(&mut session, "const value = 40; value + 2;", 1);

    let (first, _) = start_program(&mut session, None, Some(&program_id), &[], 2);
    let (second, _) = start_program(&mut session, None, Some(&program_id), &[], 3);

    assert!(first["ok"].as_bool().unwrap_or(false));
    assert!(second["ok"].as_bool().unwrap_or(false));
    assert_eq!(first["result"]["step"]["value"]["Number"]["Finite"], 42.0);
    assert_eq!(second["result"]["step"]["value"]["Number"]["Finite"], 42.0);
}
