use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use jslite::structured::StructuredNumber;
use jslite::{
    ExecutionOptions, RuntimeLimits, StructuredValue, compile, dump_program, dump_snapshot,
    lower_to_bytecode, start,
};
use jslite_sidecar::handle_request_line;
use serde_json::Value;
use sha2::{Digest, Sha256};

const SAFE_MESSAGE_PATH_FRAGMENTS: &[&str] = &["/Users/", "\\Users\\", "C:\\", "/home/"];
const SNAPSHOT_KEY: &[u8] = b"sidecar-hostile-protocol-key";

type HmacSha256 = Hmac<Sha256>;

fn assert_host_safe_message(message: &str) {
    for fragment in SAFE_MESSAGE_PATH_FRAGMENTS {
        assert!(
            !message.contains(fragment),
            "message leaked host path fragment `{fragment}`: {message}"
        );
    }
}

fn encoded_program() -> String {
    let compiled = compile("const value = 1; value + 1;").expect("compile should work");
    let bytecode = lower_to_bytecode(&compiled).expect("lowering should work");
    STANDARD.encode(dump_program(&bytecode).expect("program should serialize"))
}

fn encoded_snapshot() -> String {
    let program = compile("const value = fetch_data(1); value + 2;").expect("compile should work");
    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("program should suspend");

    let snapshot = match step {
        jslite::ExecutionStep::Completed(_) => panic!("program should suspend"),
        jslite::ExecutionStep::Suspended(suspension) => suspension.snapshot,
    };
    STANDARD.encode(dump_snapshot(&snapshot).expect("snapshot should serialize"))
}

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

fn decode_response(line: &str) -> Value {
    serde_json::from_str(line).expect("response should be valid json")
}

fn mutated_lines(seed: &str) -> Vec<String> {
    let bytes = seed.as_bytes();
    let mut cases = vec![
        String::new(),
        "{".to_string(),
        seed[..seed.len().min(8)].to_string(),
    ];
    for index in 0..bytes.len().min(32) {
        let mut mutated = bytes.to_vec();
        mutated[index] ^= 0x25;
        cases.push(String::from_utf8_lossy(&mutated).into_owned());
    }
    cases
}

#[test]
fn blank_lines_are_ignored() {
    assert!(
        handle_request_line("")
            .expect("blank line should be accepted")
            .is_none()
    );
    assert!(
        handle_request_line("   ")
            .expect("whitespace line should be accepted")
            .is_none()
    );
}

#[test]
fn hostile_but_well_formed_requests_fail_closed() {
    let snapshot = encoded_snapshot();
    let requests = vec![
        serde_json::json!({
            "method": "start",
            "id": 1,
            "program_base64": "%%%%",
            "options": {
                "inputs": {},
                "capabilities": [],
            }
        })
        .to_string(),
        serde_json::json!({
            "method": "resume",
            "id": 2,
            "snapshot_base64": "%%%%",
            "policy": {
                "capabilities": ["fetch_data"],
                "limits": {},
            },
            "payload": {
                "type": "value",
                "value": { "Undefined": null }
            }
        })
        .to_string(),
        serde_json::json!({
            "method": "start",
            "id": 3,
            "program_base64": encoded_program(),
            "options": {
                "inputs": {},
                "capabilities": [],
                "limits": {
                    "instruction_budget": 0,
                    "heap_limit_bytes": 1,
                    "allocation_budget": 1,
                    "call_depth_limit": 0,
                    "max_outstanding_host_calls": 0,
                }
            }
        })
        .to_string(),
        serde_json::json!({
            "method": "resume",
            "id": 4,
            "snapshot_base64": snapshot,
            "policy": {
                "capabilities": ["fetch_data"],
                "limits": RuntimeLimits::default(),
            },
            "payload": {
                "type": "error",
                "error": {
                    "name": "CapabilityError",
                    "message": "host failure",
                    "code": "E_HOST",
                    "details": StructuredValue::Number(StructuredNumber::Finite(1.0)),
                }
            }
        })
        .to_string(),
    ];

    for request in requests {
        let line = handle_request_line(&request)
            .expect("well-formed hostile request should still produce a response")
            .expect("response line should exist");
        let response = decode_response(&line);
        assert!(response["id"].is_number());
        assert!(response["ok"].is_boolean());
        if let Some(error) = response["error"].as_str() {
            assert_host_safe_message(error);
        }
    }
}

#[test]
fn resume_rejects_tampered_or_unauthenticated_snapshots() {
    let snapshot = encoded_snapshot();

    let unauthenticated = handle_request_line(
        &serde_json::json!({
            "method": "resume",
            "id": 10,
            "snapshot_base64": snapshot,
            "policy": {
                "capabilities": ["fetch_data"],
                "limits": RuntimeLimits::default(),
            },
            "payload": {
                "type": "value",
                "value": { "Number": { "Finite": 1.0 } }
            }
        })
        .to_string(),
    )
    .expect("request should yield a response")
    .expect("response should exist");
    let unauthenticated = decode_response(&unauthenticated);
    assert!(!unauthenticated["ok"].as_bool().unwrap_or(true));
    assert!(
        unauthenticated["error"]
            .as_str()
            .expect("error should exist")
            .contains("raw snapshot restore requires snapshot_id")
    );

    let forged = handle_request_line(
        &serde_json::json!({
            "method": "resume",
            "id": 11,
            "snapshot_base64": snapshot,
            "policy": {
                "capabilities": ["fetch_data"],
                "limits": RuntimeLimits::default(),
                "snapshot_id": snapshot_id(&snapshot),
                "snapshot_key_base64": STANDARD.encode(SNAPSHOT_KEY),
                "snapshot_key_digest": snapshot_key_digest(),
                "snapshot_token": "forged-token",
            },
            "payload": {
                "type": "value",
                "value": { "Number": { "Finite": 1.0 } }
            }
        })
        .to_string(),
    )
    .expect("request should yield a response")
    .expect("response should exist");
    let forged = decode_response(&forged);
    assert!(!forged["ok"].as_bool().unwrap_or(true));
    assert!(
        forged["error"]
            .as_str()
            .expect("error should exist")
            .contains("tampered or unauthenticated snapshot")
    );
}

#[test]
fn mutated_request_lines_never_panic() {
    let snapshot = encoded_snapshot();
    let seeds = vec![
        serde_json::json!({
            "method": "compile",
            "id": 1,
            "source": "const value = 1; value + 1;",
        })
        .to_string(),
        serde_json::json!({
            "method": "start",
            "id": 2,
            "program_base64": encoded_program(),
            "options": {
                "inputs": {},
                "capabilities": [],
                "limits": RuntimeLimits::default(),
            }
        })
        .to_string(),
        serde_json::json!({
            "method": "resume",
            "id": 3,
            "snapshot_base64": snapshot,
            "policy": {
                "capabilities": ["fetch_data"],
                "limits": RuntimeLimits::default(),
                "snapshot_id": snapshot_id(&snapshot),
                "snapshot_key_base64": STANDARD.encode(SNAPSHOT_KEY),
                "snapshot_key_digest": snapshot_key_digest(),
                "snapshot_token": snapshot_token(&snapshot),
            },
            "payload": {
                "type": "value",
                "value": StructuredValue::Number(StructuredNumber::Finite(5.0)),
            }
        })
        .to_string(),
    ];

    for seed in seeds {
        for line in mutated_lines(&seed) {
            match handle_request_line(&line) {
                Ok(Some(response)) => {
                    let json = decode_response(&response);
                    assert!(json["id"].is_number());
                    assert!(json["ok"].is_boolean());
                    if let Some(error) = json["error"].as_str() {
                        assert_host_safe_message(error);
                    }
                }
                Ok(None) => {}
                Err(error) => assert_host_safe_message(&error.to_string()),
            }
        }
    }
}
