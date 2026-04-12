use jslite_sidecar::handle_request_line;
use serde_json::{Value, json};

fn request(payload: Value) -> Value {
    let encoded = payload.to_string();
    let line = handle_request_line(&encoded)
        .unwrap_or_else(|error| panic!("request should succeed:\n{encoded}\n{error}"))
        .expect("request should yield a response line");
    serde_json::from_str(&line).expect("response should parse")
}

fn compile_program(source: &str, id: u64) -> String {
    let response = request(json!({
        "method": "compile",
        "id": id,
        "source": source,
    }));
    assert!(response["ok"].as_bool().unwrap_or(false));
    assert_eq!(response["id"], id);
    response["result"]["program_base64"]
        .as_str()
        .expect("program base64 should exist")
        .to_string()
}

fn start_program(program_base64: &str, capabilities: &[&str], id: u64) -> Value {
    request(json!({
        "method": "start",
        "id": id,
        "program_base64": program_base64,
        "options": {
            "inputs": {},
            "capabilities": capabilities,
            "limits": {},
        }
    }))
}

fn resume_snapshot(snapshot_base64: &str, capabilities: &[&str], payload: Value, id: u64) -> Value {
    request(json!({
        "method": "resume",
        "id": id,
        "snapshot_base64": snapshot_base64,
        "policy": {
            "capabilities": capabilities,
            "limits": {},
        },
        "payload": payload,
    }))
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
    let first_program = compile_program("const value = 1; value + 1;", 7);
    let second_program = compile_program("const value = 5; value + 1;", 7);

    let first = start_program(&first_program, &[], 7);
    let second = start_program(&second_program, &[], 7);

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
    let program = compile_program("const value = fetch_data(4); value + 2;", 1);

    let first = start_program(&program, &["fetch_data"], 2);
    let second = start_program(&program, &["fetch_data"], 3);

    let first_snapshot = first["result"]["step"]["snapshot_base64"]
        .as_str()
        .expect("first snapshot should exist");
    let second_snapshot = second["result"]["step"]["snapshot_base64"]
        .as_str()
        .expect("second snapshot should exist");

    let second_resumed = resume_snapshot(
        second_snapshot,
        &["fetch_data"],
        json!({
            "type": "value",
            "value": finite_number(9.0),
        }),
        4,
    );
    let first_resumed = resume_snapshot(
        first_snapshot,
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
    let program = compile_program("const value = fetch_data(5); value + 3;", 1);
    let start = start_program(&program, &["fetch_data"], 2);
    let snapshot = start["result"]["step"]["snapshot_base64"]
        .as_str()
        .expect("snapshot should exist");

    let first = resume_snapshot(
        snapshot,
        &["fetch_data"],
        json!({
            "type": "value",
            "value": finite_number(5.0),
        }),
        3,
    );
    let replay = resume_snapshot(
        snapshot,
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
fn resume_fails_closed_when_policy_does_not_reauthorize_the_suspended_capability() {
    let program = compile_program("const value = fetch_data(2); value + 1;", 1);
    let start = start_program(&program, &["fetch_data"], 2);
    let snapshot = start["result"]["step"]["snapshot_base64"]
        .as_str()
        .expect("snapshot should exist");

    let response = resume_snapshot(
        snapshot,
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
