use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, anyhow};
use mustard::BytecodeProgram;
use mustard_bridge::{
    ResumeDto, RuntimeLimitsDto, SnapshotPolicyDto, StartOptionsDto, StepDto,
    compile_program_bytes, decode_base64, decode_program, encode_bytes_base64, resume_program,
    start_shared_program,
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
        #[serde(default)]
        snapshot_base64: Option<String>,
        #[serde(default)]
        snapshot_id: Option<String>,
        #[serde(default)]
        policy: Option<Box<SnapshotPolicyDto>>,
        #[serde(default)]
        policy_id: Option<String>,
        #[serde(default)]
        auth: Option<Box<ResumeAuth>>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        snapshot_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        policy_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResumeAuth {
    snapshot_key_base64: String,
    snapshot_key_digest: String,
    snapshot_token: String,
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

#[derive(Debug, Clone)]
struct PolicyEntry {
    capabilities: Vec<String>,
    limits: RuntimeLimitsDto,
}

#[derive(Default)]
pub struct SidecarSession {
    programs: HashMap<String, ProgramEntry>,
    snapshots: HashMap<String, Vec<u8>>,
    policies: HashMap<String, PolicyEntry>,
}

impl SidecarSession {
    pub fn new() -> Self {
        Self::default()
    }

    fn register_policy(
        &mut self,
        capabilities: &[String],
        limits: &RuntimeLimitsDto,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct PolicySeed<'a> {
            capabilities: &'a [String],
            limits: &'a RuntimeLimitsDto,
        }

        let encoded = serde_json::to_vec(&PolicySeed {
            capabilities,
            limits,
        })
        .context("failed to serialize policy cache seed")?;
        let policy_id = digest_hex(&encoded);
        self.policies
            .entry(policy_id.clone())
            .or_insert_with(|| PolicyEntry {
                capabilities: capabilities.to_vec(),
                limits: limits.clone(),
            });
        Ok(policy_id)
    }

    fn resolve_snapshot(
        &mut self,
        snapshot_id: Option<String>,
        snapshot_base64: Option<String>,
    ) -> Result<(String, Vec<u8>)> {
        if let Some(snapshot_id) = snapshot_id.as_ref()
            && let Some(snapshot) = self.snapshots.get(snapshot_id)
        {
            return Ok((snapshot_id.clone(), snapshot.clone()));
        }

        let Some(snapshot_base64) = snapshot_base64 else {
            if let Some(snapshot_id) = snapshot_id {
                return Err(anyhow!("unknown snapshot_id `{snapshot_id}`"));
            }
            return Err(anyhow!("resume requires snapshot_base64 or snapshot_id"));
        };

        let bytes = decode_base64(&snapshot_base64)?;
        let derived_snapshot_id = digest_hex(&bytes);
        if let Some(snapshot_id) = snapshot_id
            && snapshot_id != derived_snapshot_id
        {
            return Err(anyhow!("snapshot_id did not match snapshot_base64"));
        }
        self.snapshots
            .entry(derived_snapshot_id.clone())
            .or_insert_with(|| bytes.clone());
        Ok((derived_snapshot_id, bytes))
    }

    fn resolve_resume_policy(
        &mut self,
        policy: Option<Box<SnapshotPolicyDto>>,
        policy_id: Option<String>,
        snapshot_id: &str,
        auth: Option<Box<ResumeAuth>>,
    ) -> Result<(SnapshotPolicyDto, Option<String>)> {
        match (policy, policy_id) {
            (Some(policy), Some(policy_id)) => {
                let registered = self.register_policy(
                    &policy.capabilities,
                    policy
                        .limits
                        .as_ref()
                        .ok_or_else(|| anyhow!("raw snapshot restore requires explicit limits"))?,
                )?;
                if registered != policy_id {
                    return Err(anyhow!("policy_id did not match policy"));
                }
                Ok(((*policy).clone(), Some(policy_id)))
            }
            (Some(policy), None) => {
                let policy_id = self.register_policy(
                    &policy.capabilities,
                    policy
                        .limits
                        .as_ref()
                        .ok_or_else(|| anyhow!("raw snapshot restore requires explicit limits"))?,
                )?;
                Ok(((*policy).clone(), Some(policy_id)))
            }
            (None, Some(policy_id)) => {
                let entry = self
                    .policies
                    .get(&policy_id)
                    .ok_or_else(|| anyhow!("unknown policy_id `{policy_id}`"))?;
                let auth =
                    auth.ok_or_else(|| anyhow!("resume with policy_id requires auth metadata"))?;
                Ok((
                    SnapshotPolicyDto {
                        capabilities: entry.capabilities.clone(),
                        limits: Some(entry.limits.clone()),
                        snapshot_key_base64: Some(auth.snapshot_key_base64),
                        snapshot_token: Some(auth.snapshot_token),
                        snapshot_id: Some(snapshot_id.to_string()),
                        snapshot_key_digest: Some(auth.snapshot_key_digest),
                    },
                    Some(policy_id),
                ))
            }
            (None, None) => Err(anyhow!("resume requires policy or policy_id")),
        }
    }

    fn suspended_step_metadata(
        &mut self,
        step: &StepDto,
        policy_id: Option<String>,
    ) -> Result<(Option<String>, Option<String>)> {
        let StepDto::Suspended {
            snapshot_base64, ..
        } = step
        else {
            return Ok((None, None));
        };
        let bytes = decode_base64(snapshot_base64)?;
        let snapshot_id = digest_hex(&bytes);
        self.snapshots.entry(snapshot_id.clone()).or_insert(bytes);
        Ok((Some(snapshot_id), policy_id))
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
                let policy_id = self.register_policy(&options.capabilities, &options.limits)?;
                let step = start_shared_program(program, options, None)?;
                let (snapshot_id, policy_id) =
                    self.suspended_step_metadata(&step, Some(policy_id))?;
                Ok(ResponsePayload::Step {
                    step,
                    snapshot_id,
                    policy_id,
                })
            })(),
            Request::Resume {
                snapshot_id,
                snapshot_base64,
                policy,
                policy_id,
                auth,
                payload,
                ..
            } => (|| {
                let (snapshot_id, snapshot_bytes) =
                    self.resolve_snapshot(snapshot_id, snapshot_base64)?;
                let (policy, policy_id) =
                    self.resolve_resume_policy(policy, policy_id, &snapshot_id, auth)?;
                let step = resume_program(&snapshot_bytes, *payload, policy, None)?;
                let (next_snapshot_id, policy_id) =
                    self.suspended_step_metadata(&step, policy_id)?;
                Ok(ResponsePayload::Step {
                    step,
                    snapshot_id: next_snapshot_id,
                    policy_id,
                })
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
