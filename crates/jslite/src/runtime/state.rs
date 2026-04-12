use std::collections::{HashSet, VecDeque};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};

use crate::{
    cancellation::CancellationToken, limits::RuntimeLimits, span::SourceSpan,
    structured::StructuredValue,
};

use super::{api::ExecutionStep, bytecode::BytecodeProgram};

new_key_type! { pub(super) struct EnvKey; }
new_key_type! { pub(super) struct CellKey; }
new_key_type! { pub(super) struct ObjectKey; }
new_key_type! { pub(super) struct ArrayKey; }
new_key_type! { pub(super) struct MapKey; }
new_key_type! { pub(super) struct SetKey; }
new_key_type! { pub(super) struct IteratorKey; }
new_key_type! { pub(super) struct ClosureKey; }
new_key_type! { pub(super) struct PromiseKey; }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(ObjectKey),
    Array(ArrayKey),
    Map(MapKey),
    Set(SetKey),
    Iterator(IteratorKey),
    Closure(ClosureKey),
    Promise(PromiseKey),
    BuiltinFunction(BuiltinFunction),
    HostFunction(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum BuiltinFunction {
    ArrayCtor,
    ArrayFrom,
    ArrayIsArray,
    ArrayPush,
    ArrayPop,
    ArraySlice,
    ArrayJoin,
    ArrayIncludes,
    ArrayIndexOf,
    ArraySort,
    ArrayValues,
    ArrayKeys,
    ArrayEntries,
    ArrayForEach,
    ArrayMap,
    ArrayFilter,
    ArrayFind,
    ArrayFindIndex,
    ArraySome,
    ArrayEvery,
    ArrayReduce,
    ObjectCtor,
    ObjectFromEntries,
    ObjectKeys,
    ObjectValues,
    ObjectEntries,
    ObjectHasOwn,
    MapCtor,
    MapGet,
    MapSet,
    MapHas,
    MapDelete,
    MapClear,
    MapEntries,
    MapKeys,
    MapValues,
    SetCtor,
    SetAdd,
    SetHas,
    SetDelete,
    SetClear,
    SetEntries,
    SetKeys,
    SetValues,
    IteratorNext,
    PromiseCtor,
    PromiseResolve,
    PromiseReject,
    PromiseResolveFunction(PromiseKey),
    PromiseRejectFunction(PromiseKey),
    PromiseThen,
    PromiseCatch,
    PromiseFinally,
    PromiseAll,
    PromiseRace,
    PromiseAny,
    PromiseAllSettled,
    RegExpCtor,
    RegExpExec,
    RegExpTest,
    ErrorCtor,
    TypeErrorCtor,
    ReferenceErrorCtor,
    RangeErrorCtor,
    NumberCtor,
    DateCtor,
    DateNow,
    DateGetTime,
    StringCtor,
    StringTrim,
    StringIncludes,
    StringStartsWith,
    StringEndsWith,
    StringSlice,
    StringSubstring,
    StringToLowerCase,
    StringToUpperCase,
    StringSplit,
    StringReplace,
    StringReplaceAll,
    StringSearch,
    StringMatch,
    StringMatchAll,
    BooleanCtor,
    MathAbs,
    MathMax,
    MathMin,
    MathFloor,
    MathCeil,
    MathRound,
    MathPow,
    MathSqrt,
    MathTrunc,
    MathSign,
    JsonStringify,
    JsonParse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Env {
    pub(super) parent: Option<EnvKey>,
    pub(super) bindings: IndexMap<String, CellKey>,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Cell {
    pub(super) value: Value,
    pub(super) mutable: bool,
    pub(super) initialized: bool,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PlainObject {
    pub(super) properties: IndexMap<String, Value>,
    pub(super) kind: ObjectKind,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum ObjectKind {
    Plain,
    Global,
    Math,
    Json,
    Console,
    Error(String),
    Date(DateObject),
    RegExp(RegExpObject),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DateObject {
    pub(super) timestamp_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RegExpObject {
    pub(super) pattern: String,
    pub(super) flags: String,
    pub(super) last_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ArrayObject {
    pub(super) elements: Vec<Value>,
    pub(super) properties: IndexMap<String, Value>,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MapObject {
    pub(super) entries: Vec<MapEntry>,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MapEntry {
    pub(super) key: Value,
    pub(super) value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SetObject {
    pub(super) entries: Vec<Value>,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IteratorObject {
    pub(super) state: IteratorState,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum IteratorState {
    Array(ArrayIteratorState),
    ArrayKeys(ArrayIteratorState),
    ArrayEntries(ArrayIteratorState),
    String(StringIteratorState),
    MapEntries(MapIteratorState),
    MapKeys(MapIteratorState),
    MapValues(MapIteratorState),
    SetEntries(SetIteratorState),
    SetValues(SetIteratorState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ArrayIteratorState {
    pub(super) array: ArrayKey,
    pub(super) next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StringIteratorState {
    pub(super) value: String,
    pub(super) next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MapIteratorState {
    pub(super) map: MapKey,
    pub(super) next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SetIteratorState {
    pub(super) set: SetKey,
    pub(super) next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Closure {
    pub(super) function_id: usize,
    pub(super) env: EnvKey,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PromiseObject {
    pub(super) state: PromiseState,
    pub(super) awaiters: Vec<AsyncContinuation>,
    pub(super) dependents: Vec<PromiseKey>,
    pub(super) reactions: Vec<PromiseReaction>,
    pub(super) driver: Option<PromiseDriver>,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum PromiseState {
    Pending,
    Fulfilled(Value),
    Rejected(PromiseRejection),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PromiseRejection {
    pub(super) value: Value,
    pub(super) span: Option<SourceSpan>,
    pub(super) traceback: Vec<TraceFrameSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TraceFrameSnapshot {
    pub(super) function_name: Option<String>,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AsyncContinuation {
    pub(super) frames: Vec<Frame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum PromiseOutcome {
    Fulfilled(Value),
    Rejected(PromiseRejection),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum PromiseReaction {
    Then {
        target: PromiseKey,
        on_fulfilled: Option<Value>,
        on_rejected: Option<Value>,
    },
    Finally {
        target: PromiseKey,
        callback: Option<Value>,
    },
    FinallyPassThrough {
        target: PromiseKey,
        original_outcome: PromiseOutcome,
    },
    Combinator {
        target: PromiseKey,
        index: usize,
        kind: PromiseCombinatorKind,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) enum PromiseCombinatorKind {
    All,
    AllSettled,
    Any,
    Race,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum PromiseDriver {
    Thenable {
        value: Value,
    },
    All {
        remaining: usize,
        values: Vec<Option<Value>>,
    },
    AllSettled {
        remaining: usize,
        results: Vec<Option<PromiseSettledResult>>,
    },
    Any {
        remaining: usize,
        reasons: Vec<Option<Value>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum PromiseSettledResult {
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum MicrotaskJob {
    ResumeAsync {
        continuation: AsyncContinuation,
        outcome: PromiseOutcome,
    },
    PromiseReaction {
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PendingHostCall {
    pub(super) capability: String,
    pub(super) args: Vec<StructuredValue>,
    pub(super) promise: Option<PromiseKey>,
    pub(super) resume_behavior: ResumeBehavior,
    pub(super) traceback: Vec<TraceFrameSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Frame {
    pub(super) function_id: usize,
    pub(super) ip: usize,
    pub(super) env: EnvKey,
    pub(super) scope_stack: Vec<EnvKey>,
    pub(super) stack: Vec<Value>,
    pub(super) handlers: Vec<ExceptionHandler>,
    pub(super) pending_exception: Option<Value>,
    pub(super) pending_completions: Vec<CompletionRecord>,
    pub(super) active_finally: Vec<ActiveFinallyState>,
    pub(super) async_promise: Option<PromiseKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ExceptionHandler {
    pub(super) catch: Option<usize>,
    pub(super) finally: Option<usize>,
    pub(super) env: EnvKey,
    pub(super) scope_stack_len: usize,
    pub(super) stack_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum CompletionRecord {
    Jump {
        target: usize,
        target_handler_depth: usize,
        target_scope_depth: usize,
    },
    Return(Value),
    Throw(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ActiveFinallyState {
    pub(super) completion_index: usize,
    pub(super) exit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Runtime {
    pub(super) program: BytecodeProgram,
    pub(super) limits: RuntimeLimits,
    pub(super) globals: EnvKey,
    pub(super) envs: SlotMap<EnvKey, Env>,
    pub(super) cells: SlotMap<CellKey, Cell>,
    pub(super) objects: SlotMap<ObjectKey, PlainObject>,
    pub(super) arrays: SlotMap<ArrayKey, ArrayObject>,
    pub(super) maps: SlotMap<MapKey, MapObject>,
    pub(super) sets: SlotMap<SetKey, SetObject>,
    pub(super) iterators: SlotMap<IteratorKey, IteratorObject>,
    pub(super) closures: SlotMap<ClosureKey, Closure>,
    pub(super) promises: SlotMap<PromiseKey, PromiseObject>,
    pub(super) frames: Vec<Frame>,
    pub(super) root_result: Option<Value>,
    pub(super) microtasks: VecDeque<MicrotaskJob>,
    pub(super) pending_host_calls: VecDeque<PendingHostCall>,
    pub(super) suspended_host_call: Option<PendingHostCall>,
    pub(super) snapshot_nonce: u64,
    pub(super) instruction_counter: usize,
    #[serde(skip, default)]
    pub(super) heap_bytes_used: usize,
    #[serde(skip, default)]
    pub(super) allocation_count: usize,
    #[serde(skip, default)]
    pub(super) cancellation_token: Option<CancellationToken>,
    #[serde(skip, default)]
    pub(super) pending_internal_exception: Option<PromiseRejection>,
    #[serde(skip, default)]
    pub(super) snapshot_policy_required: bool,
    pub(super) pending_resume_behavior: ResumeBehavior,
}

pub(super) enum RunState {
    Completed(Value),
    PushedFrame,
    StartedAsync(Value),
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
        resume_behavior: ResumeBehavior,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) enum ResumeBehavior {
    Value,
    Undefined,
}

pub(super) enum StepAction {
    Continue,
    Return(ExecutionStep),
}

#[derive(Debug, Default)]
pub(super) struct GarbageCollectionMarks {
    pub(super) envs: HashSet<EnvKey>,
    pub(super) cells: HashSet<CellKey>,
    pub(super) objects: HashSet<ObjectKey>,
    pub(super) arrays: HashSet<ArrayKey>,
    pub(super) maps: HashSet<MapKey>,
    pub(super) sets: HashSet<SetKey>,
    pub(super) iterators: HashSet<IteratorKey>,
    pub(super) closures: HashSet<ClosureKey>,
    pub(super) promises: HashSet<PromiseKey>,
}

#[derive(Debug, Default)]
pub(super) struct GarbageCollectionWorklist {
    pub(super) envs: Vec<EnvKey>,
    pub(super) cells: Vec<CellKey>,
    pub(super) objects: Vec<ObjectKey>,
    pub(super) arrays: Vec<ArrayKey>,
    pub(super) maps: Vec<MapKey>,
    pub(super) sets: Vec<SetKey>,
    pub(super) iterators: Vec<IteratorKey>,
    pub(super) closures: Vec<ClosureKey>,
    pub(super) promises: Vec<PromiseKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GarbageCollectionStats {
    pub(super) reclaimed_bytes: usize,
    pub(super) reclaimed_allocations: usize,
}
