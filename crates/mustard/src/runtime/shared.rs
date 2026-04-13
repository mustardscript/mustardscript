//! Shared runtime helpers used across sibling runtime modules.

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

pub(super) struct CallbackCallOptions<'a> {
    pub(super) non_callable_message: &'a str,
    pub(super) host_suspension_message: &'a str,
    pub(super) unsettled_message: &'a str,
    pub(super) allow_host_suspension: bool,
    pub(super) allow_pending_promise_result: bool,
}

pub(super) fn limit_error(message: impl Into<String>) -> MustardError {
    MustardError::Message {
        kind: DiagnosticKind::Limit,
        message: message.into(),
        span: None,
        traceback: Vec::new(),
    }
}

pub(super) fn serialization_error(message: impl Into<String>) -> MustardError {
    MustardError::Message {
        kind: DiagnosticKind::Serialization,
        message: message.into(),
        span: None,
        traceback: Vec::new(),
    }
}

pub(super) fn next_snapshot_nonce() -> u64 {
    static NEXT_SNAPSHOT_NONCE: AtomicU64 = AtomicU64::new(1);
    NEXT_SNAPSHOT_NONCE.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn pop_many(stack: &mut Vec<Value>, count: usize) -> MustardResult<Vec<Value>> {
    if stack.len() < count {
        return Err(MustardError::runtime("stack underflow"));
    }
    let start = stack.len() - count;
    Ok(stack.drain(start..).collect())
}

pub(super) fn resume_behavior_for_capability(capability: &str) -> ResumeBehavior {
    match capability {
        "console.log" | "console.warn" | "console.error" => ResumeBehavior::Undefined,
        _ => ResumeBehavior::Value,
    }
}

pub(super) fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => *value != 0.0 && !value.is_nan(),
        Value::BigInt(value) => value != &0.into(),
        Value::String(value) => !value.is_empty(),
        Value::Object(_)
        | Value::Array(_)
        | Value::Map(_)
        | Value::Set(_)
        | Value::Iterator(_)
        | Value::Promise(_)
        | Value::Closure(_)
        | Value::BuiltinFunction(_)
        | Value::HostFunction(_) => true,
    }
}

pub(super) fn is_callable(value: &Value) -> bool {
    matches!(
        value,
        Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_)
    )
}

pub(super) fn strict_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Undefined, Value::Undefined) => true,
        (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::BigInt(left), Value::BigInt(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Object(left), Value::Object(right)) => left == right,
        (Value::Array(left), Value::Array(right)) => left == right,
        (Value::Map(left), Value::Map(right)) => left == right,
        (Value::Set(left), Value::Set(right)) => left == right,
        (Value::Iterator(left), Value::Iterator(right)) => left == right,
        (Value::Promise(left), Value::Promise(right)) => left == right,
        (Value::Closure(left), Value::Closure(right)) => left == right,
        (Value::BuiltinFunction(left), Value::BuiltinFunction(right)) => left == right,
        (Value::HostFunction(left), Value::HostFunction(right)) => left == right,
        _ => false,
    }
}

pub(super) fn same_value_zero(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            (left == right) || (left.is_nan() && right.is_nan())
        }
        (Value::BigInt(left), Value::BigInt(right)) => left == right,
        _ => strict_equal(left, right),
    }
}
