use anyhow::{Context, Result};
use jslite_bridge::{
    ResumeDto, SnapshotPolicyDto, StartOptionsDto, StepDto, compile_program_bytes, decode_base64,
    decode_program_base64, encode_bytes_base64, resume_program, start_program,
};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
enum Request {
    Compile {
        protocol_version: u32,
        id: u64,
        source: String,
    },
    Start {
        protocol_version: u32,
        id: u64,
        program_base64: String,
        options: StartOptionsDto,
    },
    Resume {
        protocol_version: u32,
        id: u64,
        snapshot_base64: String,
        policy: Box<SnapshotPolicyDto>,
        payload: Box<ResumeDto>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    protocol_version: u32,
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ResponsePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ResponsePayload {
    Program { program_base64: String },
    Step { step: StepDto },
}

fn handle(request: Request) -> Response {
    let id = match &request {
        Request::Compile { id, .. } | Request::Start { id, .. } | Request::Resume { id, .. } => *id,
    };
    let protocol_version = match &request {
        Request::Compile {
            protocol_version, ..
        }
        | Request::Start {
            protocol_version, ..
        }
        | Request::Resume {
            protocol_version, ..
        } => *protocol_version,
    };

    if protocol_version != PROTOCOL_VERSION {
        return Response {
            protocol_version: PROTOCOL_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(format!(
                "unsupported sidecar protocol version {protocol_version}; expected {PROTOCOL_VERSION}"
            )),
        };
    }

    let result: Result<ResponsePayload> = match request {
        Request::Compile { source, .. } => (|| {
            let bytes = compile_program_bytes(&source)?;
            Ok(ResponsePayload::Program {
                program_base64: encode_bytes_base64(&bytes),
            })
        })(),
        Request::Start {
            program_base64,
            options,
            ..
        } => (|| {
            let program = decode_program_base64(&program_base64)?;
            let step = start_program(&program, options, None)?;
            Ok(ResponsePayload::Step { step })
        })(),
        Request::Resume {
            snapshot_base64,
            policy,
            payload,
            ..
        } => (|| {
            let snapshot_bytes = decode_base64(&snapshot_base64)?;
            let step = resume_program(&snapshot_bytes, *payload, *policy, None)?;
            Ok(ResponsePayload::Step { step })
        })(),
    };

    match result {
        Ok(result) => Response {
            protocol_version: PROTOCOL_VERSION,
            id,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => Response {
            protocol_version: PROTOCOL_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(error.to_string()),
        },
    }
}

pub fn handle_request_line(line: &str) -> Result<Option<String>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    let request: Request = serde_json::from_str(line).context("invalid request")?;
    let response = handle(request);
    let encoded = serde_json::to_string(&response).context("failed to encode response")?;
    Ok(Some(encoded))
}
