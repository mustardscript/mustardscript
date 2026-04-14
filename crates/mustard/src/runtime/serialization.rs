use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use slotmap::SlotMap;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use crate::{
    CollectionCallSiteMetrics, RuntimeDebugMetrics,
    diagnostic::{DiagnosticKind, MustardError, MustardResult},
    limits::RuntimeLimits,
};

use super::{
    Runtime,
    api::ExecutionSnapshot,
    bytecode::BytecodeProgram,
    state::{
        ArrayKey, ArrayObject, Cell, CellKey, Closure, ClosureKey, CollectionCallSiteKey, Env,
        EnvKey, Frame, IteratorKey, IteratorObject, MapKey, MapObject, MicrotaskJob, ObjectKey,
        PendingHostCall, PlainObject, PromiseKey, PromiseObject, ResumeBehavior, SetKey, SetObject,
        Value,
    },
    validation::validate_bytecode_program,
};

const SERIAL_FORMAT_VERSION: u32 = 2;

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

fn program_identity(program: &BytecodeProgram) -> MustardResult<[u8; 32]> {
    let bytes = dump_program(program)?;
    Ok(Sha256::digest(&bytes).into())
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

pub fn dump_detached_snapshot(snapshot: &ExecutionSnapshot) -> MustardResult<Vec<u8>> {
    bincode::serialize(&DetachedSerializedSnapshotRef {
        version: SERIAL_FORMAT_VERSION,
        program_identity: program_identity(snapshot.runtime.program.as_ref())?,
        runtime: DetachedRuntimeRef::from_runtime(&snapshot.runtime),
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

pub fn load_detached_snapshot(
    bytes: &[u8],
    program: Arc<BytecodeProgram>,
) -> MustardResult<ExecutionSnapshot> {
    let decoded: DetachedSerializedSnapshot =
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
    let expected_program_identity = program_identity(program.as_ref())?;
    if decoded.program_identity != expected_program_identity {
        return Err(MustardError::Message {
            kind: DiagnosticKind::Serialization,
            message: "detached snapshot program identity mismatch".to_string(),
            span: None,
            traceback: Vec::new(),
        });
    }
    let runtime = decoded.runtime.into_runtime(program);
    ExecutionSnapshot::restore_loaded_runtime(runtime)
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

#[derive(Debug, Serialize)]
struct DetachedSerializedSnapshotRef<'a> {
    version: u32,
    program_identity: [u8; 32],
    runtime: DetachedRuntimeRef<'a>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DetachedSerializedSnapshot {
    version: u32,
    program_identity: [u8; 32],
    runtime: DetachedRuntime,
}

#[derive(Debug, Serialize)]
struct DetachedRuntimeRef<'a> {
    limits: &'a RuntimeLimits,
    globals: EnvKey,
    envs: &'a SlotMap<EnvKey, Env>,
    cells: &'a SlotMap<CellKey, Cell>,
    objects: &'a SlotMap<ObjectKey, PlainObject>,
    arrays: &'a SlotMap<ArrayKey, ArrayObject>,
    maps: &'a SlotMap<MapKey, MapObject>,
    sets: &'a SlotMap<SetKey, SetObject>,
    iterators: &'a SlotMap<IteratorKey, IteratorObject>,
    closures: &'a SlotMap<ClosureKey, Closure>,
    promises: &'a SlotMap<PromiseKey, PromiseObject>,
    frames: &'a Vec<Frame>,
    root_result: &'a Option<Value>,
    microtasks: &'a VecDeque<MicrotaskJob>,
    pending_host_calls: &'a VecDeque<PendingHostCall>,
    suspended_host_call: &'a Option<PendingHostCall>,
    builtin_prototypes: &'a IndexMap<super::state::BuiltinFunction, ObjectKey>,
    builtin_function_objects: &'a IndexMap<super::state::BuiltinFunction, ObjectKey>,
    host_function_objects: &'a IndexMap<String, ObjectKey>,
    collection_call_sites: &'a HashMap<CollectionCallSiteKey, CollectionCallSiteMetrics>,
    snapshot_nonce: u64,
    instruction_counter: usize,
    pending_resume_behavior: ResumeBehavior,
}

impl<'a> DetachedRuntimeRef<'a> {
    fn from_runtime(runtime: &'a Runtime) -> Self {
        Self {
            limits: &runtime.limits,
            globals: runtime.globals,
            envs: &runtime.envs,
            cells: &runtime.cells,
            objects: &runtime.objects,
            arrays: &runtime.arrays,
            maps: &runtime.maps,
            sets: &runtime.sets,
            iterators: &runtime.iterators,
            closures: &runtime.closures,
            promises: &runtime.promises,
            frames: &runtime.frames,
            root_result: &runtime.root_result,
            microtasks: &runtime.microtasks,
            pending_host_calls: &runtime.pending_host_calls,
            suspended_host_call: &runtime.suspended_host_call,
            builtin_prototypes: &runtime.builtin_prototypes,
            builtin_function_objects: &runtime.builtin_function_objects,
            host_function_objects: &runtime.host_function_objects,
            collection_call_sites: &runtime.collection_call_sites,
            snapshot_nonce: runtime.snapshot_nonce,
            instruction_counter: runtime.instruction_counter,
            pending_resume_behavior: runtime.pending_resume_behavior,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DetachedRuntime {
    limits: RuntimeLimits,
    globals: EnvKey,
    envs: SlotMap<EnvKey, Env>,
    cells: SlotMap<CellKey, Cell>,
    objects: SlotMap<ObjectKey, PlainObject>,
    arrays: SlotMap<ArrayKey, ArrayObject>,
    maps: SlotMap<MapKey, MapObject>,
    sets: SlotMap<SetKey, SetObject>,
    iterators: SlotMap<IteratorKey, IteratorObject>,
    closures: SlotMap<ClosureKey, Closure>,
    promises: SlotMap<PromiseKey, PromiseObject>,
    frames: Vec<Frame>,
    root_result: Option<Value>,
    microtasks: VecDeque<MicrotaskJob>,
    pending_host_calls: VecDeque<PendingHostCall>,
    suspended_host_call: Option<PendingHostCall>,
    builtin_prototypes: IndexMap<super::state::BuiltinFunction, ObjectKey>,
    builtin_function_objects: IndexMap<super::state::BuiltinFunction, ObjectKey>,
    host_function_objects: IndexMap<String, ObjectKey>,
    #[serde(default)]
    collection_call_sites: HashMap<CollectionCallSiteKey, CollectionCallSiteMetrics>,
    snapshot_nonce: u64,
    instruction_counter: usize,
    pending_resume_behavior: ResumeBehavior,
}

impl DetachedRuntime {
    fn into_runtime(self, program: Arc<BytecodeProgram>) -> Runtime {
        Runtime {
            program,
            limits: self.limits,
            globals: self.globals,
            envs: self.envs,
            cells: self.cells,
            objects: self.objects,
            arrays: self.arrays,
            maps: self.maps,
            sets: self.sets,
            iterators: self.iterators,
            closures: self.closures,
            promises: self.promises,
            frames: self.frames,
            root_result: self.root_result,
            microtasks: self.microtasks,
            pending_host_calls: self.pending_host_calls,
            suspended_host_call: self.suspended_host_call,
            builtin_prototypes: self.builtin_prototypes,
            builtin_function_objects: self.builtin_function_objects,
            host_function_objects: self.host_function_objects,
            object_shapes: HashMap::new(),
            next_object_shape_id: 1,
            static_property_inline_caches: HashMap::new(),
            property_feedback_sites: HashMap::new(),
            builtin_feedback_sites: HashMap::new(),
            collection_call_sites: self.collection_call_sites,
            snapshot_nonce: self.snapshot_nonce,
            instruction_counter: self.instruction_counter,
            heap_bytes_used: 0,
            allocation_count: 0,
            gc_allocation_debt_bytes: 0,
            gc_allocation_debt_count: 0,
            debug_metrics: RuntimeDebugMetrics::default(),
            operation_counters_enabled: false,
            accounting_recount_required: true,
            cancellation_token: None,
            regex_cache: HashMap::new(),
            pending_internal_exception: None,
            pending_sync_callback_result: None,
            snapshot_policy_required: false,
            pending_resume_behavior: self.pending_resume_behavior,
        }
    }
}
