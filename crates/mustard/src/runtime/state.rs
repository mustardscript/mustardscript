use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use indexmap::IndexMap;
use num_bigint::BigInt;
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) enum Value {
    #[default]
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
    BigInt(BigInt),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum BuiltinFunction {
    FunctionCtor,
    FunctionCall,
    FunctionApply,
    FunctionBind,
    ArrayCtor,
    ArrayFrom,
    ArrayOf,
    ArrayIsArray,
    ArrayPush,
    ArrayPop,
    ArraySlice,
    ArraySplice,
    ArrayConcat,
    ArrayAt,
    ArrayJoin,
    ArrayIncludes,
    ArrayIndexOf,
    ArrayLastIndexOf,
    ArrayReverse,
    ArrayFill,
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
    ArrayFlat,
    ArrayFlatMap,
    ArrayReduce,
    ArrayReduceRight,
    ArrayFindLast,
    ArrayFindLastIndex,
    ObjectCtor,
    ObjectAssign,
    ObjectCreate,
    ObjectFreeze,
    ObjectSeal,
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
    MapForEach,
    SetCtor,
    SetAdd,
    SetHas,
    SetDelete,
    SetClear,
    SetEntries,
    SetKeys,
    SetValues,
    SetForEach,
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
    SyntaxErrorCtor,
    NumberCtor,
    NumberParseInt,
    NumberParseFloat,
    NumberIsNaN,
    NumberIsFinite,
    NumberIsInteger,
    NumberIsSafeInteger,
    DateCtor,
    DateNow,
    DateGetTime,
    DateValueOf,
    DateToISOString,
    DateToJSON,
    DateGetUTCFullYear,
    DateGetUTCMonth,
    DateGetUTCDate,
    DateGetUTCHours,
    DateGetUTCMinutes,
    DateGetUTCSeconds,
    IntlDateTimeFormatCtor,
    IntlNumberFormatCtor,
    IntlDateTimeFormatFormat,
    IntlDateTimeFormatResolvedOptions,
    IntlNumberFormatFormat,
    IntlNumberFormatResolvedOptions,
    StringCtor,
    StringTrim,
    StringTrimStart,
    StringTrimEnd,
    StringIncludes,
    StringStartsWith,
    StringEndsWith,
    StringIndexOf,
    StringLastIndexOf,
    StringCharAt,
    StringAt,
    StringSlice,
    StringSubstring,
    StringToLowerCase,
    StringToUpperCase,
    StringRepeat,
    StringConcat,
    StringPadStart,
    StringPadEnd,
    StringSplit,
    StringReplace,
    StringReplaceAll,
    StringSearch,
    StringMatch,
    StringMatchAll,
    StringToString,
    StringValueOf,
    BooleanCtor,
    BooleanToString,
    BooleanValueOf,
    NumberToString,
    NumberValueOf,
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
    MathLog,
    MathExp,
    MathLog2,
    MathLog10,
    MathSin,
    MathCos,
    MathAtan2,
    MathHypot,
    MathCbrt,
    MathRandom,
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
    Intl,
    FunctionPrototype(Value),
    BoundFunction(BoundFunctionData),
    Error(String),
    Date(DateObject),
    RegExp(RegExpObject),
    NumberObject(f64),
    StringObject(String),
    BooleanObject(bool),
    IntlDateTimeFormat(IntlDateTimeFormatObject),
    IntlNumberFormat(IntlNumberFormatObject),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct BoundFunctionData {
    pub(super) target: Value,
    pub(super) this_value: Value,
    pub(super) args: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DateObject {
    pub(super) timestamp_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IntlDateTimeFormatObject {
    pub(super) locale: String,
    pub(super) time_zone: String,
    pub(super) year: Option<IntlFieldStyle>,
    pub(super) month: Option<IntlFieldStyle>,
    pub(super) day: Option<IntlFieldStyle>,
    pub(super) hour: Option<IntlFieldStyle>,
    pub(super) minute: Option<IntlFieldStyle>,
    pub(super) second: Option<IntlFieldStyle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IntlNumberFormatObject {
    pub(super) locale: String,
    pub(super) style: IntlNumberStyle,
    pub(super) currency: Option<String>,
    pub(super) minimum_fraction_digits: usize,
    pub(super) maximum_fraction_digits: usize,
    pub(super) use_grouping: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum IntlFieldStyle {
    Numeric,
    TwoDigit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum IntlNumberStyle {
    Decimal,
    Percent,
    Currency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RegExpObject {
    pub(super) pattern: String,
    pub(super) flags: String,
    pub(super) last_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ArrayObject {
    pub(super) elements: Vec<Option<Value>>,
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
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) this_value: Value,
    #[serde(default)]
    pub(super) prototype: Option<ObjectKey>,
    #[serde(default)]
    pub(super) properties: IndexMap<String, Value>,
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

fn accounting_recount_required_after_deserialize() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Runtime {
    pub(super) program: Arc<BytecodeProgram>,
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
    #[serde(default)]
    pub(super) builtin_prototypes: IndexMap<BuiltinFunction, ObjectKey>,
    #[serde(default)]
    pub(super) builtin_function_objects: IndexMap<BuiltinFunction, ObjectKey>,
    #[serde(default)]
    pub(super) host_function_objects: IndexMap<String, ObjectKey>,
    pub(super) snapshot_nonce: u64,
    pub(super) instruction_counter: usize,
    #[serde(skip, default)]
    pub(super) heap_bytes_used: usize,
    #[serde(skip, default)]
    pub(super) allocation_count: usize,
    #[serde(skip, default = "accounting_recount_required_after_deserialize")]
    pub(super) accounting_recount_required: bool,
    #[serde(skip, default)]
    pub(super) cancellation_token: Option<CancellationToken>,
    #[serde(skip, default)]
    pub(super) pending_internal_exception: Option<PromiseRejection>,
    #[serde(skip, default)]
    pub(super) snapshot_policy_required: bool,
    pub(super) pending_resume_behavior: ResumeBehavior,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeImage {
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
    pub(super) builtin_prototypes: IndexMap<BuiltinFunction, ObjectKey>,
    pub(super) builtin_function_objects: IndexMap<BuiltinFunction, ObjectKey>,
    pub(super) host_function_objects: IndexMap<String, ObjectKey>,
    pub(super) heap_bytes_used: usize,
    pub(super) allocation_count: usize,
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
