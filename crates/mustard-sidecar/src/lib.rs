use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, anyhow};
use hmac::Mac;
use mustard::{
    BytecodeProgram, ExecutionOptions, ExecutionSnapshot, ExecutionStep, ResumeOptions,
    StructuredValue, dump_snapshot, load_snapshot, resume_with_options, start_shared_bytecode,
};
use mustard_bridge::{
    ResumeDto, RuntimeLimitsDto, SnapshotPolicyDto, StartOptionsDto, StepDto,
    compile_program_bytes, decode_base64, decode_program, encode_bytes_base64,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_REQUEST_LINE_BYTES: usize = 1024 * 1024;
pub const MAX_REQUEST_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
enum Request {
    Compile {
        protocol_version: u32,
        id: u64,
        source: String,
    },
    Start {
        protocol_version: u32,
        id: u64,
        program_id: Option<String>,
        program_bytes: Option<Vec<u8>>,
        options: StartOptionsDto,
    },
    Resume {
        protocol_version: u32,
        id: u64,
        snapshot_id: Option<String>,
        snapshot_bytes: Option<Vec<u8>>,
        policy: Option<Box<SnapshotPolicyDto>>,
        policy_id: Option<String>,
        auth: Option<Box<ResumeAuth>>,
        payload: Box<ResumeDto>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
enum JsonRequest {
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
#[serde(tag = "method", rename_all = "snake_case")]
enum BinaryRequestHeader {
    Compile {
        protocol_version: u32,
        id: u64,
        source: String,
    },
    Start {
        protocol_version: u32,
        id: u64,
        #[serde(default)]
        program_id: Option<String>,
        options: StartOptionsDto,
    },
    Resume {
        protocol_version: u32,
        id: u64,
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

#[derive(Debug)]
struct Response {
    protocol_version: u32,
    id: u64,
    ok: bool,
    result: Option<ResponseBody>,
    error: Option<String>,
}

#[derive(Debug)]
enum ResponseBody {
    Program {
        program_bytes: Vec<u8>,
        program_id: String,
    },
    Step {
        step: StepBody,
        snapshot_id: Option<String>,
        policy_id: Option<String>,
    },
}

#[derive(Debug)]
enum StepBody {
    Completed {
        value: StructuredValue,
    },
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
        snapshot_bytes: Vec<u8>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonResponse {
    protocol_version: u32,
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<JsonResponsePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JsonResponsePayload {
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

#[derive(Debug, Serialize, Deserialize)]
struct BinaryResponseHeader {
    protocol_version: u32,
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<BinaryResponsePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BinaryResponsePayload {
    Program {
        program_id: String,
    },
    Step {
        step: BinaryStepHeader,
        #[serde(skip_serializing_if = "Option::is_none")]
        snapshot_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        policy_id: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BinaryStepHeader {
    Completed {
        value: StructuredValue,
    },
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
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
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(snapshot_key)
        .map_err(|_| anyhow!("invalid snapshot key"))?;
    mac.update(snapshot_id.as_bytes());
    let digest = mac.finalize().into_bytes();
    let mut token = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    Ok(token)
}

fn assert_authenticated_snapshot_id(snapshot_id: &str, policy: &SnapshotPolicyDto) -> Result<()> {
    let snapshot_key_base64 = policy
        .snapshot_key_base64
        .as_deref()
        .ok_or_else(|| anyhow!("raw snapshot restore requires snapshot_key_base64"))?;
    let snapshot_token = policy
        .snapshot_token
        .as_deref()
        .ok_or_else(|| anyhow!("raw snapshot restore requires snapshot_token"))?;
    let snapshot_key_digest = policy
        .snapshot_key_digest
        .as_deref()
        .ok_or_else(|| anyhow!("raw snapshot restore requires snapshot_key_digest"))?;
    let snapshot_key = decode_base64(snapshot_key_base64)?;
    if snapshot_key_digest_hex(&snapshot_key) != snapshot_key_digest {
        return Err(anyhow!(
            "raw snapshot restore rejected a mismatched snapshot key digest"
        ));
    }
    let expected = encode_snapshot_token(snapshot_id, &snapshot_key)?;
    if expected != snapshot_token {
        return Err(anyhow!(
            "raw snapshot restore rejected a tampered or unauthenticated snapshot"
        ));
    }
    Ok(())
}

fn assert_authenticated_snapshot(snapshot_bytes: &[u8], policy: &SnapshotPolicyDto) -> Result<()> {
    let snapshot_id = policy
        .snapshot_id
        .as_deref()
        .ok_or_else(|| anyhow!("raw snapshot restore requires snapshot_id"))?;
    let expected_snapshot_id = digest_hex(snapshot_bytes);
    if expected_snapshot_id != snapshot_id {
        return Err(anyhow!(
            "raw snapshot restore rejected a tampered or unauthenticated snapshot"
        ));
    }
    assert_authenticated_snapshot_id(snapshot_id, policy)
}

fn step_to_json(step: StepBody) -> JsonResponsePayload {
    match step {
        StepBody::Completed { value } => JsonResponsePayload::Step {
            step: StepDto::Completed { value },
            snapshot_id: None,
            policy_id: None,
        },
        StepBody::Suspended {
            capability,
            args,
            snapshot_bytes,
        } => JsonResponsePayload::Step {
            step: StepDto::Suspended {
                capability,
                args,
                snapshot_base64: encode_bytes_base64(&snapshot_bytes),
            },
            snapshot_id: None,
            policy_id: None,
        },
    }
}

fn request_from_json(request: JsonRequest) -> Result<Request> {
    Ok(match request {
        JsonRequest::Compile {
            protocol_version,
            id,
            source,
        } => Request::Compile {
            protocol_version,
            id,
            source,
        },
        JsonRequest::Start {
            protocol_version,
            id,
            program_base64,
            program_id,
            options,
        } => Request::Start {
            protocol_version,
            id,
            program_id,
            program_bytes: program_base64.as_deref().map(decode_base64).transpose()?,
            options,
        },
        JsonRequest::Resume {
            protocol_version,
            id,
            snapshot_base64,
            snapshot_id,
            policy,
            policy_id,
            auth,
            payload,
        } => Request::Resume {
            protocol_version,
            id,
            snapshot_id,
            snapshot_bytes: snapshot_base64.as_deref().map(decode_base64).transpose()?,
            policy,
            policy_id,
            auth,
            payload,
        },
    })
}

fn request_from_binary_header(header: BinaryRequestHeader, blob: Vec<u8>) -> Result<Request> {
    match header {
        BinaryRequestHeader::Compile {
            protocol_version,
            id,
            source,
        } => {
            if !blob.is_empty() {
                return Err(anyhow!(
                    "compile requests must not include a binary payload"
                ));
            }
            Ok(Request::Compile {
                protocol_version,
                id,
                source,
            })
        }
        BinaryRequestHeader::Start {
            protocol_version,
            id,
            program_id,
            options,
        } => Ok(Request::Start {
            protocol_version,
            id,
            program_id,
            program_bytes: (!blob.is_empty()).then_some(blob),
            options,
        }),
        BinaryRequestHeader::Resume {
            protocol_version,
            id,
            snapshot_id,
            policy,
            policy_id,
            auth,
            payload,
        } => Ok(Request::Resume {
            protocol_version,
            id,
            snapshot_id,
            snapshot_bytes: (!blob.is_empty()).then_some(blob),
            policy,
            policy_id,
            auth,
            payload,
        }),
    }
}

fn response_to_json(response: Response) -> JsonResponse {
    let result = match response.result {
        Some(ResponseBody::Program {
            program_bytes,
            program_id,
        }) => Some(JsonResponsePayload::Program {
            program_base64: encode_bytes_base64(&program_bytes),
            program_id,
        }),
        Some(ResponseBody::Step {
            step,
            snapshot_id,
            policy_id,
        }) => {
            let mut step_result = step_to_json(step);
            if let JsonResponsePayload::Step {
                snapshot_id: step_snapshot_id,
                policy_id: step_policy_id,
                ..
            } = &mut step_result
            {
                *step_snapshot_id = snapshot_id;
                *step_policy_id = policy_id;
            }
            Some(step_result)
        }
        None => None,
    };
    JsonResponse {
        protocol_version: response.protocol_version,
        id: response.id,
        ok: response.ok,
        result,
        error: response.error,
    }
}

fn response_to_binary_parts(response: Response) -> Result<(Vec<u8>, Vec<u8>)> {
    let (result, blob) = match response.result {
        Some(ResponseBody::Program {
            program_bytes,
            program_id,
        }) => (
            Some(BinaryResponsePayload::Program { program_id }),
            program_bytes,
        ),
        Some(ResponseBody::Step {
            step,
            snapshot_id,
            policy_id,
        }) => match step {
            StepBody::Completed { value } => (
                Some(BinaryResponsePayload::Step {
                    step: BinaryStepHeader::Completed { value },
                    snapshot_id,
                    policy_id,
                }),
                Vec::new(),
            ),
            StepBody::Suspended {
                capability,
                args,
                snapshot_bytes,
            } => (
                Some(BinaryResponsePayload::Step {
                    step: BinaryStepHeader::Suspended { capability, args },
                    snapshot_id,
                    policy_id,
                }),
                snapshot_bytes,
            ),
        },
        None => (None, Vec::new()),
    };
    let header = BinaryResponseHeader {
        protocol_version: response.protocol_version,
        id: response.id,
        ok: response.ok,
        result,
        error: response.error,
    };
    let encoded = serde_json::to_vec(&header).context("failed to encode response header")?;
    Ok((encoded, blob))
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

#[derive(Debug)]
struct SnapshotEntry {
    bytes: Vec<u8>,
    live: Option<ExecutionSnapshot>,
}

impl SnapshotEntry {
    fn new(bytes: Vec<u8>, live: Option<ExecutionSnapshot>) -> Self {
        Self { bytes, live }
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
    snapshots: HashMap<String, SnapshotEntry>,
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
        snapshot_bytes: Option<Vec<u8>>,
    ) -> Result<(String, Option<Vec<u8>>)> {
        if let Some(snapshot_id) = snapshot_id.as_ref()
            && self.snapshots.contains_key(snapshot_id)
            && snapshot_bytes.is_none()
        {
            return Ok((snapshot_id.clone(), None));
        }

        let Some(bytes) = snapshot_bytes else {
            if let Some(snapshot_id) = snapshot_id {
                return Err(anyhow!("unknown snapshot_id `{snapshot_id}`"));
            }
            return Err(anyhow!("resume requires snapshot bytes or snapshot_id"));
        };

        let derived_snapshot_id = digest_hex(&bytes);
        if let Some(snapshot_id) = snapshot_id
            && snapshot_id != derived_snapshot_id
        {
            return Err(anyhow!("snapshot_id did not match snapshot bytes"));
        }
        Ok((derived_snapshot_id, Some(bytes)))
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

    fn resolve_live_snapshot(
        &mut self,
        snapshot_id: &str,
        inline_snapshot_bytes: Option<Vec<u8>>,
    ) -> Result<ExecutionSnapshot> {
        if let Some(entry) = self.snapshots.get_mut(snapshot_id) {
            if let Some(snapshot) = entry.live.take() {
                return Ok(snapshot);
            }
            return Ok(load_snapshot(&entry.bytes)?);
        }

        let Some(snapshot_bytes) = inline_snapshot_bytes else {
            return Err(anyhow!("unknown snapshot_id `{snapshot_id}`"));
        };
        let snapshot = load_snapshot(&snapshot_bytes)?;
        self.snapshots.insert(
            snapshot_id.to_string(),
            SnapshotEntry::new(snapshot_bytes, None),
        );
        Ok(snapshot)
    }

    fn response_step(
        &mut self,
        step: ExecutionStep,
        policy_id: Option<String>,
    ) -> Result<(StepBody, Option<String>, Option<String>)> {
        match step {
            ExecutionStep::Completed(value) => Ok((StepBody::Completed { value }, None, None)),
            ExecutionStep::Suspended(suspension) => {
                let mustard::Suspension {
                    capability,
                    args,
                    snapshot,
                } = *suspension;
                let snapshot_bytes = dump_snapshot(&snapshot)?;
                let snapshot_id = digest_hex(&snapshot_bytes);
                self.snapshots.insert(
                    snapshot_id.clone(),
                    SnapshotEntry::new(snapshot_bytes.clone(), Some(snapshot)),
                );
                Ok((
                    StepBody::Suspended {
                        capability,
                        args,
                        snapshot_bytes,
                    },
                    Some(snapshot_id),
                    policy_id,
                ))
            }
        }
    }

    fn resolve_program(
        &mut self,
        program_id: Option<String>,
        program_bytes: Option<Vec<u8>>,
    ) -> Result<Arc<BytecodeProgram>> {
        if let Some(program_id) = program_id.as_ref()
            && let Some(entry) = self.programs.get(program_id)
        {
            return entry.resolve();
        }

        let Some(bytes) = program_bytes else {
            if let Some(program_id) = program_id {
                return Err(anyhow!("unknown program_id `{program_id}`"));
            }
            return Err(anyhow!("start requires program bytes or program_id"));
        };

        let derived_program_id = digest_hex(&bytes);
        if let Some(program_id) = program_id
            && program_id != derived_program_id
        {
            return Err(anyhow!("program_id did not match program bytes"));
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

        let result: Result<ResponseBody> = match request {
            Request::Compile { source, .. } => (|| {
                let bytes = compile_program_bytes(&source)?;
                let program_id = digest_hex(&bytes);
                self.programs
                    .entry(program_id.clone())
                    .or_insert_with(|| ProgramEntry::new(bytes.clone()));
                Ok(ResponseBody::Program {
                    program_bytes: bytes,
                    program_id,
                })
            })(),
            Request::Start {
                program_bytes,
                program_id,
                options,
                ..
            } => (|| {
                let program = self.resolve_program(program_id, program_bytes)?;
                let policy_id = self.register_policy(&options.capabilities, &options.limits)?;
                let step = start_shared_bytecode(
                    program,
                    ExecutionOptions {
                        inputs: options.inputs.into_iter().collect(),
                        capabilities: options.capabilities,
                        limits: options.limits.into_runtime_limits(),
                        cancellation_token: None,
                    },
                )?;
                let (step, snapshot_id, policy_id) = self.response_step(step, Some(policy_id))?;
                Ok(ResponseBody::Step {
                    step,
                    snapshot_id,
                    policy_id,
                })
            })(),
            Request::Resume {
                snapshot_id,
                snapshot_bytes,
                policy,
                policy_id,
                auth,
                payload,
                ..
            } => (|| {
                let (snapshot_id, inline_snapshot_bytes) =
                    self.resolve_snapshot(snapshot_id, snapshot_bytes)?;
                let (policy, policy_id) =
                    self.resolve_resume_policy(policy, policy_id, &snapshot_id, auth)?;
                if let Some(snapshot_bytes) = inline_snapshot_bytes.as_deref() {
                    assert_authenticated_snapshot(snapshot_bytes, &policy)?;
                } else {
                    assert_authenticated_snapshot_id(&snapshot_id, &policy)?;
                }
                let snapshot = self.resolve_live_snapshot(&snapshot_id, inline_snapshot_bytes)?;
                let step = resume_with_options(
                    snapshot,
                    payload.into_resume_payload(),
                    ResumeOptions {
                        cancellation_token: None,
                        snapshot_policy: Some(policy.into_snapshot_policy()?),
                    },
                )?;
                let (step, next_snapshot_id, policy_id) = self.response_step(step, policy_id)?;
                Ok(ResponseBody::Step {
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
        let request = request_from_json(serde_json::from_str(line).context("invalid request")?)?;
        let response = response_to_json(self.handle(request));
        let encoded = serde_json::to_string(&response).context("failed to encode response")?;
        Ok(Some(encoded))
    }

    pub fn handle_request_frame(
        &mut self,
        header_json: &str,
        blob: &[u8],
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        if header_json.trim().is_empty() && blob.is_empty() {
            return Ok(None);
        }
        let request = request_from_binary_header(
            serde_json::from_str(header_json).context("invalid request header")?,
            blob.to_vec(),
        )?;
        let response = self.handle(request);
        let (header, payload) = response_to_binary_parts(response)?;
        Ok(Some((header, payload)))
    }
}

pub fn handle_request_line(line: &str) -> Result<Option<String>> {
    SidecarSession::new().handle_request_line(line)
}
