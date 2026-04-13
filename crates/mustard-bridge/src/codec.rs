use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mustard::{
    BytecodeProgram, ExecutionStep, dump_detached_snapshot, dump_snapshot, load_program,
};
use serde::{Serialize, de::DeserializeOwned};

use crate::dto::StepDto;

pub fn parse_json<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(Into::into)
}

pub fn encode_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(Into::into)
}

pub fn encode_step(step: ExecutionStep) -> Result<StepDto> {
    Ok(match step {
        ExecutionStep::Completed(value) => StepDto::Completed { value },
        ExecutionStep::Suspended(suspension) => StepDto::Suspended {
            capability: suspension.capability,
            args: suspension.args,
            snapshot_base64: STANDARD.encode(dump_snapshot(&suspension.snapshot)?),
        },
    })
}

pub fn encode_detached_step(step: ExecutionStep) -> Result<StepDto> {
    Ok(match step {
        ExecutionStep::Completed(value) => StepDto::Completed { value },
        ExecutionStep::Suspended(suspension) => StepDto::Suspended {
            capability: suspension.capability,
            args: suspension.args,
            snapshot_base64: STANDARD.encode(dump_detached_snapshot(&suspension.snapshot)?),
        },
    })
}

pub fn encode_step_json(step: ExecutionStep) -> Result<String> {
    encode_json(&encode_step(step)?)
}

pub fn decode_program(bytes: &[u8]) -> Result<BytecodeProgram> {
    load_program(bytes).map_err(Into::into)
}

pub fn decode_base64(value: &str) -> Result<Vec<u8>> {
    STANDARD.decode(value).map_err(Into::into)
}

pub fn decode_program_base64(value: &str) -> Result<BytecodeProgram> {
    let bytes = decode_base64(value)?;
    decode_program(&bytes)
}

pub fn encode_bytes_base64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}
