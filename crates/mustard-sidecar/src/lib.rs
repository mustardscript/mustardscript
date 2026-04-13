use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, anyhow};
use mustard::BytecodeProgram;
use mustard_bridge::{
    ResumeDto, SnapshotPolicyDto, StartOptionsDto, StepDto, compile_program_bytes, decode_base64,
    decode_program, encode_bytes_base64, resume_program, start_shared_program,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
        #[serde(default)]
        program_base64: Option<String>,
        #[serde(default)]
        program_id: Option<String>,
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
    Program {
        program_base64: String,
        program_id: String,
    },
    Step {
        step: StepDto,
    },
}

fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

#[derive(Debug)]
struct ProgramEntry {
    bytes: Vec<u8>,
    decoded: OnceLock<Arc<BytecodeProgram>>,
}

impl ProgramEntry {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            decoded: OnceLock::new(),
        }
    }

    fn resolve(&self) -> Result<Arc<BytecodeProgram>> {
        if let Some(program) = self.decoded.get() {
            return Ok(Arc::clone(program));
        }
        let program = Arc::new(decode_program(&self.bytes)?);
        match self.decoded.set(Arc::clone(&program)) {
            Ok(()) => Ok(program),
            Err(existing) => Ok(existing),
        }
    }
}

#[derive(Default)]
pub struct SidecarSession {
    programs: HashMap<String, ProgramEntry>,
}

impl SidecarSession {
    pub fn new() -> Self {
        Self::default()
    }

    fn resolve_program(
        &mut self,
        program_id: Option<String>,
        program_base64: Option<String>,
    ) -> Result<Arc<BytecodeProgram>> {
        if let Some(program_id) = program_id.as_ref()
            && let Some(entry) = self.programs.get(program_id)
        {
            return entry.resolve();
        }

        let Some(program_base64) = program_base64 else {
            if let Some(program_id) = program_id {
                return Err(anyhow!("unknown program_id `{program_id}`"));
            }
            return Err(anyhow!("start requires program_base64 or program_id"));
        };

        let bytes = decode_base64(&program_base64)?;
        let derived_program_id = digest_hex(&bytes);
        if let Some(program_id) = program_id
            && program_id != derived_program_id
        {
            return Err(anyhow!("program_id did not match program_base64"));
        }
        let entry = self
            .programs
            .entry(derived_program_id)
            .or_insert_with(|| ProgramEntry::new(bytes));
        entry.resolve()
    }

    fn handle(&mut self, request: Request) -> Response {
        let id = match &request {
            Request::Compile { id, .. }
            | Request::Start { id, .. }
            | Request::Resume { id, .. } => *id,
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
                let program_id = digest_hex(&bytes);
                self.programs
                    .entry(program_id.clone())
                    .or_insert_with(|| ProgramEntry::new(bytes.clone()));
                Ok(ResponsePayload::Program {
                    program_base64: encode_bytes_base64(&bytes),
                    program_id,
                })
            })(),
            Request::Start {
                program_base64,
                program_id,
                options,
                ..
            } => (|| {
                let program = self.resolve_program(program_id, program_base64)?;
                let step = start_shared_program(program, options, None)?;
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

    pub fn handle_request_line(&mut self, line: &str) -> Result<Option<String>> {
        if line.trim().is_empty() {
            return Ok(None);
        }
        let request: Request = serde_json::from_str(line).context("invalid request")?;
        let response = self.handle(request);
        let encoded = serde_json::to_string(&response).context("failed to encode response")?;
        Ok(Some(encoded))
    }
}

pub fn handle_request_line(line: &str) -> Result<Option<String>> {
    SidecarSession::new().handle_request_line(line)
}
