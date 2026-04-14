#[cfg(test)]
use std::sync::{Mutex, MutexGuard};
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use crate::{CollectionCallSiteMetrics, RuntimeDebugMetrics};
use indexmap::{Equivalent, IndexMap};
use num_bigint::BigInt;
use regex::Regex;

use serde::{Deserialize, Serialize};
use slotmap::{SecondaryMap, SlotMap, new_key_type};

use crate::{
    cancellation::CancellationToken, limits::RuntimeLimits, span::SourceSpan,
    structured::StructuredValue,
};

use super::{
    api::ExecutionStep, bytecode::BytecodeProgram, properties::array_index_from_property_key,
};

new_key_type! { pub(super) struct EnvKey; }
new_key_type! { pub(super) struct CellKey; }
new_key_type! { pub(super) struct ObjectKey; }
new_key_type! { pub(super) struct ArrayKey; }
new_key_type! { pub(super) struct MapKey; }
new_key_type! { pub(super) struct SetKey; }
new_key_type! { pub(super) struct IteratorKey; }
new_key_type! { pub(super) struct ClosureKey; }
new_key_type! { pub(super) struct PromiseKey; }

pub(super) const COLLECTION_LOOKUP_PROMOTION_LEN: usize = 32;
pub(super) const COLLECTION_STRING_LOOKUP_PROMOTION_LEN: usize = 8;
pub(super) const PROPERTY_FEEDBACK_HOT_SITE_THRESHOLD: u32 = 8;
pub(super) const PROPERTY_FEEDBACK_PATCH_WARM_THRESHOLD: u32 = 8;
pub(super) const PROPERTY_FEEDBACK_PATCH_MAX_INVALIDATIONS: u32 = 2;
pub(super) const BUILTIN_FEEDBACK_HOT_SITE_THRESHOLD: u32 = 8;

const STRING_LOOKUP_OVERRIDE_UNSET: u8 = 0;
const STRING_LOOKUP_OVERRIDE_DISABLED: u8 = 1;
const STRING_LOOKUP_OVERRIDE_ENABLED: u8 = 2;
const PROPERTY_PATCH_OVERRIDE_UNSET: u8 = 0;
const PROPERTY_PATCH_OVERRIDE_DISABLED: u8 = 1;
const PROPERTY_PATCH_OVERRIDE_ENABLED: u8 = 2;

static STRING_LOOKUP_OVERRIDE: AtomicU8 = AtomicU8::new(STRING_LOOKUP_OVERRIDE_UNSET);
static PROPERTY_PATCH_OVERRIDE: AtomicU8 = AtomicU8::new(PROPERTY_PATCH_OVERRIDE_UNSET);
#[cfg(test)]
static STRING_LOOKUP_TEST_LOCK: Mutex<()> = Mutex::new(());
#[cfg(test)]
static PROPERTY_PATCH_TEST_LOCK: Mutex<()> = Mutex::new(());

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
pub(super) enum CollectionNumberKey {
    Finite(u64),
    Nan,
}

impl CollectionNumberKey {
    fn from_f64(value: f64) -> Self {
        if value.is_nan() {
            Self::Nan
        } else if value == 0.0 {
            Self::Finite(0.0f64.to_bits())
        } else {
            Self::Finite(value.to_bits())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum CollectionIndexKey {
    Undefined,
    Null,
    Bool(bool),
    Number(CollectionNumberKey),
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

impl CollectionIndexKey {
    pub(super) fn from_value(value: &Value) -> Self {
        match value {
            Value::Undefined => Self::Undefined,
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(*value),
            Value::Number(value) => Self::Number(CollectionNumberKey::from_f64(*value)),
            Value::String(value) => Self::String(value.clone()),
            Value::Object(value) => Self::Object(*value),
            Value::Array(value) => Self::Array(*value),
            Value::Map(value) => Self::Map(*value),
            Value::Set(value) => Self::Set(*value),
            Value::Iterator(value) => Self::Iterator(*value),
            Value::Closure(value) => Self::Closure(*value),
            Value::Promise(value) => Self::Promise(*value),
            Value::BuiltinFunction(value) => Self::BuiltinFunction(*value),
            Value::HostFunction(value) => Self::HostFunction(value.clone()),
            Value::BigInt(value) => Self::BigInt(value.clone()),
        }
    }
}

pub(super) fn uses_string_heavy_collection_lookup(value: &Value) -> bool {
    matches!(value, Value::String(_) | Value::HostFunction(_))
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

pub(super) fn string_heavy_collection_lookup_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();

    match STRING_LOOKUP_OVERRIDE.load(Ordering::Relaxed) {
        STRING_LOOKUP_OVERRIDE_DISABLED => false,
        STRING_LOOKUP_OVERRIDE_ENABLED => true,
        _ => *ENABLED
            .get_or_init(|| env_flag_enabled("MUSTARD_ENABLE_STRING_HEAVY_COLLECTION_LOOKUP")),
    }
}

pub(super) fn feedback_property_patching_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();

    match PROPERTY_PATCH_OVERRIDE.load(Ordering::Relaxed) {
        PROPERTY_PATCH_OVERRIDE_DISABLED => false,
        PROPERTY_PATCH_OVERRIDE_ENABLED => true,
        _ => *ENABLED.get_or_init(|| env_flag_enabled("MUSTARD_ENABLE_FEEDBACK_PROPERTY_PATCHING")),
    }
}

#[cfg(test)]
pub(super) struct StringHeavyCollectionLookupOverrideGuard {
    previous: u8,
    _lock: MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for StringHeavyCollectionLookupOverrideGuard {
    fn drop(&mut self) {
        STRING_LOOKUP_OVERRIDE.store(self.previous, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(super) fn override_string_heavy_collection_lookup_for_tests(
    enabled: bool,
) -> StringHeavyCollectionLookupOverrideGuard {
    let lock = STRING_LOOKUP_TEST_LOCK
        .lock()
        .expect("string-heavy lookup test override lock should not be poisoned");
    let next = if enabled {
        STRING_LOOKUP_OVERRIDE_ENABLED
    } else {
        STRING_LOOKUP_OVERRIDE_DISABLED
    };
    let previous = STRING_LOOKUP_OVERRIDE.swap(next, Ordering::Relaxed);
    StringHeavyCollectionLookupOverrideGuard {
        previous,
        _lock: lock,
    }
}

#[cfg(test)]
pub(super) struct FeedbackPropertyPatchingOverrideGuard {
    previous: u8,
    _lock: MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for FeedbackPropertyPatchingOverrideGuard {
    fn drop(&mut self) {
        PROPERTY_PATCH_OVERRIDE.store(self.previous, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(super) fn override_feedback_property_patching_for_tests(
    enabled: bool,
) -> FeedbackPropertyPatchingOverrideGuard {
    let lock = PROPERTY_PATCH_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let next = if enabled {
        PROPERTY_PATCH_OVERRIDE_ENABLED
    } else {
        PROPERTY_PATCH_OVERRIDE_DISABLED
    };
    let previous = PROPERTY_PATCH_OVERRIDE.swap(next, Ordering::Relaxed);
    FeedbackPropertyPatchingOverrideGuard {
        previous,
        _lock: lock,
    }
}

impl Hash for CollectionIndexKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Undefined => 0u8.hash(state),
            Self::Null => 1u8.hash(state),
            Self::Bool(value) => {
                2u8.hash(state);
                value.hash(state);
            }
            Self::Number(value) => {
                3u8.hash(state);
                value.hash(state);
            }
            Self::String(value) => {
                4u8.hash(state);
                value.hash(state);
            }
            Self::Object(value) => {
                5u8.hash(state);
                value.hash(state);
            }
            Self::Array(value) => {
                6u8.hash(state);
                value.hash(state);
            }
            Self::Map(value) => {
                7u8.hash(state);
                value.hash(state);
            }
            Self::Set(value) => {
                8u8.hash(state);
                value.hash(state);
            }
            Self::Iterator(value) => {
                9u8.hash(state);
                value.hash(state);
            }
            Self::Closure(value) => {
                10u8.hash(state);
                value.hash(state);
            }
            Self::Promise(value) => {
                11u8.hash(state);
                value.hash(state);
            }
            Self::BuiltinFunction(value) => {
                12u8.hash(state);
                value.hash(state);
            }
            Self::HostFunction(value) => {
                13u8.hash(state);
                value.hash(state);
            }
            Self::BigInt(value) => {
                14u8.hash(state);
                value.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum CollectionLookupKey<'a> {
    Undefined,
    Null,
    Bool(bool),
    Number(CollectionNumberKey),
    String(&'a str),
    Object(ObjectKey),
    Array(ArrayKey),
    Map(MapKey),
    Set(SetKey),
    Iterator(IteratorKey),
    Closure(ClosureKey),
    Promise(PromiseKey),
    BuiltinFunction(BuiltinFunction),
    HostFunction(&'a str),
    BigInt(&'a BigInt),
}

impl<'a> CollectionLookupKey<'a> {
    pub(super) fn from_value(value: &'a Value) -> Self {
        match value {
            Value::Undefined => Self::Undefined,
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(*value),
            Value::Number(value) => Self::Number(CollectionNumberKey::from_f64(*value)),
            Value::String(value) => Self::String(value),
            Value::Object(value) => Self::Object(*value),
            Value::Array(value) => Self::Array(*value),
            Value::Map(value) => Self::Map(*value),
            Value::Set(value) => Self::Set(*value),
            Value::Iterator(value) => Self::Iterator(*value),
            Value::Closure(value) => Self::Closure(*value),
            Value::Promise(value) => Self::Promise(*value),
            Value::BuiltinFunction(value) => Self::BuiltinFunction(*value),
            Value::HostFunction(value) => Self::HostFunction(value),
            Value::BigInt(value) => Self::BigInt(value),
        }
    }
}

impl Hash for CollectionLookupKey<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Undefined => 0u8.hash(state),
            Self::Null => 1u8.hash(state),
            Self::Bool(value) => {
                2u8.hash(state);
                value.hash(state);
            }
            Self::Number(value) => {
                3u8.hash(state);
                value.hash(state);
            }
            Self::String(value) => {
                4u8.hash(state);
                value.hash(state);
            }
            Self::Object(value) => {
                5u8.hash(state);
                value.hash(state);
            }
            Self::Array(value) => {
                6u8.hash(state);
                value.hash(state);
            }
            Self::Map(value) => {
                7u8.hash(state);
                value.hash(state);
            }
            Self::Set(value) => {
                8u8.hash(state);
                value.hash(state);
            }
            Self::Iterator(value) => {
                9u8.hash(state);
                value.hash(state);
            }
            Self::Closure(value) => {
                10u8.hash(state);
                value.hash(state);
            }
            Self::Promise(value) => {
                11u8.hash(state);
                value.hash(state);
            }
            Self::BuiltinFunction(value) => {
                12u8.hash(state);
                value.hash(state);
            }
            Self::HostFunction(value) => {
                13u8.hash(state);
                value.hash(state);
            }
            Self::BigInt(value) => {
                14u8.hash(state);
                value.hash(state);
            }
        }
    }
}

impl Equivalent<CollectionIndexKey> for CollectionLookupKey<'_> {
    fn equivalent(&self, key: &CollectionIndexKey) -> bool {
        match (self, key) {
            (Self::Undefined, CollectionIndexKey::Undefined)
            | (Self::Null, CollectionIndexKey::Null) => true,
            (Self::Bool(left), CollectionIndexKey::Bool(right)) => left == right,
            (Self::Number(left), CollectionIndexKey::Number(right)) => left == right,
            (Self::String(left), CollectionIndexKey::String(right)) => *left == right,
            (Self::Object(left), CollectionIndexKey::Object(right)) => left == right,
            (Self::Array(left), CollectionIndexKey::Array(right)) => left == right,
            (Self::Map(left), CollectionIndexKey::Map(right)) => left == right,
            (Self::Set(left), CollectionIndexKey::Set(right)) => left == right,
            (Self::Iterator(left), CollectionIndexKey::Iterator(right)) => left == right,
            (Self::Closure(left), CollectionIndexKey::Closure(right)) => left == right,
            (Self::Promise(left), CollectionIndexKey::Promise(right)) => left == right,
            (Self::BuiltinFunction(left), CollectionIndexKey::BuiltinFunction(right)) => {
                left == right
            }
            (Self::HostFunction(left), CollectionIndexKey::HostFunction(right)) => *left == right,
            (Self::BigInt(left), CollectionIndexKey::BigInt(right)) => *left == right,
            _ => false,
        }
    }
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
pub(super) struct SharedObjectShape {
    pub(super) id: u64,
    pub(super) property_slots: IndexMap<String, usize>,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

impl SharedObjectShape {
    pub(super) fn from_keys(id: u64, keys: Vec<String>) -> Self {
        Self {
            id,
            property_slots: keys
                .into_iter()
                .enumerate()
                .map(|(slot, key)| (key, slot))
                .collect(),
            accounted_bytes: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ShapedObjectProperties {
    pub(super) shape: Arc<SharedObjectShape>,
    pub(super) slots: Vec<Value>,
}

impl ShapedObjectProperties {
    fn get(&self, key: &str) -> Option<&Value> {
        self.shape
            .property_slots
            .get(key)
            .and_then(|slot| self.slots.get(*slot))
    }

    fn into_plain(self) -> IndexMap<String, Value> {
        self.shape
            .property_slots
            .iter()
            .map(|(key, slot)| {
                (
                    key.clone(),
                    self.slots.get(*slot).cloned().unwrap_or(Value::Undefined),
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum ObjectProperties {
    Plain(IndexMap<String, Value>),
    Shaped(ShapedObjectProperties),
}

pub(super) enum ObjectPropertyValues<'a> {
    Plain(indexmap::map::Values<'a, String, Value>),
    Shaped(std::slice::Iter<'a, Value>),
}

impl<'a> Iterator for ObjectPropertyValues<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Plain(iter) => iter.next(),
            Self::Shaped(iter) => iter.next(),
        }
    }
}

pub(super) enum ObjectPropertyKeys<'a> {
    Plain(indexmap::map::Keys<'a, String, Value>),
    Shaped(indexmap::map::Keys<'a, String, usize>),
}

impl<'a> Iterator for ObjectPropertyKeys<'a> {
    type Item = &'a String;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Plain(iter) => iter.next(),
            Self::Shaped(iter) => iter.next(),
        }
    }
}

pub(super) enum ObjectPropertyIter<'a> {
    Plain(indexmap::map::Iter<'a, String, Value>),
    Shaped {
        shape_iter: indexmap::map::Iter<'a, String, usize>,
        slots: &'a [Value],
    },
}

impl<'a> Iterator for ObjectPropertyIter<'a> {
    type Item = (&'a String, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Plain(iter) => iter.next(),
            Self::Shaped { shape_iter, slots } => {
                let (key, slot) = shape_iter.next()?;
                slots.get(*slot).map(|value| (key, value))
            }
        }
    }
}

impl ObjectProperties {
    pub(super) fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Plain(properties) => properties.get(key),
            Self::Shaped(properties) => properties.get(key),
        }
    }

    pub(super) fn contains_key(&self, key: &str) -> bool {
        match self {
            Self::Plain(properties) => properties.contains_key(key),
            Self::Shaped(properties) => properties.shape.property_slots.contains_key(key),
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::Plain(properties) => properties.len(),
            Self::Shaped(properties) => properties.slots.len(),
        }
    }

    pub(super) fn is_shaped(&self) -> bool {
        matches!(self, Self::Shaped(_))
    }

    pub(super) fn values(&self) -> ObjectPropertyValues<'_> {
        match self {
            Self::Plain(properties) => ObjectPropertyValues::Plain(properties.values()),
            Self::Shaped(properties) => ObjectPropertyValues::Shaped(properties.slots.iter()),
        }
    }

    pub(super) fn keys(&self) -> ObjectPropertyKeys<'_> {
        match self {
            Self::Plain(properties) => ObjectPropertyKeys::Plain(properties.keys()),
            Self::Shaped(properties) => {
                ObjectPropertyKeys::Shaped(properties.shape.property_slots.keys())
            }
        }
    }

    pub(super) fn iter(&self) -> ObjectPropertyIter<'_> {
        match self {
            Self::Plain(properties) => ObjectPropertyIter::Plain(properties.iter()),
            Self::Shaped(properties) => ObjectPropertyIter::Shaped {
                shape_iter: properties.shape.property_slots.iter(),
                slots: &properties.slots,
            },
        }
    }

    pub(super) fn ordered_keys(&self) -> Vec<String> {
        self.ordered_keys_filtered(|_, _| true)
    }

    pub(super) fn ordered_keys_filtered<F>(&self, mut include: F) -> Vec<String>
    where
        F: FnMut(&str, &Value) -> bool,
    {
        let mut keys = Vec::with_capacity(self.len());
        let mut index_keys = self
            .iter()
            .filter(|(key, value)| include(key, value))
            .filter_map(|(key, _)| {
                array_index_from_property_key(key).map(|index| (index, key.clone()))
            })
            .collect::<Vec<_>>();
        index_keys.sort_unstable_by_key(|(index, _)| *index);
        keys.extend(index_keys.into_iter().map(|(_, key)| key));
        keys.extend(
            self.iter()
                .filter(|(key, value)| {
                    include(key, value) && array_index_from_property_key(key).is_none()
                })
                .map(|(key, _)| key.clone()),
        );
        keys
    }

    pub(super) fn materialize(&mut self) -> &mut IndexMap<String, Value> {
        if matches!(self, Self::Shaped(_)) {
            let previous = std::mem::replace(self, Self::Plain(IndexMap::new()));
            *self = match previous {
                Self::Plain(properties) => Self::Plain(properties),
                Self::Shaped(properties) => Self::Plain(properties.into_plain()),
            };
        }

        match self {
            Self::Plain(properties) => properties,
            Self::Shaped(_) => unreachable!("materialized object properties should be plain"),
        }
    }

    pub(super) fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        self.materialize().insert(key, value)
    }
}

impl<'a> IntoIterator for &'a ObjectProperties {
    type Item = (&'a String, &'a Value);
    type IntoIter = ObjectPropertyIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PlainObject {
    pub(super) properties: ObjectProperties,
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
    pub(super) entries: Vec<Option<MapEntry>>,
    #[serde(default)]
    pub(super) live_len: usize,
    #[serde(skip, default)]
    pub(super) string_key_live_len: usize,
    #[serde(default)]
    pub(super) clear_epoch: u64,
    #[serde(skip, default)]
    pub(super) lookup: IndexMap<CollectionIndexKey, usize>,
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
    pub(super) entries: Vec<Option<Value>>,
    #[serde(default)]
    pub(super) live_len: usize,
    #[serde(skip, default)]
    pub(super) string_key_live_len: usize,
    #[serde(default)]
    pub(super) clear_epoch: u64,
    #[serde(skip, default)]
    pub(super) lookup: IndexMap<CollectionIndexKey, usize>,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IteratorObject {
    pub(super) state: IteratorState,
    #[serde(skip, default)]
    pub(super) accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ArrayIteratorState {
    pub(super) array: ArrayKey,
    pub(super) next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct StringIteratorState {
    pub(super) value: String,
    pub(super) next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct MapIteratorState {
    pub(super) map: MapKey,
    pub(super) next_index: usize,
    #[serde(default)]
    pub(super) observed_clear_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SetIteratorState {
    pub(super) set: SetKey,
    pub(super) next_index: usize,
    #[serde(default)]
    pub(super) observed_clear_epoch: u64,
}

impl MapObject {
    pub(super) fn lookup_promotion_len(&self) -> usize {
        if string_heavy_collection_lookup_enabled()
            && self.live_len > 0
            && self.string_key_live_len == self.live_len
        {
            COLLECTION_STRING_LOOKUP_PROMOTION_LEN
        } else {
            COLLECTION_LOOKUP_PROMOTION_LEN
        }
    }

    pub(super) fn from_entries(entries: Vec<MapEntry>) -> Self {
        let mut map = Self {
            entries: entries.into_iter().map(Some).collect(),
            live_len: 0,
            string_key_live_len: 0,
            clear_epoch: 0,
            lookup: IndexMap::new(),
            accounted_bytes: 0,
        };
        map.rebuild_lookup();
        map
    }

    pub(super) fn rebuild_lookup(&mut self) {
        self.lookup.clear();
        self.live_len = 0;
        self.string_key_live_len = 0;
        for (index, entry) in self.entries.iter().enumerate() {
            let Some(entry) = entry else {
                continue;
            };
            self.live_len += 1;
            if uses_string_heavy_collection_lookup(&entry.key) {
                self.string_key_live_len += 1;
            }
            if self.live_len >= self.lookup_promotion_len() {
                self.lookup
                    .insert(CollectionIndexKey::from_value(&entry.key), index);
            }
        }
        if self.live_len < self.lookup_promotion_len() {
            self.lookup.clear();
        } else if self.lookup.len() < self.live_len {
            self.lookup.clear();
            for (index, entry) in self.entries.iter().enumerate() {
                let Some(entry) = entry else {
                    continue;
                };
                self.lookup
                    .insert(CollectionIndexKey::from_value(&entry.key), index);
            }
        }
    }
}

impl SetObject {
    pub(super) fn lookup_promotion_len(&self) -> usize {
        if string_heavy_collection_lookup_enabled()
            && self.live_len > 0
            && self.string_key_live_len == self.live_len
        {
            COLLECTION_STRING_LOOKUP_PROMOTION_LEN
        } else {
            COLLECTION_LOOKUP_PROMOTION_LEN
        }
    }

    pub(super) fn from_entries(entries: Vec<Value>) -> Self {
        let mut set = Self {
            entries: entries.into_iter().map(Some).collect(),
            live_len: 0,
            string_key_live_len: 0,
            clear_epoch: 0,
            lookup: IndexMap::new(),
            accounted_bytes: 0,
        };
        set.rebuild_lookup();
        set
    }

    pub(super) fn rebuild_lookup(&mut self) {
        self.lookup.clear();
        self.live_len = 0;
        self.string_key_live_len = 0;
        for (index, value) in self.entries.iter().enumerate() {
            let Some(value) = value else {
                continue;
            };
            self.live_len += 1;
            if uses_string_heavy_collection_lookup(value) {
                self.string_key_live_len += 1;
            }
            if self.live_len >= self.lookup_promotion_len() {
                self.lookup
                    .insert(CollectionIndexKey::from_value(value), index);
            }
        }
        if self.live_len < self.lookup_promotion_len() {
            self.lookup.clear();
        } else if self.lookup.len() < self.live_len {
            self.lookup.clear();
            for (index, value) in self.entries.iter().enumerate() {
                let Some(value) = value else {
                    continue;
                };
                self.lookup
                    .insert(CollectionIndexKey::from_value(value), index);
            }
        }
    }
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
pub(super) enum PromiseCombinatorInput {
    Promise(PromiseKey),
    Fulfilled(Value),
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
        source: PromiseKey,
    },
    PromiseReaction {
        reaction: PromiseReaction,
        source: PromiseKey,
    },
    PromiseCombinator {
        target: PromiseKey,
        index: usize,
        kind: PromiseCombinatorKind,
        input: PromiseCombinatorInput,
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
    #[serde(skip, default)]
    pub(super) callback_capture: bool,
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

#[derive(Debug, Clone, Default)]
pub(super) enum GetPropStaticInlineCache {
    #[default]
    Uninitialized,
    Monomorphic {
        shape_id: u64,
        slot: Option<usize>,
    },
    Polymorphic(Vec<GetPropStaticInlineCacheEntry>),
    Megamorphic,
}

#[derive(Debug, Clone)]
pub(super) struct GetPropStaticInlineCacheEntry {
    pub(super) shape_id: u64,
    pub(super) slot: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PropertyPatch {
    pub(super) shape_id: u64,
    pub(super) slot: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct PropertyFeedbackSite {
    pub(super) last_observation: Option<(u64, Option<usize>)>,
    pub(super) stable_hits: u32,
    pub(super) hot: bool,
    pub(super) ever_patched: bool,
    pub(super) patched: Option<PropertyPatch>,
    pub(super) invalidations: u32,
    pub(super) disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BuiltinFeedbackKind {
    Collection,
    String,
}

#[derive(Debug, Clone)]
pub(super) struct BuiltinFeedbackSite {
    pub(super) kind: BuiltinFeedbackKind,
    pub(super) builtin: BuiltinFunction,
    pub(super) hits: u32,
    pub(super) hot: bool,
    pub(super) polymorphic: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) struct CollectionCallSiteKey {
    pub(super) function_id: usize,
    pub(super) instruction_offset: usize,
}

fn accounting_recount_required_after_deserialize() -> bool {
    true
}

fn default_next_object_shape_id() -> u64 {
    1
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
    #[serde(skip, default)]
    pub(super) object_shapes: HashMap<Vec<String>, Arc<SharedObjectShape>>,
    #[serde(skip, default = "default_next_object_shape_id")]
    pub(super) next_object_shape_id: u64,
    #[serde(skip, default)]
    pub(super) static_property_inline_caches: HashMap<(usize, usize), GetPropStaticInlineCache>,
    #[serde(skip, default)]
    pub(super) property_feedback_sites: HashMap<(usize, usize), PropertyFeedbackSite>,
    #[serde(skip, default)]
    pub(super) builtin_feedback_sites: HashMap<(usize, usize), BuiltinFeedbackSite>,
    #[serde(default)]
    pub(super) collection_call_sites: HashMap<CollectionCallSiteKey, CollectionCallSiteMetrics>,
    pub(super) snapshot_nonce: u64,
    pub(super) instruction_counter: usize,
    #[serde(skip, default)]
    pub(super) heap_bytes_used: usize,
    #[serde(skip, default)]
    pub(super) allocation_count: usize,
    #[serde(skip, default)]
    pub(super) gc_allocation_debt_bytes: usize,
    #[serde(skip, default)]
    pub(super) gc_allocation_debt_count: usize,
    #[serde(skip, default)]
    pub(super) debug_metrics: RuntimeDebugMetrics,
    #[serde(skip, default)]
    pub(super) operation_counters_enabled: bool,
    #[serde(skip, default = "accounting_recount_required_after_deserialize")]
    pub(super) accounting_recount_required: bool,
    #[serde(skip, default)]
    pub(super) cancellation_token: Option<CancellationToken>,
    #[serde(skip, default)]
    pub(super) regex_cache: HashMap<(String, String), Regex>,
    #[serde(skip, default)]
    pub(super) pending_internal_exception: Option<PromiseRejection>,
    #[serde(skip, default)]
    pub(super) pending_sync_callback_result: Option<Value>,
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
    pub(super) envs: SecondaryMap<EnvKey, ()>,
    pub(super) cells: SecondaryMap<CellKey, ()>,
    pub(super) objects: SecondaryMap<ObjectKey, ()>,
    pub(super) arrays: SecondaryMap<ArrayKey, ()>,
    pub(super) maps: SecondaryMap<MapKey, ()>,
    pub(super) sets: SecondaryMap<SetKey, ()>,
    pub(super) iterators: SecondaryMap<IteratorKey, ()>,
    pub(super) closures: SecondaryMap<ClosureKey, ()>,
    pub(super) promises: SecondaryMap<PromiseKey, ()>,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct GarbageCollectionStats {
    pub(super) reclaimed_bytes: usize,
    pub(super) reclaimed_allocations: usize,
}
