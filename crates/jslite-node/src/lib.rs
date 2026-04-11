use base64::{Engine as _, engine::general_purpose::STANDARD};
use napi::bindgen_prelude::Buffer;
use napi::{Error, Result};
use napi_derive::napi;
use serde::{Deserialize, Serialize};

use jslite::{
    BytecodeProgram, ExecutionOptions, ExecutionStep, HostError, ResumePayload, StructuredValue,
    compile, dump_program, dump_snapshot, load_program, load_snapshot, lower_to_bytecode, resume,
    start_bytecode,
};

#[derive(Debug, Serialize, Deserialize)]
struct StartOptionsDto {
    #[serde(default)]
    inputs: std::collections::BTreeMap<String, StructuredValue>,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StepDto {
    Completed {
        value: StructuredValue,
    },
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
        snapshot_base64: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResumeDto {
    Value { value: StructuredValue },
    Error { error: HostError },
}

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}

fn parse_json<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(to_napi_error)
}

fn encode_step(step: ExecutionStep) -> Result<String> {
    let dto = match step {
        ExecutionStep::Completed(value) => StepDto::Completed { value },
        ExecutionStep::Suspended(suspension) => StepDto::Suspended {
            capability: suspension.capability,
            args: suspension.args,
            snapshot_base64: STANDARD
                .encode(dump_snapshot(&suspension.snapshot).map_err(to_napi_error)?),
        },
    };
    serde_json::to_string(&dto).map_err(to_napi_error)
}

fn decode_program(bytes: Buffer) -> Result<BytecodeProgram> {
    load_program(bytes.as_ref()).map_err(to_napi_error)
}

#[napi]
pub fn compile_program(source: String) -> Result<Buffer> {
    let parsed = compile(&source).map_err(to_napi_error)?;
    let bytecode = lower_to_bytecode(&parsed).map_err(to_napi_error)?;
    let bytes = dump_program(&bytecode).map_err(to_napi_error)?;
    Ok(Buffer::from(bytes))
}

#[napi]
pub fn start_program(program: Buffer, options_json: String) -> Result<String> {
    let program = decode_program(program)?;
    let options: StartOptionsDto = parse_json(&options_json)?;
    let step = start_bytecode(
        &program,
        ExecutionOptions {
            inputs: options.inputs.into_iter().collect(),
            capabilities: options.capabilities,
            ..ExecutionOptions::default()
        },
    )
    .map_err(to_napi_error)?;
    encode_step(step)
}

#[napi]
pub fn resume_program(snapshot: Buffer, payload_json: String) -> Result<String> {
    let snapshot = load_snapshot(snapshot.as_ref()).map_err(to_napi_error)?;
    let payload: ResumeDto = parse_json(&payload_json)?;
    let payload = match payload {
        ResumeDto::Value { value } => ResumePayload::Value(value),
        ResumeDto::Error { error } => ResumePayload::Error(error),
    };
    let step = resume(snapshot, payload).map_err(to_napi_error)?;
    encode_step(step)
}
