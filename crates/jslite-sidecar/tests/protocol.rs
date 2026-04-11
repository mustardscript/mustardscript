use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::Value;

#[test]
fn sidecar_compiles_starts_and_resumes() {
    let exe = env!("CARGO_BIN_EXE_jslite-sidecar");
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
    let program = compile_response["result"]["program_base64"]
        .as_str()
        .expect("program base64 should exist")
        .to_string();

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "method": "start",
            "id": 2,
            "program_base64": program,
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
    assert_eq!(start_response["result"]["step"]["type"], "suspended");
    assert_eq!(start_response["result"]["step"]["capability"], "fetch_data");
    let snapshot = start_response["result"]["step"]["snapshot_base64"]
        .as_str()
        .expect("snapshot base64 should exist")
        .to_string();

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "method": "resume",
            "id": 3,
            "snapshot_base64": snapshot,
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
    assert_eq!(resume_response["result"]["step"]["type"], "completed");
    assert_eq!(
        resume_response["result"]["step"]["value"]["Number"]["Finite"],
        6.0
    );

    drop(stdin);
    let status = child.wait().expect("sidecar should exit cleanly");
    assert!(status.success());
}
