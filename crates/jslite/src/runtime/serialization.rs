use serde::{Deserialize, Serialize};

use crate::diagnostic::{DiagnosticKind, JsliteError, JsliteResult};

use super::{
    api::ExecutionSnapshot,
    bytecode::BytecodeProgram,
    validation::{validate_bytecode_program, validate_snapshot},
};

const SERIAL_FORMAT_VERSION: u32 = 1;

pub fn dump_program(program: &BytecodeProgram) -> JsliteResult<Vec<u8>> {
    bincode::serialize(&SerializedProgram {
        version: SERIAL_FORMAT_VERSION,
        program: program.clone(),
    })
    .map_err(|error| JsliteError::Message {
        kind: DiagnosticKind::Serialization,
        message: error.to_string(),
        span: None,
        traceback: Vec::new(),
    })
}

pub fn load_program(bytes: &[u8]) -> JsliteResult<BytecodeProgram> {
    let decoded: SerializedProgram =
        bincode::deserialize(bytes).map_err(|error| JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: error.to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    if decoded.version != SERIAL_FORMAT_VERSION {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "serialized program version mismatch: expected {}, got {}",
                SERIAL_FORMAT_VERSION, decoded.version
            ),
            span: None,
            traceback: Vec::new(),
        });
    }
    validate_bytecode_program(&decoded.program)?;
    Ok(decoded.program)
}

pub fn dump_snapshot(snapshot: &ExecutionSnapshot) -> JsliteResult<Vec<u8>> {
    bincode::serialize(&SerializedSnapshot {
        version: SERIAL_FORMAT_VERSION,
        snapshot: snapshot.clone(),
    })
    .map_err(|error| JsliteError::Message {
        kind: DiagnosticKind::Serialization,
        message: error.to_string(),
        span: None,
        traceback: Vec::new(),
    })
}

pub fn load_snapshot(bytes: &[u8]) -> JsliteResult<ExecutionSnapshot> {
    let decoded: SerializedSnapshot =
        bincode::deserialize(bytes).map_err(|error| JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: error.to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    if decoded.version != SERIAL_FORMAT_VERSION {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "serialized snapshot version mismatch: expected {}, got {}",
                SERIAL_FORMAT_VERSION, decoded.version
            ),
            span: None,
            traceback: Vec::new(),
        });
    }
    let mut snapshot = decoded.snapshot;
    validate_snapshot(&snapshot)?;
    snapshot.runtime.recompute_accounting_after_load()?;
    snapshot.runtime.snapshot_policy_required = true;
    Ok(snapshot)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedProgram {
    version: u32,
    program: BytecodeProgram,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedSnapshot {
    version: u32,
    snapshot: ExecutionSnapshot,
}
