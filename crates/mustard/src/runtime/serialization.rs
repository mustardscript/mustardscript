use serde::{Deserialize, Serialize};

use crate::diagnostic::{DiagnosticKind, MustardError, MustardResult};

use super::{
    Runtime, api::ExecutionSnapshot, bytecode::BytecodeProgram,
    validation::validate_bytecode_program,
};

const SERIAL_FORMAT_VERSION: u32 = 1;

pub fn dump_program(program: &BytecodeProgram) -> MustardResult<Vec<u8>> {
    bincode::serialize(&SerializedProgram {
        version: SERIAL_FORMAT_VERSION,
        program: program.clone(),
    })
    .map_err(|error| MustardError::Message {
        kind: DiagnosticKind::Serialization,
        message: error.to_string(),
        span: None,
        traceback: Vec::new(),
    })
}

pub fn load_program(bytes: &[u8]) -> MustardResult<BytecodeProgram> {
    let decoded: SerializedProgram =
        bincode::deserialize(bytes).map_err(|error| MustardError::Message {
            kind: DiagnosticKind::Serialization,
            message: error.to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    if decoded.version != SERIAL_FORMAT_VERSION {
        return Err(MustardError::Message {
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

pub fn dump_snapshot(snapshot: &ExecutionSnapshot) -> MustardResult<Vec<u8>> {
    bincode::serialize(&SerializedSnapshotRef {
        version: SERIAL_FORMAT_VERSION,
        runtime: &snapshot.runtime,
    })
    .map_err(|error| MustardError::Message {
        kind: DiagnosticKind::Serialization,
        message: error.to_string(),
        span: None,
        traceback: Vec::new(),
    })
}

pub fn load_snapshot(bytes: &[u8]) -> MustardResult<ExecutionSnapshot> {
    let decoded: SerializedSnapshot =
        bincode::deserialize(bytes).map_err(|error| MustardError::Message {
            kind: DiagnosticKind::Serialization,
            message: error.to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    if decoded.version != SERIAL_FORMAT_VERSION {
        return Err(MustardError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "serialized snapshot version mismatch: expected {}, got {}",
                SERIAL_FORMAT_VERSION, decoded.version
            ),
            span: None,
            traceback: Vec::new(),
        });
    }
    ExecutionSnapshot::restore_loaded_runtime(decoded.runtime)
}

pub fn canonical_snapshot_auth_bytes(bytes: &[u8]) -> MustardResult<Vec<u8>> {
    let decoded: SerializedSnapshot =
        bincode::deserialize(bytes).map_err(|error| MustardError::Message {
            kind: DiagnosticKind::Serialization,
            message: error.to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    if decoded.version != SERIAL_FORMAT_VERSION {
        return Err(MustardError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "serialized snapshot version mismatch: expected {}, got {}",
                SERIAL_FORMAT_VERSION, decoded.version
            ),
            span: None,
            traceback: Vec::new(),
        });
    }

    let mut snapshot = ExecutionSnapshot::restore_loaded_runtime(decoded.runtime)?;
    snapshot.runtime.snapshot_nonce = 0;
    bincode::serialize(&SerializedSnapshot {
        version: SERIAL_FORMAT_VERSION,
        runtime: snapshot.runtime,
    })
    .map_err(|error| MustardError::Message {
        kind: DiagnosticKind::Serialization,
        message: error.to_string(),
        span: None,
        traceback: Vec::new(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedProgram {
    version: u32,
    program: BytecodeProgram,
}

#[derive(Debug, Serialize)]
struct SerializedSnapshotRef<'a> {
    version: u32,
    runtime: &'a Runtime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedSnapshot {
    version: u32,
    runtime: Runtime,
}
