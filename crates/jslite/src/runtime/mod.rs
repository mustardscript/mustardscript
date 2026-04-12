mod accounting;
mod api;
mod async_runtime;
mod builtins;
mod bytecode;
mod compiler;
mod conversions;
mod env;
mod exceptions;
mod gc;
mod properties;
mod serialization;
mod state;
mod validation;
mod vm;

pub use api::{
    ExecutionOptions, ExecutionSnapshot, ExecutionStep, HostError, ResumeOptions, ResumePayload,
    SnapshotInspection, SnapshotPolicy, Suspension, execute, inspect_snapshot, resume,
    resume_with_options, start, start_bytecode,
};
pub use bytecode::{BytecodeProgram, FunctionPrototype, Instruction};
pub use compiler::lower_to_bytecode;
use compiler::pattern_bindings;
pub use serialization::{dump_program, dump_snapshot, load_program, load_snapshot};

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use indexmap::IndexMap;
use slotmap::SlotMap;

use self::properties::{format_number_key, property_name_to_key};
use self::state::*;
use crate::{
    cancellation::CancellationToken,
    diagnostic::{DiagnosticKind, JsliteError, JsliteResult, TraceFrame},
    ir::{BinaryOp, Pattern, PropertyName, UnaryOp},
    span::SourceSpan,
    structured::{StructuredNumber, StructuredValue},
};

const INTERNAL_CALLBACK_THROW_MARKER: &str = "\0internal-array-callback-throw";

struct CallbackCallOptions<'a> {
    non_callable_message: &'a str,
    host_suspension_message: &'a str,
    unsettled_message: &'a str,
    allow_host_suspension: bool,
    allow_pending_promise_result: bool,
}

impl Runtime {
    fn new(program: BytecodeProgram, options: ExecutionOptions) -> JsliteResult<Self> {
        let ExecutionOptions {
            inputs,
            capabilities,
            limits,
            cancellation_token,
        } = options;
        let mut envs = SlotMap::with_key();
        let globals = envs.insert(Env {
            parent: None,
            bindings: IndexMap::new(),
            accounted_bytes: 0,
        });
        let mut runtime = Self {
            program,
            limits,
            globals,
            envs,
            cells: SlotMap::with_key(),
            objects: SlotMap::with_key(),
            arrays: SlotMap::with_key(),
            maps: SlotMap::with_key(),
            sets: SlotMap::with_key(),
            iterators: SlotMap::with_key(),
            closures: SlotMap::with_key(),
            promises: SlotMap::with_key(),
            frames: Vec::new(),
            root_result: None,
            microtasks: VecDeque::new(),
            pending_host_calls: VecDeque::new(),
            suspended_host_call: None,
            snapshot_nonce: next_snapshot_nonce(),
            instruction_counter: 0,
            heap_bytes_used: 0,
            allocation_count: 0,
            cancellation_token,
            pending_internal_exception: None,
            snapshot_policy_required: false,
            pending_resume_behavior: ResumeBehavior::Value,
        };
        runtime.account_existing_env(globals)?;
        runtime.install_builtins()?;
        for capability in capabilities {
            runtime.define_global(capability.clone(), Value::HostFunction(capability), false)?;
        }
        for (name, value) in inputs {
            let value = runtime.value_from_structured(value)?;
            runtime.define_global(name, value, true)?;
        }
        Ok(runtime)
    }

    fn apply_resume_options(&mut self, options: ResumeOptions) -> JsliteResult<()> {
        if options.cancellation_token.is_some() {
            self.cancellation_token = options.cancellation_token;
        }
        if let Some(policy) = options.snapshot_policy {
            self.apply_snapshot_policy(policy)?;
        }
        if self.snapshot_policy_required {
            return Err(serialization_error(
                "loaded snapshots require explicit host policy before resume",
            ));
        }
        Ok(())
    }

    fn apply_snapshot_policy(&mut self, policy: SnapshotPolicy) -> JsliteResult<()> {
        validation::validate_snapshot_policy(self, &policy)?;
        self.limits = policy.limits;
        if self.frames.len() > self.limits.call_depth_limit {
            return Err(limit_error("call depth limit exceeded"));
        }
        self.recompute_accounting_after_load()?;
        let outstanding_host_calls =
            self.pending_host_calls.len() + usize::from(self.suspended_host_call.is_some());
        if outstanding_host_calls > self.limits.max_outstanding_host_calls {
            return Err(limit_error("outstanding host-call limit exhausted"));
        }
        if self.instruction_counter > self.limits.instruction_budget {
            return Err(limit_error("instruction budget exhausted"));
        }
        self.snapshot_policy_required = false;
        Ok(())
    }

    fn check_cancellation(&self) -> JsliteResult<()> {
        if self
            .cancellation_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(limit_error("execution cancelled"));
        }
        Ok(())
    }

    pub(crate) fn with_temporary_roots<T, F>(&mut self, roots: &[Value], f: F) -> JsliteResult<T>
    where
        F: FnOnce(&mut Self) -> JsliteResult<T>,
    {
        let frame_index =
            self.frames.len().checked_sub(1).ok_or_else(|| {
                JsliteError::runtime("no active frame available for temporary roots")
            })?;
        let original_len = self.frames[frame_index].stack.len();
        self.frames[frame_index].stack.extend(roots.iter().cloned());
        let result = f(self);
        if let Some(frame) = self.frames.get_mut(frame_index) {
            frame.stack.truncate(original_len);
        }
        result
    }

    pub(crate) fn call_callback(
        &mut self,
        callback: Value,
        this_arg: Value,
        args: &[Value],
        options: CallbackCallOptions<'_>,
    ) -> JsliteResult<Value> {
        match callback {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("closure not found"))?;
                let env = self.new_env(Some(closure.env))?;
                let had_async_boundary = self.current_async_boundary_index().is_some();
                let (is_async, function_id) = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .map(|function| (function.is_async, closure.function_id))
                    .ok_or_else(|| JsliteError::runtime("function not found"))?;
                if is_async {
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    self.push_frame(function_id, env, args, this_arg, Some(promise))?;
                    Ok(Value::Promise(promise))
                } else {
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    let base_depth = self.frames.len();
                    self.push_frame(function_id, env, args, this_arg, Some(promise))?;
                    self.run_until_frame_depth(base_depth, options.host_suspension_message)?;
                    match self.promise_outcome(promise)? {
                        Some(PromiseOutcome::Fulfilled(value)) => Ok(value),
                        Some(PromiseOutcome::Rejected(rejection)) => {
                            self.pending_internal_exception = Some(rejection);
                            Err(JsliteError::runtime(INTERNAL_CALLBACK_THROW_MARKER))
                        }
                        None if options.allow_pending_promise_result && had_async_boundary => {
                            Ok(Value::Promise(promise))
                        }
                        None => Err(JsliteError::runtime(options.unsettled_message)),
                    }
                }
            }
            Value::BuiltinFunction(function) => self.call_builtin(function, this_arg, args),
            Value::HostFunction(capability) => {
                if !options.allow_host_suspension || self.current_async_boundary_index().is_none() {
                    return Err(JsliteError::runtime(options.host_suspension_message));
                }
                let outstanding =
                    self.pending_host_calls.len() + usize::from(self.suspended_host_call.is_some());
                if outstanding >= self.limits.max_outstanding_host_calls {
                    return Err(limit_error("outstanding host-call limit exhausted"));
                }
                let args = args
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<JsliteResult<Vec<_>>>()?;
                let promise = self.insert_promise(PromiseState::Pending)?;
                self.pending_host_calls.push_back(PendingHostCall {
                    capability,
                    args,
                    promise: Some(promise),
                    resume_behavior: ResumeBehavior::Value,
                    traceback: self.traceback_snapshots(),
                });
                Ok(Value::Promise(promise))
            }
            _ => Err(JsliteError::runtime(options.non_callable_message)),
        }
    }

    fn bump_instruction_budget(&mut self) -> JsliteResult<()> {
        self.instruction_counter += 1;
        if self.instruction_counter > self.limits.instruction_budget {
            return Err(limit_error("instruction budget exhausted"));
        }
        Ok(())
    }
}

fn limit_error(message: impl Into<String>) -> JsliteError {
    JsliteError::Message {
        kind: DiagnosticKind::Limit,
        message: message.into(),
        span: None,
        traceback: Vec::new(),
    }
}

fn serialization_error(message: impl Into<String>) -> JsliteError {
    JsliteError::Message {
        kind: DiagnosticKind::Serialization,
        message: message.into(),
        span: None,
        traceback: Vec::new(),
    }
}

fn next_snapshot_nonce() -> u64 {
    static NEXT_SNAPSHOT_NONCE: AtomicU64 = AtomicU64::new(1);
    NEXT_SNAPSHOT_NONCE.fetch_add(1, Ordering::Relaxed)
}

fn pop_many(stack: &mut Vec<Value>, count: usize) -> JsliteResult<Vec<Value>> {
    if stack.len() < count {
        return Err(JsliteError::runtime("stack underflow"));
    }
    let start = stack.len() - count;
    Ok(stack.drain(start..).collect())
}

fn resume_behavior_for_capability(capability: &str) -> ResumeBehavior {
    match capability {
        "console.log" | "console.warn" | "console.error" => ResumeBehavior::Undefined,
        _ => ResumeBehavior::Value,
    }
}

fn is_truthy(value: &Value) -> bool {
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

fn is_callable(value: &Value) -> bool {
    matches!(
        value,
        Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_)
    )
}

fn strict_equal(left: &Value, right: &Value) -> bool {
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
        _ => false,
    }
}

fn same_value_zero(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            (left == right) || (left.is_nan() && right.is_nan())
        }
        (Value::BigInt(left), Value::BigInt(right)) => left == right,
        _ => strict_equal(left, right),
    }
}
