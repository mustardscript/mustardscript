mod accounting;
mod api;
mod bytecode;
mod compiler;
mod conversions;
mod env;
mod gc;
mod properties;
mod serialization;
mod state;
mod validation;

pub use api::{
    ExecutionOptions, ExecutionSnapshot, ExecutionStep, HostError, ResumeOptions, ResumePayload,
    Suspension, execute, resume, resume_with_options, start, start_bytecode,
};
pub use bytecode::{BytecodeProgram, FunctionPrototype, Instruction};
pub use compiler::lower_to_bytecode;
use compiler::pattern_bindings;
pub use serialization::{dump_program, dump_snapshot, load_program, load_snapshot};

use std::cmp::Ordering;
use std::collections::{HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use indexmap::IndexMap;
use regress::Regex;
use slotmap::SlotMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use self::state::*;
use self::{
    conversions::structured_to_json,
    properties::{canonicalize_collection_key, format_number_key, property_name_to_key},
};
use crate::{
    cancellation::CancellationToken,
    diagnostic::{DiagnosticKind, JsliteError, JsliteResult, TraceFrame},
    ir::{BinaryOp, Pattern, PropertyName, UnaryOp},
    span::SourceSpan,
    structured::{StructuredNumber, StructuredValue},
};

const INTERNAL_CALLBACK_THROW_MARKER: &str = "\0internal-array-callback-throw";

#[derive(Debug, Clone, Copy)]
struct RegExpFlagsState {
    global: bool,
    ignore_case: bool,
    multiline: bool,
    dot_all: bool,
    unicode: bool,
    sticky: bool,
}

#[derive(Debug, Clone)]
struct RegExpMatchData {
    start_byte: usize,
    end_byte: usize,
    start_index: usize,
    end_index: usize,
    captures: Vec<Option<String>>,
    named_groups: IndexMap<String, Option<String>>,
}

#[derive(Debug, Clone)]
enum StringSearchPattern {
    Literal(String),
    RegExp {
        object: ObjectKey,
        regex: RegExpObject,
    },
}

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
            instruction_counter: 0,
            heap_bytes_used: 0,
            allocation_count: 0,
            cancellation_token,
            pending_internal_exception: None,
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

    fn apply_resume_options(&mut self, options: ResumeOptions) {
        if options.cancellation_token.is_some() {
            self.cancellation_token = options.cancellation_token;
        }
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

    fn run_root(&mut self) -> JsliteResult<ExecutionStep> {
        self.check_cancellation()?;
        self.collect_garbage()?;
        self.check_call_depth()?;
        let root_env = self.new_env(Some(self.globals))?;
        self.push_frame(self.program.root, root_env, &[], Value::Undefined, None)?;
        self.run()
    }

    fn traceback_frames(&self) -> Vec<TraceFrame> {
        self.frames
            .iter()
            .rev()
            .filter_map(|frame| {
                self.program
                    .functions
                    .get(frame.function_id)
                    .map(|function| TraceFrame {
                        function_name: function.name.clone(),
                        span: function.span,
                    })
            })
            .collect()
    }

    fn annotate_runtime_error(&self, error: JsliteError) -> JsliteError {
        error.with_traceback(self.traceback_frames())
    }

    fn traceback_snapshots(&self) -> Vec<TraceFrameSnapshot> {
        self.traceback_frames()
            .into_iter()
            .map(|frame| TraceFrameSnapshot {
                function_name: frame.function_name,
                span: frame.span,
            })
            .collect()
    }

    fn compose_traceback(&self, origin: &[TraceFrameSnapshot]) -> Vec<TraceFrame> {
        let mut frames = self.traceback_frames();
        for frame in origin {
            let candidate = TraceFrame {
                function_name: frame.function_name.clone(),
                span: frame.span,
            };
            if !frames.iter().any(|existing| {
                existing.function_name == candidate.function_name && existing.span == candidate.span
            }) {
                frames.push(candidate);
            }
        }
        frames
    }

    fn current_async_boundary_index(&self) -> Option<usize> {
        self.frames
            .iter()
            .rposition(|frame| frame.async_promise.is_some())
    }

    fn promise_outcome(&self, promise: PromiseKey) -> JsliteResult<Option<PromiseOutcome>> {
        let promise = self
            .promises
            .get(promise)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?;
        Ok(match &promise.state {
            PromiseState::Pending => None,
            PromiseState::Fulfilled(value) => Some(PromiseOutcome::Fulfilled(value.clone())),
            PromiseState::Rejected(rejection) => Some(PromiseOutcome::Rejected(rejection.clone())),
        })
    }

    fn coerce_to_promise(&mut self, value: Value) -> JsliteResult<PromiseKey> {
        match value {
            Value::Promise(promise) => Ok(promise),
            other => self.insert_promise(PromiseState::Fulfilled(other)),
        }
    }

    fn attach_awaiter(
        &mut self,
        promise: PromiseKey,
        continuation: AsyncContinuation,
    ) -> JsliteResult<()> {
        self.promises
            .get_mut(promise)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .awaiters
            .push(continuation);
        self.refresh_promise_accounting(promise)
    }

    fn attach_dependent(&mut self, promise: PromiseKey, dependent: PromiseKey) -> JsliteResult<()> {
        self.promises
            .get_mut(promise)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .dependents
            .push(dependent);
        self.refresh_promise_accounting(promise)
    }

    fn attach_promise_reaction(
        &mut self,
        promise: PromiseKey,
        reaction: PromiseReaction,
    ) -> JsliteResult<()> {
        match self.promise_outcome(promise)? {
            Some(outcome) => self.schedule_promise_reaction(reaction, outcome),
            None => {
                self.promises
                    .get_mut(promise)
                    .ok_or_else(|| JsliteError::runtime("promise missing"))?
                    .reactions
                    .push(reaction);
                self.refresh_promise_accounting(promise)
            }
        }
    }

    fn schedule_promise_reaction(
        &mut self,
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        self.microtasks
            .push_back(MicrotaskJob::PromiseReaction { reaction, outcome });
        Ok(())
    }

    fn settle_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        let (awaiters, dependents, reactions) = {
            let promise_ref = self
                .promises
                .get_mut(promise)
                .ok_or_else(|| JsliteError::runtime("promise missing"))?;
            if !matches!(promise_ref.state, PromiseState::Pending) {
                return Ok(());
            }
            promise_ref.state = match &outcome {
                PromiseOutcome::Fulfilled(value) => PromiseState::Fulfilled(value.clone()),
                PromiseOutcome::Rejected(rejection) => PromiseState::Rejected(rejection.clone()),
            };
            promise_ref.driver = None;
            (
                std::mem::take(&mut promise_ref.awaiters),
                std::mem::take(&mut promise_ref.dependents),
                std::mem::take(&mut promise_ref.reactions),
            )
        };
        self.refresh_promise_accounting(promise)?;
        for continuation in awaiters {
            self.microtasks.push_back(MicrotaskJob::ResumeAsync {
                continuation,
                outcome: outcome.clone(),
            });
        }
        for dependent in dependents {
            self.resolve_promise_with_outcome(dependent, outcome.clone())?;
        }
        for reaction in reactions {
            self.schedule_promise_reaction(reaction, outcome.clone())?;
        }
        Ok(())
    }

    fn resolve_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        match outcome {
            PromiseOutcome::Fulfilled(value) => self.resolve_promise(promise, value),
            PromiseOutcome::Rejected(rejection) => self.reject_promise(promise, rejection),
        }
    }

    fn resolve_promise(&mut self, promise: PromiseKey, value: Value) -> JsliteResult<()> {
        if let Value::Promise(source) = value {
            if source == promise {
                let error_value =
                    self.value_from_runtime_message("TypeError: promise cannot resolve to itself")?;
                return self.reject_promise(
                    promise,
                    PromiseRejection {
                        value: error_value,
                        span: None,
                        traceback: self.traceback_snapshots(),
                    },
                );
            }
            match self.promise_outcome(source)? {
                Some(outcome) => self.resolve_promise_with_outcome(promise, outcome),
                None => self.attach_dependent(source, promise),
            }
        } else {
            self.settle_promise_with_outcome(promise, PromiseOutcome::Fulfilled(value))
        }
    }

    fn reject_promise(
        &mut self,
        promise: PromiseKey,
        rejection: PromiseRejection,
    ) -> JsliteResult<()> {
        self.settle_promise_with_outcome(promise, PromiseOutcome::Rejected(rejection))
    }

    fn suspend_async_await(&mut self, value: Value) -> JsliteResult<()> {
        let boundary = self.current_async_boundary_index().ok_or_else(|| {
            JsliteError::runtime("await is only supported inside async functions")
        })?;
        let promise = self.coerce_to_promise(value)?;
        let continuation = AsyncContinuation {
            frames: self.frames.split_off(boundary),
        };
        match self.promise_outcome(promise)? {
            Some(outcome) => self.microtasks.push_back(MicrotaskJob::ResumeAsync {
                continuation,
                outcome,
            }),
            None => self.attach_awaiter(promise, continuation)?,
        }
        Ok(())
    }

    fn promise_reaction_target(&self, reaction: &PromiseReaction) -> PromiseKey {
        match reaction {
            PromiseReaction::Then { target, .. }
            | PromiseReaction::Finally { target, .. }
            | PromiseReaction::FinallyPassThrough { target, .. }
            | PromiseReaction::Combinator { target, .. } => *target,
        }
    }

    fn runtime_error_to_promise_rejection(
        &mut self,
        error: JsliteError,
    ) -> JsliteResult<PromiseRejection> {
        match error {
            JsliteError::Message {
                kind: DiagnosticKind::Runtime,
                message,
                span,
                traceback,
            } => Ok(PromiseRejection {
                value: self.value_from_runtime_message(&message)?,
                span,
                traceback: if traceback.is_empty() {
                    self.traceback_snapshots()
                } else {
                    traceback
                        .into_iter()
                        .map(|frame| TraceFrameSnapshot {
                            function_name: frame.function_name,
                            span: frame.span,
                        })
                        .collect()
                },
            }),
            other => Err(other),
        }
    }

    fn reject_promise_from_error(
        &mut self,
        target: PromiseKey,
        error: JsliteError,
    ) -> JsliteResult<()> {
        let rejection = self.runtime_error_to_promise_rejection(error)?;
        self.reject_promise(target, rejection)
    }

    fn invoke_promise_handler(
        &mut self,
        handler: Value,
        args: &[Value],
        target: PromiseKey,
    ) -> JsliteResult<()> {
        match handler {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("closure not found"))?;
                let env = self.new_env(Some(closure.env))?;
                let (is_async, function_id) = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .map(|function| (function.is_async, closure.function_id))
                    .ok_or_else(|| JsliteError::runtime("function not found"))?;
                if is_async {
                    let bridge = self.insert_promise(PromiseState::Pending)?;
                    self.attach_dependent(bridge, target)?;
                    self.push_frame(function_id, env, args, Value::Undefined, Some(bridge))?;
                } else {
                    self.push_frame(function_id, env, args, Value::Undefined, Some(target))?;
                }
                Ok(())
            }
            Value::BuiltinFunction(function) => {
                let value = self.call_builtin(function, Value::Undefined, args)?;
                self.resolve_promise(target, value)
            }
            Value::HostFunction(capability) => {
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
                self.attach_dependent(promise, target)?;
                self.pending_host_calls.push_back(PendingHostCall {
                    capability,
                    args,
                    promise,
                    resume_behavior: ResumeBehavior::Value,
                    traceback: self.traceback_snapshots(),
                });
                Ok(())
            }
            _ => Err(JsliteError::runtime("value is not callable")),
        }
    }

    fn make_promise_all_settled_result(
        &mut self,
        result: PromiseSettledResult,
    ) -> JsliteResult<Value> {
        let properties = match result {
            PromiseSettledResult::Fulfilled(value) => IndexMap::from([
                ("status".to_string(), Value::String("fulfilled".to_string())),
                ("value".to_string(), value),
            ]),
            PromiseSettledResult::Rejected(reason) => IndexMap::from([
                ("status".to_string(), Value::String("rejected".to_string())),
                ("reason".to_string(), reason),
            ]),
        };
        Ok(Value::Object(
            self.insert_object(properties, ObjectKind::Plain)?,
        ))
    }

    fn make_aggregate_error(&mut self, reasons: Vec<Value>) -> JsliteResult<Value> {
        let error = self.make_error_object(
            "AggregateError",
            &[Value::String("All promises were rejected".to_string())],
            None,
            None,
        )?;
        let errors = Value::Array(self.insert_array(reasons, IndexMap::new())?);
        self.set_property(error.clone(), Value::String("errors".to_string()), errors)?;
        Ok(error)
    }

    fn activate_promise_combinator(
        &mut self,
        target: PromiseKey,
        index: usize,
        kind: PromiseCombinatorKind,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        if self.promise_outcome(target)?.is_some() {
            return Ok(());
        }
        match kind {
            PromiseCombinatorKind::Race => self.resolve_promise_with_outcome(target, outcome),
            PromiseCombinatorKind::All => {
                let mut resolved_values = None;
                let mut rejection = None;
                {
                    let promise = self
                        .promises
                        .get_mut(target)
                        .ok_or_else(|| JsliteError::runtime("promise missing"))?;
                    let PromiseState::Pending = promise.state else {
                        return Ok(());
                    };
                    let PromiseDriver::All { remaining, values } = promise
                        .driver
                        .as_mut()
                        .ok_or_else(|| JsliteError::runtime("promise combinator state missing"))?
                    else {
                        return Err(JsliteError::runtime("promise combinator kind mismatch"));
                    };
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => {
                            values[index] = Some(value);
                            *remaining = remaining.saturating_sub(1);
                            if *remaining == 0 {
                                resolved_values = Some(
                                    values
                                        .iter()
                                        .map(|value| value.clone().unwrap_or(Value::Undefined))
                                        .collect::<Vec<_>>(),
                                );
                            }
                        }
                        PromiseOutcome::Rejected(reason) => rejection = Some(reason),
                    }
                }
                self.refresh_promise_accounting(target)?;
                if let Some(rejection) = rejection {
                    self.reject_promise(target, rejection)?;
                } else if let Some(values) = resolved_values {
                    let array = Value::Array(self.insert_array(values, IndexMap::new())?);
                    self.resolve_promise(target, array)?;
                }
                Ok(())
            }
            PromiseCombinatorKind::AllSettled => {
                let mut settled_results = None;
                {
                    let promise = self
                        .promises
                        .get_mut(target)
                        .ok_or_else(|| JsliteError::runtime("promise missing"))?;
                    let PromiseState::Pending = promise.state else {
                        return Ok(());
                    };
                    let PromiseDriver::AllSettled { remaining, results } = promise
                        .driver
                        .as_mut()
                        .ok_or_else(|| JsliteError::runtime("promise combinator state missing"))?
                    else {
                        return Err(JsliteError::runtime("promise combinator kind mismatch"));
                    };
                    results[index] = Some(match outcome {
                        PromiseOutcome::Fulfilled(value) => PromiseSettledResult::Fulfilled(value),
                        PromiseOutcome::Rejected(reason) => {
                            PromiseSettledResult::Rejected(reason.value)
                        }
                    });
                    *remaining = remaining.saturating_sub(1);
                    if *remaining == 0 {
                        settled_results = Some(
                            results
                                .iter()
                                .map(|result| {
                                    result.clone().unwrap_or(PromiseSettledResult::Fulfilled(
                                        Value::Undefined,
                                    ))
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                self.refresh_promise_accounting(target)?;
                if let Some(results) = settled_results {
                    let mut values = Vec::with_capacity(results.len());
                    for result in results {
                        values.push(self.make_promise_all_settled_result(result)?);
                    }
                    let array = Value::Array(self.insert_array(values, IndexMap::new())?);
                    self.resolve_promise(target, array)?;
                }
                Ok(())
            }
            PromiseCombinatorKind::Any => {
                let mut rejection_values = None;
                let mut fulfillment = None;
                {
                    let promise = self
                        .promises
                        .get_mut(target)
                        .ok_or_else(|| JsliteError::runtime("promise missing"))?;
                    let PromiseState::Pending = promise.state else {
                        return Ok(());
                    };
                    let PromiseDriver::Any { remaining, reasons } = promise
                        .driver
                        .as_mut()
                        .ok_or_else(|| JsliteError::runtime("promise combinator state missing"))?
                    else {
                        return Err(JsliteError::runtime("promise combinator kind mismatch"));
                    };
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => fulfillment = Some(value),
                        PromiseOutcome::Rejected(reason) => {
                            reasons[index] = Some(reason.value);
                            *remaining = remaining.saturating_sub(1);
                            if *remaining == 0 {
                                rejection_values = Some(
                                    reasons
                                        .iter()
                                        .map(|value| value.clone().unwrap_or(Value::Undefined))
                                        .collect::<Vec<_>>(),
                                );
                            }
                        }
                    }
                }
                self.refresh_promise_accounting(target)?;
                if let Some(value) = fulfillment {
                    self.resolve_promise(target, value)?;
                } else if let Some(reasons) = rejection_values {
                    let rejection = PromiseRejection {
                        value: self.make_aggregate_error(reasons)?,
                        span: None,
                        traceback: self.traceback_snapshots(),
                    };
                    self.reject_promise(target, rejection)?;
                }
                Ok(())
            }
        }
    }

    fn activate_promise_reaction(
        &mut self,
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        let target = self.promise_reaction_target(&reaction);
        let result = (|| match reaction {
            PromiseReaction::Then {
                target,
                on_fulfilled,
                on_rejected,
            } => match outcome {
                PromiseOutcome::Fulfilled(value) => {
                    if let Some(handler) = on_fulfilled {
                        self.invoke_promise_handler(handler, &[value], target)
                    } else {
                        self.resolve_promise(target, value)
                    }
                }
                PromiseOutcome::Rejected(rejection) => {
                    if let Some(handler) = on_rejected {
                        self.invoke_promise_handler(handler, &[rejection.value], target)
                    } else {
                        self.reject_promise(target, rejection)
                    }
                }
            },
            PromiseReaction::Finally { target, callback } => {
                if let Some(callback) = callback {
                    let bridge = self.insert_promise(PromiseState::Pending)?;
                    self.attach_promise_reaction(
                        bridge,
                        PromiseReaction::FinallyPassThrough {
                            target,
                            original_outcome: outcome,
                        },
                    )?;
                    self.invoke_promise_handler(callback, &[], bridge)
                } else {
                    self.resolve_promise_with_outcome(target, outcome)
                }
            }
            PromiseReaction::FinallyPassThrough {
                target,
                original_outcome,
            } => match outcome {
                PromiseOutcome::Fulfilled(_) => {
                    self.resolve_promise_with_outcome(target, original_outcome)
                }
                PromiseOutcome::Rejected(rejection) => self.reject_promise(target, rejection),
            },
            PromiseReaction::Combinator {
                target,
                index,
                kind,
            } => self.activate_promise_combinator(target, index, kind, outcome),
        })();

        match result {
            Ok(()) => Ok(()),
            Err(error) => self.reject_promise_from_error(target, error),
        }
    }

    fn activate_microtask(&mut self, job: MicrotaskJob) -> JsliteResult<()> {
        if !self.frames.is_empty() {
            return Err(JsliteError::runtime(
                "microtask checkpoint ran while frames were still active",
            ));
        }
        match job {
            MicrotaskJob::ResumeAsync {
                continuation,
                outcome,
            } => {
                self.frames = continuation.frames;
                match outcome {
                    PromiseOutcome::Fulfilled(value) => {
                        let frame = self.frames.last_mut().ok_or_else(|| {
                            JsliteError::runtime("async continuation resumed without frames")
                        })?;
                        frame.stack.push(value);
                    }
                    PromiseOutcome::Rejected(rejection) => {
                        match self.raise_exception_with_origin(
                            rejection.value,
                            rejection.span,
                            Some(rejection.traceback),
                        )? {
                            StepAction::Continue => {}
                            StepAction::Return(_) => {}
                        }
                    }
                }
            }
            MicrotaskJob::PromiseReaction { reaction, outcome } => {
                self.activate_promise_reaction(reaction, outcome)?;
            }
        }
        Ok(())
    }

    fn has_pending_async_work(&self) -> bool {
        self.suspended_host_call.is_some()
            || !self.pending_host_calls.is_empty()
            || !self.microtasks.is_empty()
            || self.promises.values().any(|promise| {
                matches!(promise.state, PromiseState::Pending)
                    && (!promise.awaiters.is_empty()
                        || !promise.dependents.is_empty()
                        || !promise.reactions.is_empty())
            })
    }

    fn root_error_from_rejection(&self, rejection: PromiseRejection) -> JsliteResult<JsliteError> {
        Ok(JsliteError::Message {
            kind: DiagnosticKind::Runtime,
            message: self.render_exception(&rejection.value)?,
            span: rejection.span,
            traceback: self.compose_traceback(&rejection.traceback),
        })
    }

    fn suspend_host_request(&mut self, request: PendingHostCall) -> ExecutionStep {
        let capability = request.capability.clone();
        let args = request.args.clone();
        self.suspended_host_call = Some(request);
        ExecutionStep::Suspended(Box::new(Suspension {
            capability,
            args,
            snapshot: ExecutionSnapshot {
                runtime: self.clone(),
            },
        }))
    }

    fn process_idle_state(&mut self) -> JsliteResult<Option<ExecutionStep>> {
        if let Some(job) = self.microtasks.pop_front() {
            self.activate_microtask(job)?;
            return Ok(None);
        }
        if let Some(request) = self.pending_host_calls.pop_front() {
            return Ok(Some(self.suspend_host_request(request)));
        }
        if let Some(root_result) = self.root_result.clone() {
            return match root_result {
                Value::Promise(promise) => match self.promise_outcome(promise)? {
                    Some(PromiseOutcome::Fulfilled(value)) => Ok(Some(ExecutionStep::Completed(
                        self.value_to_structured(value)?,
                    ))),
                    Some(PromiseOutcome::Rejected(rejection)) => {
                        Err(self.root_error_from_rejection(rejection)?)
                    }
                    None => Err(JsliteError::runtime(
                        "async root promise could not make progress",
                    )),
                },
                value => {
                    if self.has_pending_async_work() {
                        return Err(JsliteError::runtime(
                            "async execution became idle with pending work",
                        ));
                    }
                    Ok(Some(ExecutionStep::Completed(
                        self.value_to_structured(value)?,
                    )))
                }
            };
        }
        if self.has_pending_async_work() {
            return Err(JsliteError::runtime(
                "async execution became idle before producing a root result",
            ));
        }
        Err(JsliteError::runtime("vm lost all frames"))
    }

    fn resume(&mut self, payload: ResumePayload) -> JsliteResult<ExecutionStep> {
        if let Err(error) = self.check_cancellation() {
            if let Some(request) = self.suspended_host_call.as_ref() {
                return Err(error.with_traceback(self.compose_traceback(&request.traceback)));
            }
            return Err(self.annotate_runtime_error(error));
        }
        self.collect_garbage()
            .map_err(|error| self.annotate_runtime_error(error))?;
        if let Some(request) = self.suspended_host_call.take() {
            let outcome = match payload {
                ResumePayload::Value(value) => {
                    let value = match request.resume_behavior {
                        ResumeBehavior::Value => self
                            .value_from_structured(value)
                            .map_err(|error| self.annotate_runtime_error(error))?,
                        ResumeBehavior::Undefined => Value::Undefined,
                    };
                    PromiseOutcome::Fulfilled(value)
                }
                ResumePayload::Error(error) => PromiseOutcome::Rejected(PromiseRejection {
                    value: self
                        .value_from_host_error(error)
                        .map_err(|error| self.annotate_runtime_error(error))?,
                    span: None,
                    traceback: Vec::new(),
                }),
                ResumePayload::Cancelled => {
                    return Err(limit_error("execution cancelled")
                        .with_traceback(self.compose_traceback(&request.traceback)));
                }
            };
            self.resolve_promise_with_outcome(request.promise, outcome)
                .map_err(|error| self.annotate_runtime_error(error))?;
            return self.run();
        }
        match payload {
            ResumePayload::Value(value) => {
                let value = match self.pending_resume_behavior {
                    ResumeBehavior::Value => self
                        .value_from_structured(value)
                        .map_err(|error| self.annotate_runtime_error(error))?,
                    ResumeBehavior::Undefined => Value::Undefined,
                };
                self.pending_resume_behavior = ResumeBehavior::Value;
                let Some(frame) = self.frames.last_mut() else {
                    return Err(self.annotate_runtime_error(JsliteError::runtime(
                        "no suspended frame available",
                    )));
                };
                frame.stack.push(value);
            }
            ResumePayload::Error(error) => {
                self.pending_resume_behavior = ResumeBehavior::Value;
                let value = self
                    .value_from_host_error(error)
                    .map_err(|error| self.annotate_runtime_error(error))?;
                match self.raise_exception(value, None) {
                    Ok(StepAction::Continue) => return self.run(),
                    Ok(StepAction::Return(step)) => return Ok(step),
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                }
            }
            ResumePayload::Cancelled => {
                return Err(self.annotate_runtime_error(limit_error("execution cancelled")));
            }
        }
        self.run()
    }

    fn step_active_frame(&mut self) -> JsliteResult<StepAction> {
        let frame_index = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| JsliteError::runtime("vm lost all frames"))?;
        let function_id = self.frames[frame_index].function_id;
        let ip = self.frames[frame_index].ip;
        let instruction = self
            .program
            .functions
            .get(function_id)
            .and_then(|function| function.code.get(ip))
            .cloned()
            .ok_or_else(|| JsliteError::runtime("instruction pointer out of range"))?;
        self.frames[frame_index].ip += 1;
        self.bump_instruction_budget()?;
        self.collect_garbage_before_instruction(&instruction)?;
        match instruction {
            Instruction::PushUndefined => {
                self.frames[frame_index].stack.push(Value::Undefined);
            }
            Instruction::PushNull => self.frames[frame_index].stack.push(Value::Null),
            Instruction::PushBool(value) => self.frames[frame_index].stack.push(Value::Bool(value)),
            Instruction::PushNumber(value) => {
                self.frames[frame_index].stack.push(Value::Number(value))
            }
            Instruction::PushString(value) => {
                self.frames[frame_index].stack.push(Value::String(value))
            }
            Instruction::PushRegExp { pattern, flags } => {
                let value = self.make_regexp_value(pattern, flags)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::LoadName(name) => {
                let env = self.frames[frame_index].env;
                let value = self.lookup_name(env, &name)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::StoreName(name) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let env = self.frames[frame_index].env;
                self.assign_name(env, &name, value.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::InitializePattern(pattern) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let env = self.frames[frame_index].env;
                self.initialize_pattern(env, &pattern, value)?;
            }
            Instruction::PushEnv => {
                let current_env = self.frames[frame_index].env;
                let env = self.new_env(Some(current_env))?;
                self.frames[frame_index].scope_stack.push(current_env);
                self.frames[frame_index].env = env;
            }
            Instruction::PopEnv => {
                let restored = self.frames[frame_index]
                    .scope_stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("scope stack underflow"))?;
                self.frames[frame_index].env = restored;
            }
            Instruction::DeclareName { name, mutable } => {
                let env = self.frames[frame_index].env;
                self.declare_name(env, name, mutable)?;
            }
            Instruction::MakeClosure { function_id } => {
                let env = self.frames[frame_index].env;
                let closure = self.insert_closure(function_id, env)?;
                self.frames[frame_index].stack.push(Value::Closure(closure));
            }
            Instruction::MakeArray { count } => {
                let values = pop_many(&mut self.frames[frame_index].stack, count)?;
                let array = self.insert_array(values, IndexMap::new())?;
                self.frames[frame_index].stack.push(Value::Array(array));
            }
            Instruction::MakeObject { keys } => {
                let values = pop_many(&mut self.frames[frame_index].stack, keys.len())?;
                let mut properties = IndexMap::new();
                for (key, value) in keys.into_iter().zip(values.into_iter()) {
                    properties.insert(property_name_to_key(&key), value);
                }
                let object = self.insert_object(properties, ObjectKind::Plain)?;
                self.frames[frame_index].stack.push(Value::Object(object));
            }
            Instruction::CreateIterator => {
                let iterable = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let iterator = self.create_iterator(iterable)?;
                self.frames[frame_index].stack.push(iterator);
            }
            Instruction::IteratorNext => {
                let iterator = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let (value, done) = self.iterator_next(iterator)?;
                self.frames[frame_index].stack.push(value);
                self.frames[frame_index].stack.push(Value::Bool(done));
            }
            Instruction::GetPropStatic { name, optional } => {
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let value = self.get_property(object, Value::String(name), optional)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::GetPropComputed { optional } => {
                let property = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let value = self.get_property(object, property, optional)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::SetPropStatic { name } => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                self.set_property(object, Value::String(name), value.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::SetPropComputed => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let property = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                self.set_property(object, property, value.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Unary(operator) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let result = self.apply_unary(operator, value)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::Binary(operator) => {
                let right = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let left = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let result = self.apply_binary(operator, left, right)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::Pop => {
                self.frames[frame_index].stack.pop();
            }
            Instruction::Dup => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Dup2 => {
                let len = self.frames[frame_index].stack.len();
                if len < 2 {
                    return Err(JsliteError::runtime("stack underflow"));
                }
                let a = self.frames[frame_index].stack[len - 2].clone();
                let b = self.frames[frame_index].stack[len - 1].clone();
                self.frames[frame_index].stack.push(a);
                self.frames[frame_index].stack.push(b);
            }
            Instruction::PushHandler { catch, finally } => {
                let frame = &mut self.frames[frame_index];
                frame.handlers.push(ExceptionHandler {
                    catch,
                    finally,
                    env: frame.env,
                    scope_stack_len: frame.scope_stack.len(),
                    stack_len: frame.stack.len(),
                });
            }
            Instruction::PopHandler => {
                self.frames[frame_index]
                    .handlers
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("handler stack underflow"))?;
            }
            Instruction::EnterFinally { exit } => {
                let completion_index = self.frames[frame_index]
                    .pending_completions
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| JsliteError::runtime("missing pending completion"))?;
                self.frames[frame_index]
                    .active_finally
                    .push(ActiveFinallyState {
                        completion_index,
                        exit,
                    });
            }
            Instruction::BeginCatch => {
                let value = self.frames[frame_index]
                    .pending_exception
                    .take()
                    .ok_or_else(|| JsliteError::runtime("missing pending exception"))?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Throw { span } => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                return self.raise_exception(value, Some(span));
            }
            Instruction::PushPendingJump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => {
                self.store_completion(
                    frame_index,
                    CompletionRecord::Jump {
                        target,
                        target_handler_depth,
                        target_scope_depth,
                    },
                )?;
            }
            Instruction::PushPendingReturn => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                self.store_completion(frame_index, CompletionRecord::Return(value))?;
            }
            Instruction::PushPendingThrow => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                self.store_completion(frame_index, CompletionRecord::Throw(value))?;
            }
            Instruction::ContinuePending => {
                let marker = self.frames[frame_index]
                    .active_finally
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("missing active finally state"))?;
                if marker.completion_index >= self.frames[frame_index].pending_completions.len() {
                    return Err(JsliteError::runtime(
                        "active finally references missing completion",
                    ));
                }
                let completion = self.frames[frame_index]
                    .pending_completions
                    .remove(marker.completion_index);
                return self.resume_completion(completion);
            }
            Instruction::Jump(target) => self.frames[frame_index].ip = target,
            Instruction::JumpIfFalse(target) => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                if !is_truthy(&value) {
                    self.frames[frame_index].ip = target;
                }
            }
            Instruction::JumpIfTrue(target) => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                if is_truthy(&value) {
                    self.frames[frame_index].ip = target;
                }
            }
            Instruction::JumpIfNullish(target) => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                if matches!(value, Value::Null | Value::Undefined) {
                    self.frames[frame_index].ip = target;
                }
            }
            Instruction::Call {
                argc,
                with_this,
                optional,
            } => {
                let args = pop_many(&mut self.frames[frame_index].stack, argc)?;
                let callee = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let this_value = if with_this {
                    self.frames[frame_index]
                        .stack
                        .pop()
                        .ok_or_else(|| JsliteError::runtime("stack underflow"))?
                } else {
                    Value::Undefined
                };
                if optional && matches!(callee, Value::Undefined | Value::Null) {
                    self.frames[frame_index].stack.push(Value::Undefined);
                    return Ok(StepAction::Continue);
                }
                match self.call_callable(callee, this_value, &args)? {
                    RunState::Completed(value) => {
                        self.frames[frame_index].stack.push(value);
                    }
                    RunState::PushedFrame => {}
                    RunState::StartedAsync(value) => {
                        self.frames[frame_index].stack.push(value);
                    }
                    RunState::Suspended {
                        capability,
                        args,
                        resume_behavior,
                    } => {
                        self.pending_resume_behavior = resume_behavior;
                        return Ok(StepAction::Return(ExecutionStep::Suspended(Box::new(
                            Suspension {
                                capability,
                                args,
                                snapshot: ExecutionSnapshot {
                                    runtime: self.clone(),
                                },
                            },
                        ))));
                    }
                }
            }
            Instruction::Await => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                self.suspend_async_await(value)?;
            }
            Instruction::Construct { argc } => {
                let args = pop_many(&mut self.frames[frame_index].stack, argc)?;
                let callee = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                let value = self.construct(callee, &args)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Return => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .unwrap_or(Value::Undefined);
                return self.complete_return(value);
            }
        }
        Ok(StepAction::Continue)
    }

    fn run(&mut self) -> JsliteResult<ExecutionStep> {
        loop {
            self.check_cancellation()
                .map_err(|error| self.annotate_runtime_error(error))?;
            if self.frames.is_empty() {
                match self.process_idle_state() {
                    Ok(Some(step)) => return Ok(step),
                    Ok(None) => continue,
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                }
            }
            let action = match self.step_active_frame() {
                Ok(action) => action,
                Err(error) => match self.handle_runtime_fault(error) {
                    Ok(action) => action,
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                },
            };

            match action {
                StepAction::Continue => {}
                StepAction::Return(step) => return Ok(step),
            }
        }
    }

    fn run_until_frame_depth(&mut self, target_depth: usize) -> JsliteResult<()> {
        while self.frames.len() > target_depth {
            self.check_cancellation()?;
            let action = match self.step_active_frame() {
                Ok(action) => action,
                Err(error) => self.handle_runtime_fault(error)?,
            };
            match action {
                StepAction::Continue => {}
                StepAction::Return(ExecutionStep::Suspended(_)) => {
                    return Err(JsliteError::runtime(
                        "TypeError: array callback helpers do not support synchronous host suspensions",
                    ));
                }
                StepAction::Return(ExecutionStep::Completed(_)) => {
                    return Err(JsliteError::runtime(
                        "nested callback execution unexpectedly completed the program",
                    ));
                }
            }
        }
        Ok(())
    }

    fn with_temporary_roots<T, F>(&mut self, roots: &[Value], f: F) -> JsliteResult<T>
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

    fn handle_runtime_fault(&mut self, error: JsliteError) -> JsliteResult<StepAction> {
        match error {
            JsliteError::Message {
                kind: DiagnosticKind::Runtime,
                message,
                span,
                ..
            } => {
                if message == INTERNAL_CALLBACK_THROW_MARKER {
                    let rejection = self.pending_internal_exception.take().ok_or_else(|| {
                        JsliteError::runtime("missing internal callback exception state")
                    })?;
                    return self.raise_exception_with_origin(
                        rejection.value,
                        rejection.span,
                        Some(rejection.traceback),
                    );
                }
                let value = self.value_from_runtime_message(&message)?;
                self.raise_exception(value, span)
            }
            other => Err(other),
        }
    }

    fn store_completion(
        &mut self,
        frame_index: usize,
        completion: CompletionRecord,
    ) -> JsliteResult<()> {
        let completion_index = self.frames[frame_index]
            .active_finally
            .last()
            .map(|active| active.completion_index);
        if let Some(completion_index) = completion_index {
            if completion_index >= self.frames[frame_index].pending_completions.len() {
                return Err(JsliteError::runtime(
                    "active finally references missing completion",
                ));
            }
            self.frames[frame_index].pending_completions[completion_index] = completion;
        } else {
            self.frames[frame_index]
                .pending_completions
                .push(completion);
        }
        Ok(())
    }

    fn restore_handler_state(
        &mut self,
        frame_index: usize,
        handler: &ExceptionHandler,
    ) -> JsliteResult<()> {
        let frame = &mut self.frames[frame_index];
        frame.env = handler.env;
        frame.scope_stack.truncate(handler.scope_stack_len);
        frame.stack.truncate(handler.stack_len);
        Ok(())
    }

    fn raise_exception(
        &mut self,
        value: Value,
        span: Option<SourceSpan>,
    ) -> JsliteResult<StepAction> {
        self.raise_exception_with_origin(value, span, None)
    }

    fn raise_exception_with_origin(
        &mut self,
        value: Value,
        span: Option<SourceSpan>,
        origin_traceback: Option<Vec<TraceFrameSnapshot>>,
    ) -> JsliteResult<StepAction> {
        let traceback = match origin_traceback.as_ref() {
            Some(origin) => self.compose_traceback(origin),
            None => self.traceback_frames(),
        };
        let thrown = value;

        loop {
            let Some(frame_index) = self.frames.len().checked_sub(1) else {
                return Err(JsliteError::runtime("vm lost all frames"));
            };

            if let Some(handler_index) = self.frames[frame_index]
                .handlers
                .iter()
                .rposition(|handler| handler.catch.is_some() || handler.finally.is_some())
            {
                let handler = self.frames[frame_index].handlers[handler_index].clone();
                self.frames[frame_index].handlers.truncate(handler_index);
                self.restore_handler_state(frame_index, &handler)?;

                if let Some(catch_ip) = handler.catch {
                    self.frames[frame_index].pending_exception = Some(thrown);
                    self.frames[frame_index].ip = catch_ip;
                    return Ok(StepAction::Continue);
                }

                if let Some(finally_ip) = handler.finally {
                    self.frames[frame_index]
                        .pending_completions
                        .push(CompletionRecord::Throw(thrown));
                    self.frames[frame_index].ip = finally_ip;
                    return Ok(StepAction::Continue);
                }
            }

            if let Some(active) = self.frames[frame_index].active_finally.last().cloned() {
                if active.completion_index >= self.frames[frame_index].pending_completions.len() {
                    return Err(JsliteError::runtime(
                        "active finally references missing completion",
                    ));
                }
                self.frames[frame_index].pending_completions[active.completion_index] =
                    CompletionRecord::Throw(thrown);
                self.frames[frame_index].ip = active.exit;
                return Ok(StepAction::Continue);
            }

            if let Some(async_promise) = self.frames[frame_index].async_promise {
                self.frames.pop();
                self.reject_promise(
                    async_promise,
                    PromiseRejection {
                        value: thrown,
                        span,
                        traceback: traceback
                            .iter()
                            .map(|frame| TraceFrameSnapshot {
                                function_name: frame.function_name.clone(),
                                span: frame.span,
                            })
                            .collect(),
                    },
                )?;
                return Ok(StepAction::Continue);
            }

            if self.frames.len() == 1 {
                let message = self.render_exception(&thrown)?;
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Runtime,
                    message,
                    span,
                    traceback,
                });
            }

            self.frames.pop();
        }
    }

    fn resume_completion(&mut self, completion: CompletionRecord) -> JsliteResult<StepAction> {
        match completion {
            CompletionRecord::Throw(value) => self.raise_exception(value, None),
            CompletionRecord::Jump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => self.resume_nonthrow_completion(
                target_handler_depth,
                target_scope_depth,
                CompletionRecord::Jump {
                    target,
                    target_handler_depth,
                    target_scope_depth,
                },
            ),
            CompletionRecord::Return(value) => {
                self.resume_nonthrow_completion(0, 0, CompletionRecord::Return(value))
            }
        }
    }

    fn resume_nonthrow_completion(
        &mut self,
        target_handler_depth: usize,
        target_scope_depth: usize,
        completion: CompletionRecord,
    ) -> JsliteResult<StepAction> {
        let frame_index = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| JsliteError::runtime("vm lost all frames"))?;
        let current_depth = self.frames[frame_index].handlers.len();
        if target_handler_depth > current_depth {
            return Err(JsliteError::runtime(
                "completion targets missing handler depth",
            ));
        }
        if target_scope_depth > self.frames[frame_index].scope_stack.len() {
            return Err(JsliteError::runtime(
                "completion targets missing scope depth",
            ));
        }

        let restore_state = if target_handler_depth < current_depth {
            self.frames[frame_index]
                .handlers
                .get(target_handler_depth)
                .cloned()
        } else {
            None
        };

        if let Some(handler_index) = (target_handler_depth..current_depth)
            .rev()
            .find(|index| self.frames[frame_index].handlers[*index].finally.is_some())
        {
            let handler = self.frames[frame_index].handlers[handler_index].clone();
            self.frames[frame_index].handlers.truncate(handler_index);
            self.restore_handler_state(frame_index, &handler)?;
            self.frames[frame_index]
                .pending_completions
                .push(completion);
            self.frames[frame_index].ip = handler
                .finally
                .ok_or_else(|| JsliteError::runtime("missing finally target"))?;
            return Ok(StepAction::Continue);
        }

        if let Some(handler) = restore_state.as_ref() {
            self.restore_handler_state(frame_index, handler)?;
        }
        self.frames[frame_index]
            .handlers
            .truncate(target_handler_depth);

        match completion {
            CompletionRecord::Jump { target, .. } => {
                if self.frames[frame_index].scope_stack.len() < target_scope_depth {
                    return Err(JsliteError::runtime(
                        "completion targets missing scope depth",
                    ));
                }
                while self.frames[frame_index].scope_stack.len() > target_scope_depth {
                    let restored = self.frames[frame_index]
                        .scope_stack
                        .pop()
                        .ok_or_else(|| JsliteError::runtime("scope stack underflow"))?;
                    self.frames[frame_index].env = restored;
                }
                self.frames[frame_index].ip = target;
                Ok(StepAction::Continue)
            }
            CompletionRecord::Return(value) => self.complete_return(value),
            CompletionRecord::Throw(_) => unreachable!(),
        }
    }

    fn complete_return(&mut self, value: Value) -> JsliteResult<StepAction> {
        let frame = self
            .frames
            .pop()
            .ok_or_else(|| JsliteError::runtime("vm lost all frames"))?;
        if let Some(async_promise) = frame.async_promise {
            self.resolve_promise(async_promise, value)?;
            return Ok(StepAction::Continue);
        }
        if let Some(parent) = self.frames.last_mut() {
            parent.stack.push(value);
            Ok(StepAction::Continue)
        } else {
            self.root_result = Some(value);
            Ok(StepAction::Continue)
        }
    }

    fn check_call_depth(&self) -> JsliteResult<()> {
        if self.frames.len() >= self.limits.call_depth_limit {
            return Err(limit_error("call depth limit exceeded"));
        }
        Ok(())
    }

    fn push_frame(
        &mut self,
        function_id: usize,
        env: EnvKey,
        args: &[Value],
        this_value: Value,
        async_promise: Option<PromiseKey>,
    ) -> JsliteResult<()> {
        self.check_call_depth()?;
        let (params, rest) = self
            .program
            .functions
            .get(function_id)
            .map(|function| (function.params.clone(), function.rest.clone()))
            .ok_or_else(|| JsliteError::runtime("function not found"))?;
        let this_cell = self.insert_cell(this_value, true, true)?;
        self.envs
            .get_mut(env)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .bindings
            .insert("this".to_string(), this_cell);
        self.refresh_env_accounting(env)?;
        for pattern in &params {
            for (name, _) in pattern_bindings(pattern) {
                self.declare_name(env, name, true)?;
            }
        }
        for (index, pattern) in params.iter().enumerate() {
            let arg = args.get(index).cloned().unwrap_or(Value::Undefined);
            self.initialize_pattern(env, pattern, arg)?;
        }
        if let Some(rest) = &rest {
            for (name, _) in pattern_bindings(rest) {
                self.declare_name(env, name, true)?;
            }
            let rest_array = self.insert_array(
                args.iter().skip(params.len()).cloned().collect(),
                IndexMap::new(),
            )?;
            self.initialize_pattern(env, rest, Value::Array(rest_array))?;
        }
        self.frames.push(Frame {
            function_id,
            ip: 0,
            env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            pending_completions: Vec::new(),
            active_finally: Vec::new(),
            async_promise,
        });
        Ok(())
    }

    fn call_callable(
        &mut self,
        callee: Value,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<RunState> {
        match callee {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("closure not found"))?;
                let env = self.new_env(Some(closure.env))?;
                let (is_async, is_arrow) = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .map(|function| (function.is_async, function.is_arrow))
                    .ok_or_else(|| JsliteError::runtime("function not found"))?;
                let frame_this = if is_arrow {
                    Value::Undefined
                } else {
                    this_value
                };
                if is_async {
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    self.push_frame(closure.function_id, env, args, frame_this, Some(promise))?;
                    Ok(RunState::StartedAsync(Value::Promise(promise)))
                } else {
                    self.push_frame(closure.function_id, env, args, frame_this, None)?;
                    Ok(RunState::PushedFrame)
                }
            }
            Value::BuiltinFunction(function) => Ok(RunState::Completed(
                self.call_builtin(function, this_value, args)?,
            )),
            Value::HostFunction(capability) => {
                let resume_behavior = resume_behavior_for_capability(&capability);
                let args = args
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<JsliteResult<Vec<_>>>()?;
                if self.current_async_boundary_index().is_some() {
                    let outstanding = self.pending_host_calls.len()
                        + usize::from(self.suspended_host_call.is_some());
                    if outstanding >= self.limits.max_outstanding_host_calls {
                        return Err(limit_error("outstanding host-call limit exhausted"));
                    }
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    self.pending_host_calls.push_back(PendingHostCall {
                        capability,
                        args,
                        promise,
                        resume_behavior,
                        traceback: self.traceback_snapshots(),
                    });
                    Ok(RunState::Completed(Value::Promise(promise)))
                } else {
                    Ok(RunState::Suspended {
                        resume_behavior,
                        capability,
                        args,
                    })
                }
            }
            _ => Err(JsliteError::runtime("value is not callable")),
        }
    }

    fn construct(&mut self, callee: Value, args: &[Value]) -> JsliteResult<Value> {
        match callee {
            Value::BuiltinFunction(
                BuiltinFunction::ArrayCtor
                | BuiltinFunction::DateCtor
                | BuiltinFunction::ObjectCtor
                | BuiltinFunction::MapCtor
                | BuiltinFunction::SetCtor
                | BuiltinFunction::PromiseCtor
                | BuiltinFunction::RegExpCtor
                | BuiltinFunction::ErrorCtor
                | BuiltinFunction::TypeErrorCtor
                | BuiltinFunction::ReferenceErrorCtor
                | BuiltinFunction::RangeErrorCtor
                | BuiltinFunction::NumberCtor
                | BuiltinFunction::StringCtor
                | BuiltinFunction::BooleanCtor,
            ) => match callee {
                Value::BuiltinFunction(BuiltinFunction::MapCtor) => self.construct_map(args),
                Value::BuiltinFunction(BuiltinFunction::SetCtor) => self.construct_set(args),
                Value::BuiltinFunction(BuiltinFunction::DateCtor) => self.construct_date(args),
                Value::BuiltinFunction(BuiltinFunction::RegExpCtor) => self.construct_regexp(args),
                Value::BuiltinFunction(kind) => self.call_builtin(kind, Value::Undefined, args),
                _ => unreachable!(),
            },
            _ => Err(JsliteError::runtime(
                "only conservative built-in constructors are supported in v1",
            )),
        }
    }

    fn call_builtin(
        &mut self,
        function: BuiltinFunction,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        match function {
            BuiltinFunction::ArrayCtor => {
                let array = self.insert_array(args.to_vec(), IndexMap::new())?;
                Ok(Value::Array(array))
            }
            BuiltinFunction::ArrayFrom => self.call_array_from(args),
            BuiltinFunction::ArrayIsArray => {
                Ok(Value::Bool(matches!(args.first(), Some(Value::Array(_)))))
            }
            BuiltinFunction::ArrayPush => self.call_array_push(this_value, args),
            BuiltinFunction::ArrayPop => self.call_array_pop(this_value),
            BuiltinFunction::ArraySlice => self.call_array_slice(this_value, args),
            BuiltinFunction::ArrayJoin => self.call_array_join(this_value, args),
            BuiltinFunction::ArrayIncludes => self.call_array_includes(this_value, args),
            BuiltinFunction::ArrayIndexOf => self.call_array_index_of(this_value, args),
            BuiltinFunction::ArraySort => self.call_array_sort(this_value, args),
            BuiltinFunction::ArrayValues => self.call_array_values(this_value),
            BuiltinFunction::ArrayKeys => self.call_array_keys(this_value),
            BuiltinFunction::ArrayEntries => self.call_array_entries(this_value),
            BuiltinFunction::ArrayForEach => self.call_array_for_each(this_value, args),
            BuiltinFunction::ArrayMap => self.call_array_map(this_value, args),
            BuiltinFunction::ArrayFilter => self.call_array_filter(this_value, args),
            BuiltinFunction::ArrayFind => self.call_array_find(this_value, args),
            BuiltinFunction::ArrayFindIndex => self.call_array_find_index(this_value, args),
            BuiltinFunction::ArraySome => self.call_array_some(this_value, args),
            BuiltinFunction::ArrayEvery => self.call_array_every(this_value, args),
            BuiltinFunction::ArrayReduce => self.call_array_reduce(this_value, args),
            BuiltinFunction::ObjectCtor => {
                if let Some(Value::Object(object)) = args.first() {
                    Ok(Value::Object(*object))
                } else {
                    let object = self.insert_object(IndexMap::new(), ObjectKind::Plain)?;
                    Ok(Value::Object(object))
                }
            }
            BuiltinFunction::ObjectFromEntries => self.call_object_from_entries(args),
            BuiltinFunction::ObjectKeys => self.call_object_keys(args),
            BuiltinFunction::ObjectValues => self.call_object_values(args),
            BuiltinFunction::ObjectEntries => self.call_object_entries(args),
            BuiltinFunction::ObjectHasOwn => self.call_object_has_own(args),
            BuiltinFunction::MapCtor => Err(JsliteError::runtime(
                "TypeError: Map constructor must be called with new",
            )),
            BuiltinFunction::MapGet => self.call_map_get(this_value, args),
            BuiltinFunction::MapSet => self.call_map_set(this_value, args),
            BuiltinFunction::MapHas => self.call_map_has(this_value, args),
            BuiltinFunction::MapDelete => self.call_map_delete(this_value, args),
            BuiltinFunction::MapClear => self.call_map_clear(this_value),
            BuiltinFunction::MapEntries => self.call_map_entries(this_value),
            BuiltinFunction::MapKeys => self.call_map_keys(this_value),
            BuiltinFunction::MapValues => self.call_map_values(this_value),
            BuiltinFunction::SetCtor => Err(JsliteError::runtime(
                "TypeError: Set constructor must be called with new",
            )),
            BuiltinFunction::SetAdd => self.call_set_add(this_value, args),
            BuiltinFunction::SetHas => self.call_set_has(this_value, args),
            BuiltinFunction::SetDelete => self.call_set_delete(this_value, args),
            BuiltinFunction::SetClear => self.call_set_clear(this_value),
            BuiltinFunction::SetEntries => self.call_set_entries(this_value),
            BuiltinFunction::SetKeys => self.call_set_keys(this_value),
            BuiltinFunction::SetValues => self.call_set_values(this_value),
            BuiltinFunction::IteratorNext => self.call_iterator_next(this_value),
            BuiltinFunction::PromiseCtor => Err(JsliteError::runtime(
                "Promise construction is not supported in v1; use async functions or Promise.resolve/reject",
            )),
            BuiltinFunction::PromiseResolve => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                if let Value::Promise(_) = value {
                    Ok(value)
                } else {
                    Ok(Value::Promise(
                        self.insert_promise(PromiseState::Fulfilled(value))?,
                    ))
                }
            }
            BuiltinFunction::PromiseReject => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                Ok(Value::Promise(self.insert_promise(
                    PromiseState::Rejected(PromiseRejection {
                        value,
                        span: None,
                        traceback: Vec::new(),
                    }),
                )?))
            }
            BuiltinFunction::PromiseThen => self.call_promise_then(this_value, args),
            BuiltinFunction::PromiseCatch => self.call_promise_catch(this_value, args),
            BuiltinFunction::PromiseFinally => self.call_promise_finally(this_value, args),
            BuiltinFunction::PromiseAll => self.call_promise_all(args),
            BuiltinFunction::PromiseRace => self.call_promise_race(args),
            BuiltinFunction::PromiseAny => self.call_promise_any(args),
            BuiltinFunction::PromiseAllSettled => self.call_promise_all_settled(args),
            BuiltinFunction::RegExpCtor => self.construct_regexp(args),
            BuiltinFunction::RegExpExec => self.call_regexp_exec(this_value, args),
            BuiltinFunction::RegExpTest => self.call_regexp_test(this_value, args),
            BuiltinFunction::ErrorCtor => self.make_error_object("Error", args, None, None),
            BuiltinFunction::TypeErrorCtor => self.make_error_object("TypeError", args, None, None),
            BuiltinFunction::ReferenceErrorCtor => {
                self.make_error_object("ReferenceError", args, None, None)
            }
            BuiltinFunction::RangeErrorCtor => {
                self.make_error_object("RangeError", args, None, None)
            }
            BuiltinFunction::NumberCtor => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?,
            )),
            BuiltinFunction::DateCtor => Err(JsliteError::runtime(
                "TypeError: Date constructor must be called with new",
            )),
            BuiltinFunction::DateNow => Ok(Value::Number(current_time_millis())),
            BuiltinFunction::DateGetTime => self.call_date_get_time(this_value),
            BuiltinFunction::StringCtor => Ok(Value::String(
                self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?,
            )),
            BuiltinFunction::StringTrim => self.call_string_trim(this_value),
            BuiltinFunction::StringIncludes => self.call_string_includes(this_value, args),
            BuiltinFunction::StringStartsWith => self.call_string_starts_with(this_value, args),
            BuiltinFunction::StringEndsWith => self.call_string_ends_with(this_value, args),
            BuiltinFunction::StringSlice => self.call_string_slice(this_value, args),
            BuiltinFunction::StringSubstring => self.call_string_substring(this_value, args),
            BuiltinFunction::StringToLowerCase => self.call_string_to_lower_case(this_value),
            BuiltinFunction::StringToUpperCase => self.call_string_to_upper_case(this_value),
            BuiltinFunction::StringSplit => self.call_string_split(this_value, args),
            BuiltinFunction::StringReplace => self.call_string_replace(this_value, args),
            BuiltinFunction::StringReplaceAll => self.call_string_replace_all(this_value, args),
            BuiltinFunction::StringSearch => self.call_string_search(this_value, args),
            BuiltinFunction::StringMatch => self.call_string_match(this_value, args),
            BuiltinFunction::StringMatchAll => self.call_string_match_all(this_value, args),
            BuiltinFunction::BooleanCtor => Ok(Value::Bool(is_truthy(
                args.first().unwrap_or(&Value::Undefined),
            ))),
            BuiltinFunction::MathAbs => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .abs(),
            )),
            BuiltinFunction::MathMax => {
                let mut value = f64::NEG_INFINITY;
                for arg in args {
                    value = value.max(self.to_number(arg.clone())?);
                }
                Ok(Value::Number(value))
            }
            BuiltinFunction::MathMin => {
                let mut value = f64::INFINITY;
                for arg in args {
                    value = value.min(self.to_number(arg.clone())?);
                }
                Ok(Value::Number(value))
            }
            BuiltinFunction::MathFloor => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .floor(),
            )),
            BuiltinFunction::MathCeil => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .ceil(),
            )),
            BuiltinFunction::MathRound => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .round(),
            )),
            BuiltinFunction::MathPow => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .powf(self.to_number(args.get(1).cloned().unwrap_or(Value::Undefined))?),
            )),
            BuiltinFunction::MathSqrt => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .sqrt(),
            )),
            BuiltinFunction::MathTrunc => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .trunc(),
            )),
            BuiltinFunction::MathSign => {
                let value = self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?;
                Ok(Value::Number(if value.is_nan() {
                    f64::NAN
                } else if value == 0.0 {
                    value
                } else if value.is_sign_positive() {
                    1.0
                } else {
                    -1.0
                }))
            }
            BuiltinFunction::JsonStringify => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                let structured = self.value_to_structured(value)?;
                let json = serde_json::to_string(&structured_to_json(structured)?)
                    .map_err(|error| JsliteError::runtime(error.to_string()))?;
                Ok(Value::String(json))
            }
            BuiltinFunction::JsonParse => {
                let source = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
                let parsed: serde_json::Value = serde_json::from_str(&source)
                    .map_err(|error| JsliteError::runtime(error.to_string()))?;
                self.value_from_json(parsed)
            }
        }
    }

    fn install_builtins(&mut self) -> JsliteResult<()> {
        let global_object = self.insert_object(IndexMap::new(), ObjectKind::Global)?;
        self.define_global(
            "globalThis".to_string(),
            Value::Object(global_object),
            false,
        )?;
        self.define_global(
            "Object".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ObjectCtor),
            false,
        )?;
        self.define_global(
            "Map".to_string(),
            Value::BuiltinFunction(BuiltinFunction::MapCtor),
            false,
        )?;
        self.define_global(
            "Set".to_string(),
            Value::BuiltinFunction(BuiltinFunction::SetCtor),
            false,
        )?;
        self.define_global(
            "Array".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor),
            false,
        )?;
        self.define_global(
            "Date".to_string(),
            Value::BuiltinFunction(BuiltinFunction::DateCtor),
            false,
        )?;
        self.define_global(
            "Promise".to_string(),
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor),
            false,
        )?;
        self.define_global(
            "RegExp".to_string(),
            Value::BuiltinFunction(BuiltinFunction::RegExpCtor),
            false,
        )?;
        self.define_global(
            "String".to_string(),
            Value::BuiltinFunction(BuiltinFunction::StringCtor),
            false,
        )?;
        self.define_global(
            "Error".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ErrorCtor),
            false,
        )?;
        self.define_global(
            "TypeError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::TypeErrorCtor),
            false,
        )?;
        self.define_global(
            "ReferenceError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ReferenceErrorCtor),
            false,
        )?;
        self.define_global(
            "RangeError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::RangeErrorCtor),
            false,
        )?;
        self.define_global(
            "Number".to_string(),
            Value::BuiltinFunction(BuiltinFunction::NumberCtor),
            false,
        )?;
        self.define_global(
            "Boolean".to_string(),
            Value::BuiltinFunction(BuiltinFunction::BooleanCtor),
            false,
        )?;

        let math = self.insert_object(
            IndexMap::from([
                (
                    "abs".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathAbs),
                ),
                (
                    "max".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathMax),
                ),
                (
                    "min".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathMin),
                ),
                (
                    "floor".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathFloor),
                ),
                (
                    "ceil".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathCeil),
                ),
                (
                    "round".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathRound),
                ),
                (
                    "pow".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathPow),
                ),
                (
                    "sqrt".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathSqrt),
                ),
                (
                    "trunc".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathTrunc),
                ),
                (
                    "sign".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathSign),
                ),
            ]),
            ObjectKind::Math,
        )?;
        self.define_global("Math".to_string(), Value::Object(math), false)?;

        let json = self.insert_object(
            IndexMap::from([
                (
                    "stringify".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonStringify),
                ),
                (
                    "parse".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonParse),
                ),
            ]),
            ObjectKind::Json,
        )?;
        self.define_global("JSON".to_string(), Value::Object(json), false)?;

        let console = self.insert_object(IndexMap::new(), ObjectKind::Console)?;
        self.define_global("console".to_string(), Value::Object(console), false)?;
        Ok(())
    }

    fn construct_map(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let map = self.insert_map(Vec::new())?;
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        if matches!(iterable, Value::Null | Value::Undefined) {
            return Ok(Value::Map(map));
        }

        let iterator = self.create_iterator(iterable)?;
        loop {
            let (entry, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            let items = match entry {
                Value::Array(array) => self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .clone(),
                _ => {
                    return Err(JsliteError::runtime(
                        "TypeError: Map constructor expects an iterable of [key, value] pairs",
                    ));
                }
            };
            let key = items.first().cloned().unwrap_or(Value::Undefined);
            let value = items.get(1).cloned().unwrap_or(Value::Undefined);
            self.map_set(map, key, value)?;
        }

        Ok(Value::Map(map))
    }

    fn construct_set(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let set = self.insert_set(Vec::new())?;
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        if matches!(iterable, Value::Null | Value::Undefined) {
            return Ok(Value::Set(set));
        }

        let iterator = self.create_iterator(iterable)?;
        loop {
            let (value, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            self.set_add(set, value)?;
        }

        Ok(Value::Set(set))
    }

    fn construct_regexp(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let pattern_arg = args.first().cloned().unwrap_or(Value::Undefined);
        let flags_arg = args.get(1).cloned().unwrap_or(Value::Undefined);
        let (pattern, flags) = match pattern_arg {
            Value::Object(object) if self.is_regexp_object(object) => {
                let regex = self.regexp_object(object)?.clone();
                if matches!(flags_arg, Value::Undefined) {
                    (regex.pattern, regex.flags)
                } else {
                    (regex.pattern, self.to_string(flags_arg)?)
                }
            }
            value => {
                let pattern = if matches!(value, Value::Undefined) {
                    String::new()
                } else {
                    self.to_string(value)?
                };
                let flags = if matches!(flags_arg, Value::Undefined) {
                    String::new()
                } else {
                    self.to_string(flags_arg)?
                };
                (pattern, flags)
            }
        };
        self.make_regexp_value(pattern, flags)
    }

    fn construct_date(&mut self, args: &[Value]) -> JsliteResult<Value> {
        if args.len() > 1 {
            return Err(JsliteError::runtime(
                "TypeError: Date currently supports zero or one constructor argument",
            ));
        }
        let timestamp_ms = match args {
            [] => current_time_millis(),
            [value] => self.date_timestamp_ms_from_value(value.clone())?,
            _ => unreachable!(),
        };
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::Date(DateObject { timestamp_ms }),
        )?))
    }

    fn array_receiver(&self, value: Value, method: &str) -> JsliteResult<ArrayKey> {
        match value {
            Value::Array(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Array.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn date_receiver(&self, value: Value, method: &str) -> JsliteResult<ObjectKey> {
        match value {
            Value::Object(key) if self.is_date_object(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Date.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn string_receiver(&self, value: Value, method: &str) -> JsliteResult<String> {
        match value {
            Value::String(value) => Ok(value),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: String.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn call_array_from(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        let map_fn = match args.get(1).cloned() {
            Some(Value::Undefined) | None => None,
            Some(value) if is_callable(&value) => Some(value),
            Some(_) => {
                return Err(JsliteError::runtime(
                    "TypeError: Array.from expects a callable map function",
                ));
            }
        };
        let this_arg = args.get(2).cloned().unwrap_or(Value::Undefined);
        let iterator = self.create_iterator(iterable.clone())?;
        let result = self.insert_array(Vec::new(), IndexMap::new())?;
        let mut roots = vec![iterable, iterator.clone(), Value::Array(result)];
        if let Some(map_fn) = &map_fn {
            roots.push(map_fn.clone());
            roots.push(this_arg.clone());
        }
        self.with_temporary_roots(&roots, |runtime| {
            let mut index = 0usize;
            loop {
                let (value, done) = runtime.iterator_next(iterator.clone())?;
                if done {
                    break;
                }
                let mapped = if let Some(map_fn) = &map_fn {
                    runtime.with_temporary_roots(
                        &[
                            iterator.clone(),
                            Value::Array(result),
                            map_fn.clone(),
                            this_arg.clone(),
                            value.clone(),
                        ],
                        |runtime| {
                            runtime.call_callback(
                                map_fn.clone(),
                                this_arg.clone(),
                                &[value.clone(), Value::Number(index as f64)],
                                CallbackCallOptions {
                                    non_callable_message:
                                        "TypeError: Array.from expects a callable map function",
                                    host_suspension_message:
                                        "TypeError: Array.from mapping does not support host suspensions",
                                    unsettled_message:
                                        "synchronous Array.from mapping did not settle",
                                    allow_host_suspension: false,
                                    allow_pending_promise_result: true,
                                },
                            )
                        },
                    )?
                } else {
                    value
                };
                runtime
                    .arrays
                    .get_mut(result)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .push(mapped);
                runtime.refresh_array_accounting(result)?;
                index += 1;
            }
            Ok(Value::Array(result))
        })
    }

    fn call_array_push(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "push")?;
        {
            let elements = &mut self
                .arrays
                .get_mut(array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements;
            elements.extend(args.iter().cloned());
        }
        self.refresh_array_accounting(array)?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        Ok(Value::Number(length as f64))
    }

    fn call_array_pop(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "pop")?;
        let value = self
            .arrays
            .get_mut(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .pop()
            .unwrap_or(Value::Undefined);
        self.refresh_array_accounting(array)?;
        Ok(value)
    }

    fn call_array_slice(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "slice")?;
        let elements = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .clone();
        let start = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        let end = normalize_relative_bound(
            match args.get(1) {
                Some(value) => self.to_integer(value.clone())?,
                None => elements.len() as i64,
            },
            elements.len(),
        );
        let end = end.max(start);
        Ok(Value::Array(self.insert_array(
            elements[start..end].to_vec(),
            IndexMap::new(),
        )?))
    }

    fn call_array_join(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "join")?;
        let separator = match args.first() {
            Some(value) => self.to_string(value.clone())?,
            None => ",".to_string(),
        };
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements;
        let mut parts = Vec::with_capacity(elements.len());
        for value in elements {
            parts.push(match value {
                Value::Undefined | Value::Null => String::new(),
                other => self.to_string(other.clone())?,
            });
        }
        Ok(Value::String(parts.join(&separator)))
    }

    fn call_array_includes(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "includes")?;
        let search = args.first().cloned().unwrap_or(Value::Undefined);
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements;
        let start = normalize_search_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        Ok(Value::Bool(
            elements
                .iter()
                .skip(start)
                .any(|value| same_value_zero(value, &search)),
        ))
    }

    fn call_array_index_of(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "indexOf")?;
        let search = args.first().cloned().unwrap_or(Value::Undefined);
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements;
        let start = normalize_search_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        let index = elements
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, value)| strict_equal(value, &search))
            .map(|(index, _)| index as f64)
            .unwrap_or(-1.0);
        Ok(Value::Number(index))
    }

    fn sort_compare(
        &mut self,
        comparator: Option<Value>,
        left: Value,
        right: Value,
    ) -> JsliteResult<Ordering> {
        match comparator {
            Some(comparator) => {
                let result = self.with_temporary_roots(
                    &[comparator.clone(), left.clone(), right.clone()],
                    |runtime| {
                        runtime.call_callback(
                            comparator.clone(),
                            Value::Undefined,
                            &[left.clone(), right.clone()],
                            CallbackCallOptions {
                                non_callable_message:
                                    "TypeError: Array.prototype.sort expects a callable comparator",
                                host_suspension_message:
                                    "TypeError: Array.prototype.sort does not support host suspensions",
                                unsettled_message:
                                    "synchronous Array.prototype.sort comparator did not settle",
                                allow_host_suspension: false,
                                allow_pending_promise_result: false,
                            },
                        )
                    },
                )?;
                let ordering = self.to_number(result)?;
                Ok(if ordering.is_nan() || ordering == 0.0 {
                    Ordering::Equal
                } else if ordering < 0.0 {
                    Ordering::Less
                } else {
                    Ordering::Greater
                })
            }
            None => Ok(self.to_string(left)?.cmp(&self.to_string(right)?)),
        }
    }

    fn call_array_sort(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "sort")?;
        let comparator = match args.first().cloned() {
            Some(Value::Undefined) | None => None,
            Some(value) if is_callable(&value) => Some(value),
            Some(_) => {
                return Err(JsliteError::runtime(
                    "TypeError: Array.prototype.sort expects a callable comparator",
                ));
            }
        };
        let mut roots = vec![Value::Array(array)];
        if let Some(comparator) = &comparator {
            roots.push(comparator.clone());
        }
        self.with_temporary_roots(&roots, |runtime| {
            let mut elements = runtime
                .arrays
                .get(array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements
                .clone();
            for index in 1..elements.len() {
                let current = elements[index].clone();
                let mut position = index;
                while position > 0
                    && runtime.sort_compare(
                        comparator.clone(),
                        current.clone(),
                        elements[position - 1].clone(),
                    )? == Ordering::Less
                {
                    elements[position] = elements[position - 1].clone();
                    position -= 1;
                }
                elements[position] = current;
            }
            runtime
                .arrays
                .get_mut(array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements = elements;
            runtime.refresh_array_accounting(array)?;
            Ok(Value::Array(array))
        })
    }

    fn call_array_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::Array(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    fn call_array_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::ArrayKeys(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    fn call_array_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::ArrayEntries(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    fn array_callback(&self, args: &[Value], method: &str) -> JsliteResult<(Value, Value)> {
        let callback = args.first().cloned().unwrap_or(Value::Undefined);
        if !is_callable(&callback) {
            return Err(JsliteError::runtime(format!(
                "TypeError: Array.prototype.{method} expects a callable callback",
            )));
        }
        let this_arg = args.get(1).cloned().unwrap_or(Value::Undefined);
        Ok((callback, this_arg))
    }

    fn call_callback(
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
                    self.run_until_frame_depth(base_depth)?;
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
                    promise,
                    resume_behavior: ResumeBehavior::Value,
                    traceback: self.traceback_snapshots(),
                });
                Ok(Value::Promise(promise))
            }
            _ => Err(JsliteError::runtime(options.non_callable_message)),
        }
    }

    fn call_array_callback(
        &mut self,
        callback: Value,
        this_arg: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        self.call_callback(
            callback,
            this_arg,
            args,
            CallbackCallOptions {
                non_callable_message: "TypeError: array callback is not callable",
                host_suspension_message:
                    "TypeError: array callback helpers do not support synchronous host suspensions",
                unsettled_message: "synchronous array callback did not settle",
                allow_host_suspension: true,
                allow_pending_promise_result: true,
            },
        )
    }

    fn array_callback_value(&self, array: ArrayKey, index: usize) -> JsliteResult<Value> {
        Ok(self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .get(index)
            .cloned()
            .unwrap_or(Value::Undefined))
    }

    fn call_array_for_each(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "forEach")?;
        let (callback, this_arg) = self.array_callback(args, "forEach")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            let value = self.array_callback_value(array, index)?;
            self.with_temporary_roots(
                &[Value::Array(array), callback.clone(), this_arg.clone()],
                |runtime| {
                    runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[value, Value::Number(index as f64), Value::Array(array)],
                    )?;
                    Ok(())
                },
            )?;
        }
        Ok(Value::Undefined)
    }

    fn call_array_map(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "map")?;
        let (callback, this_arg) = self.array_callback(args, "map")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        let result = self.insert_array(Vec::new(), IndexMap::new())?;
        self.with_temporary_roots(
            &[
                Value::Array(array),
                callback.clone(),
                this_arg.clone(),
                Value::Array(result),
            ],
            |runtime| {
                for index in 0..length {
                    let value = runtime.array_callback_value(array, index)?;
                    let mapped = runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[value, Value::Number(index as f64), Value::Array(array)],
                    )?;
                    runtime
                        .arrays
                        .get_mut(result)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?
                        .elements
                        .push(mapped);
                    runtime.refresh_array_accounting(result)?;
                }
                Ok(Value::Array(result))
            },
        )
    }

    fn call_array_filter(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "filter")?;
        let (callback, this_arg) = self.array_callback(args, "filter")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        let result = self.insert_array(Vec::new(), IndexMap::new())?;
        self.with_temporary_roots(
            &[
                Value::Array(array),
                callback.clone(),
                this_arg.clone(),
                Value::Array(result),
            ],
            |runtime| {
                for index in 0..length {
                    let value = runtime.array_callback_value(array, index)?;
                    let keep = runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[
                            value.clone(),
                            Value::Number(index as f64),
                            Value::Array(array),
                        ],
                    )?;
                    if is_truthy(&keep) {
                        runtime
                            .arrays
                            .get_mut(result)
                            .ok_or_else(|| JsliteError::runtime("array missing"))?
                            .elements
                            .push(value);
                        runtime.refresh_array_accounting(result)?;
                    }
                }
                Ok(Value::Array(result))
            },
        )
    }

    fn call_array_find(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "find")?;
        let (callback, this_arg) = self.array_callback(args, "find")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            let value = self.array_callback_value(array, index)?;
            let found = self.with_temporary_roots(
                &[Value::Array(array), callback.clone(), this_arg.clone()],
                |runtime| {
                    runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[
                            value.clone(),
                            Value::Number(index as f64),
                            Value::Array(array),
                        ],
                    )
                },
            )?;
            if is_truthy(&found) {
                return Ok(value);
            }
        }
        Ok(Value::Undefined)
    }

    fn call_array_find_index(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "findIndex")?;
        let (callback, this_arg) = self.array_callback(args, "findIndex")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            let value = self.array_callback_value(array, index)?;
            let found = self.with_temporary_roots(
                &[Value::Array(array), callback.clone(), this_arg.clone()],
                |runtime| {
                    runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[value, Value::Number(index as f64), Value::Array(array)],
                    )
                },
            )?;
            if is_truthy(&found) {
                return Ok(Value::Number(index as f64));
            }
        }
        Ok(Value::Number(-1.0))
    }

    fn call_array_some(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "some")?;
        let (callback, this_arg) = self.array_callback(args, "some")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            let value = self.array_callback_value(array, index)?;
            let found = self.with_temporary_roots(
                &[Value::Array(array), callback.clone(), this_arg.clone()],
                |runtime| {
                    runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[value, Value::Number(index as f64), Value::Array(array)],
                    )
                },
            )?;
            if is_truthy(&found) {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    }

    fn call_array_every(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "every")?;
        let (callback, this_arg) = self.array_callback(args, "every")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            let value = self.array_callback_value(array, index)?;
            let found = self.with_temporary_roots(
                &[Value::Array(array), callback.clone(), this_arg.clone()],
                |runtime| {
                    runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[value, Value::Number(index as f64), Value::Array(array)],
                    )
                },
            )?;
            if !is_truthy(&found) {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    }

    fn call_array_reduce(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "reduce")?;
        let (callback, this_arg) = self.array_callback(args, "reduce")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        let (mut accumulator, start_index) = match args.get(1).cloned() {
            Some(initial) => (initial, 0),
            None if length > 0 => (self.array_callback_value(array, 0)?, 1),
            None => {
                return Err(JsliteError::runtime(
                    "TypeError: Array.prototype.reduce requires an initial value for empty arrays",
                ));
            }
        };
        for index in start_index..length {
            let value = self.array_callback_value(array, index)?;
            accumulator = self.with_temporary_roots(
                &[
                    Value::Array(array),
                    callback.clone(),
                    this_arg.clone(),
                    accumulator.clone(),
                ],
                |runtime| {
                    runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[
                            accumulator.clone(),
                            value,
                            Value::Number(index as f64),
                            Value::Array(array),
                        ],
                    )
                },
            )?;
        }
        Ok(accumulator)
    }

    fn enumerable_keys(&self, value: Value) -> JsliteResult<Vec<String>> {
        match value {
            Value::Object(object) => {
                let mut keys = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                keys.sort();
                Ok(keys)
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                let mut keys = (0..array.elements.len())
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>();
                let mut extra = array.properties.keys().cloned().collect::<Vec<_>>();
                extra.sort();
                keys.extend(extra);
                Ok(keys)
            }
            _ => Err(JsliteError::runtime(
                "TypeError: Object helpers currently only support plain objects and arrays",
            )),
        }
    }

    fn enumerable_value(&self, target: Value, key: &str) -> JsliteResult<Value> {
        match target {
            Value::Object(object) => self
                .objects
                .get(object)
                .and_then(|object| object.properties.get(key))
                .cloned()
                .ok_or_else(|| JsliteError::runtime("object property missing")),
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                if let Ok(index) = key.parse::<usize>() {
                    Ok(array
                        .elements
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Undefined))
                } else {
                    array
                        .properties
                        .get(key)
                        .cloned()
                        .ok_or_else(|| JsliteError::runtime("array property missing"))
                }
            }
            _ => Err(JsliteError::runtime(
                "TypeError: Object helpers currently only support plain objects and arrays",
            )),
        }
    }

    fn call_object_keys(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self
            .enumerable_keys(target)?
            .into_iter()
            .map(Value::String)
            .collect();
        Ok(Value::Array(self.insert_array(keys, IndexMap::new())?))
    }

    fn call_object_from_entries(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        let iterator = self.create_iterator(iterable.clone())?;
        let result = self.insert_object(IndexMap::new(), ObjectKind::Plain)?;
        self.with_temporary_roots(
            &[iterable, iterator.clone(), Value::Object(result)],
            |runtime| {
                loop {
                    let (entry, done) = runtime.iterator_next(iterator.clone())?;
                    if done {
                        break;
                    }
                    let items = match entry {
                        Value::Array(array) => runtime
                            .arrays
                            .get(array)
                            .ok_or_else(|| JsliteError::runtime("array missing"))?
                            .elements
                            .clone(),
                        _ => {
                            return Err(JsliteError::runtime(
                                "TypeError: Object.fromEntries expects an iterable of [key, value] pairs",
                            ));
                        }
                    };
                    let key = runtime
                        .to_property_key(items.first().cloned().unwrap_or(Value::Undefined))?;
                    let value = items.get(1).cloned().unwrap_or(Value::Undefined);
                    runtime
                        .objects
                        .get_mut(result)
                        .ok_or_else(|| JsliteError::runtime("object missing"))?
                        .properties
                        .insert(key, value);
                    runtime.refresh_object_accounting(result)?;
                }
                Ok(Value::Object(result))
            },
        )
    }

    fn call_object_values(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self.enumerable_keys(target.clone())?;
        let mut values = Vec::with_capacity(keys.len());
        for key in keys {
            values.push(self.enumerable_value(target.clone(), &key)?);
        }
        Ok(Value::Array(self.insert_array(values, IndexMap::new())?))
    }

    fn call_object_entries(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self.enumerable_keys(target.clone())?;
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let pair = self.insert_array(
                vec![
                    Value::String(key.clone()),
                    self.enumerable_value(target.clone(), &key)?,
                ],
                IndexMap::new(),
            )?;
            entries.push(Value::Array(pair));
        }
        Ok(Value::Array(self.insert_array(entries, IndexMap::new())?))
    }

    fn call_object_has_own(&self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let key = self.to_property_key(args.get(1).cloned().unwrap_or(Value::Undefined))?;
        let has_key = match target {
            Value::Object(object) => self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?
                .properties
                .contains_key(&key),
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                key.parse::<usize>()
                    .ok()
                    .is_some_and(|index| index < array.elements.len())
                    || array.properties.contains_key(&key)
            }
            _ => {
                return Err(JsliteError::runtime(
                    "TypeError: Object helpers currently only support plain objects and arrays",
                ));
            }
        };
        Ok(Value::Bool(has_key))
    }

    fn call_string_trim(&self, this_value: Value) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "trim")?;
        Ok(Value::String(value.trim().to_string()))
    }

    fn call_string_includes(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "includes")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = clamp_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let haystack = chars[position..].iter().collect::<String>();
        Ok(Value::Bool(
            haystack.contains(&needle.iter().collect::<String>()),
        ))
    }

    fn call_string_starts_with(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "startsWith")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = clamp_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        Ok(Value::Bool(chars[position..].starts_with(&needle)))
    }

    fn call_string_ends_with(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "endsWith")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let end = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, chars.len()),
            None => chars.len(),
        };
        Ok(Value::Bool(chars[..end].ends_with(&needle)))
    }

    fn call_string_slice(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "slice")?;
        let chars = value.chars().collect::<Vec<_>>();
        let start = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let end = normalize_relative_bound(
            match args.get(1) {
                Some(value) => self.to_integer(value.clone())?,
                None => chars.len() as i64,
            },
            chars.len(),
        );
        let end = end.max(start);
        Ok(Value::String(chars[start..end].iter().collect()))
    }

    fn call_string_substring(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "substring")?;
        let chars = value.chars().collect::<Vec<_>>();
        let start = clamp_index(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let end = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, chars.len()),
            None => chars.len(),
        };
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Ok(Value::String(chars[start..end].iter().collect()))
    }

    fn call_string_to_lower_case(&self, this_value: Value) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "toLowerCase")?;
        Ok(Value::String(value.to_lowercase()))
    }

    fn call_string_to_upper_case(&self, this_value: Value) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "toUpperCase")?;
        Ok(Value::String(value.to_uppercase()))
    }

    fn string_search_pattern(
        &self,
        value: Value,
        method: &str,
    ) -> JsliteResult<StringSearchPattern> {
        match value {
            Value::Object(object) if self.is_regexp_object(object) => {
                Ok(StringSearchPattern::RegExp {
                    object,
                    regex: self.regexp_object(object)?.clone(),
                })
            }
            value => {
                if is_callable(&value) {
                    return Err(JsliteError::runtime(format!(
                        "TypeError: String.prototype.{method} does not support callback patterns",
                    )));
                }
                Ok(StringSearchPattern::Literal(self.to_string(value)?))
            }
        }
    }

    fn string_callback_replacement(
        &mut self,
        method: &str,
        callback: Value,
        input: &str,
        matched: &RegExpMatchData,
    ) -> JsliteResult<String> {
        let mut args = vec![Value::String(
            input[matched.start_byte..matched.end_byte].to_string(),
        )];
        args.extend(
            matched
                .captures
                .iter()
                .map(|value| value.clone().map_or(Value::Undefined, Value::String)),
        );
        args.push(Value::Number(matched.start_index as f64));
        args.push(Value::String(input.to_string()));
        if !matched.named_groups.is_empty() {
            let groups = matched
                .named_groups
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        value.clone().map_or(Value::Undefined, Value::String),
                    )
                })
                .collect::<IndexMap<_, _>>();
            let object = self.insert_object(groups, ObjectKind::Plain)?;
            args.push(Value::Object(object));
        }
        let mut roots = vec![callback.clone()];
        roots.extend(args.iter().cloned());
        let value = self.with_temporary_roots(&roots, |runtime| {
            runtime.call_callback(
                callback.clone(),
                Value::Undefined,
                &args,
                CallbackCallOptions {
                    non_callable_message: &format!(
                        "TypeError: String.prototype.{method} replacement callback is not callable"
                    ),
                    host_suspension_message: &format!(
                        "TypeError: String.prototype.{method} callback replacements do not support host suspensions"
                    ),
                    unsettled_message: &format!(
                        "synchronous String.prototype.{method} callback did not settle"
                    ),
                    allow_host_suspension: false,
                    allow_pending_promise_result: false,
                },
            )
        })?;
        self.to_string(value)
    }

    fn call_string_split(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "split")?;
        let limit = match args.get(1) {
            Some(value) => {
                let limit = self.to_integer(value.clone())?;
                if limit <= 0 {
                    0
                } else {
                    usize::try_from(limit).unwrap_or(usize::MAX)
                }
            }
            None => usize::MAX,
        };
        if limit == 0 {
            return Ok(Value::Array(
                self.insert_array(Vec::new(), IndexMap::new())?,
            ));
        }
        let pattern = match args.first() {
            None | Some(Value::Undefined) => None,
            Some(value) => Some(self.string_search_pattern(value.clone(), "split")?),
        };
        let elements = match pattern {
            None => vec![Value::String(value)],
            Some(StringSearchPattern::Literal(separator)) => {
                split_string_by_pattern(&value, Some(separator.as_str()), limit)
                    .into_iter()
                    .map(Value::String)
                    .collect()
            }
            Some(StringSearchPattern::RegExp { regex, .. }) => {
                let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                let mut elements = Vec::new();
                let mut last_end = 0usize;
                for matched in matches {
                    if elements.len() >= limit {
                        break;
                    }
                    elements.push(Value::String(
                        value[last_end..matched.start_byte].to_string(),
                    ));
                    if elements.len() >= limit {
                        break;
                    }
                    for capture in matched.captures {
                        elements.push(capture.map_or(Value::Undefined, Value::String));
                        if elements.len() >= limit {
                            break;
                        }
                    }
                    last_end = matched.end_byte;
                }
                if elements.len() < limit {
                    elements.push(Value::String(value[last_end..].to_string()));
                }
                elements
            }
        };
        Ok(Value::Array(self.insert_array(
            elements.into_iter().take(limit).collect(),
            IndexMap::new(),
        )?))
    }

    fn call_string_replace(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "replace")?;
        let search = self
            .string_search_pattern(args.first().cloned().unwrap_or(Value::Undefined), "replace")?;
        let replacement = args.get(1).cloned().unwrap_or(Value::Undefined);
        match (search, replacement.clone()) {
            (StringSearchPattern::Literal(search), replacement) if is_callable(&replacement) => {
                let matched = if search.is_empty() {
                    Some(RegExpMatchData {
                        start_byte: 0,
                        end_byte: 0,
                        start_index: 0,
                        end_index: 0,
                        captures: Vec::new(),
                        named_groups: IndexMap::new(),
                    })
                } else {
                    self.literal_match_data(&value, &search, 0)
                };
                if let Some(matched) = matched {
                    let replacement =
                        self.string_callback_replacement("replace", replacement, &value, &matched)?;
                    let mut result = String::new();
                    result.push_str(&value[..matched.start_byte]);
                    result.push_str(&replacement);
                    result.push_str(&value[matched.end_byte..]);
                    Ok(Value::String(result))
                } else {
                    Ok(Value::String(value))
                }
            }
            (StringSearchPattern::Literal(search), replacement) => Ok(Value::String(
                replace_first_string_match(&value, &search, &self.to_string(replacement)?),
            )),
            (StringSearchPattern::RegExp { regex, .. }, replacement)
                if is_callable(&replacement) =>
            {
                let all = regex.flags.contains('g');
                let matches = self.collect_regexp_matches_from_state(&regex, &value, all)?;
                if matches.is_empty() {
                    return Ok(Value::String(value));
                }
                let mut result = String::new();
                let mut last_end = 0usize;
                for matched in &matches {
                    result.push_str(&value[last_end..matched.start_byte]);
                    result.push_str(&self.string_callback_replacement(
                        "replace",
                        replacement.clone(),
                        &value,
                        matched,
                    )?);
                    last_end = matched.end_byte;
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
            (StringSearchPattern::RegExp { regex, .. }, replacement) => {
                let all = regex.flags.contains('g');
                let matches = self.collect_regexp_matches_from_state(&regex, &value, all)?;
                if matches.is_empty() {
                    return Ok(Value::String(value));
                }
                let replacement = self.to_string(replacement)?;
                let mut result = String::new();
                let mut last_end = 0usize;
                for matched in &matches {
                    result.push_str(&value[last_end..matched.start_byte]);
                    result.push_str(&expand_regexp_replacement_template(
                        &replacement,
                        &value,
                        matched,
                    ));
                    last_end = matched.end_byte;
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
        }
    }

    fn call_string_replace_all(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "replaceAll")?;
        let search = self.string_search_pattern(
            args.first().cloned().unwrap_or(Value::Undefined),
            "replaceAll",
        )?;
        let replacement = args.get(1).cloned().unwrap_or(Value::Undefined);
        match search {
            StringSearchPattern::Literal(search) if is_callable(&replacement) => {
                let mut matches = Vec::new();
                if search.is_empty() {
                    let total = value.chars().count();
                    for index in 0..=total {
                        let byte = char_index_to_byte_index(&value, index);
                        matches.push(RegExpMatchData {
                            start_byte: byte,
                            end_byte: byte,
                            start_index: index,
                            end_index: index,
                            captures: Vec::new(),
                            named_groups: IndexMap::new(),
                        });
                    }
                } else {
                    let mut start_index = 0usize;
                    while let Some(matched) = self.literal_match_data(&value, &search, start_index)
                    {
                        start_index = matched.end_index;
                        matches.push(matched);
                    }
                }
                let mut result = String::new();
                let mut last_end = 0usize;
                for matched in &matches {
                    result.push_str(&value[last_end..matched.start_byte]);
                    result.push_str(&self.string_callback_replacement(
                        "replaceAll",
                        replacement.clone(),
                        &value,
                        matched,
                    )?);
                    last_end = matched.end_byte;
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
            StringSearchPattern::Literal(search) => Ok(Value::String(replace_all_string_matches(
                &value,
                &search,
                &self.to_string(replacement)?,
            ))),
            StringSearchPattern::RegExp { regex, .. } => {
                if !regex.flags.contains('g') {
                    return Err(JsliteError::runtime(
                        "TypeError: String.prototype.replaceAll requires a global RegExp",
                    ));
                }
                let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                if matches.is_empty() {
                    return Ok(Value::String(value));
                }
                let mut result = String::new();
                let mut last_end = 0usize;
                if is_callable(&replacement) {
                    for matched in &matches {
                        result.push_str(&value[last_end..matched.start_byte]);
                        result.push_str(&self.string_callback_replacement(
                            "replaceAll",
                            replacement.clone(),
                            &value,
                            matched,
                        )?);
                        last_end = matched.end_byte;
                    }
                } else {
                    let replacement = self.to_string(replacement)?;
                    for matched in &matches {
                        result.push_str(&value[last_end..matched.start_byte]);
                        result.push_str(&expand_regexp_replacement_template(
                            &replacement,
                            &value,
                            matched,
                        ));
                        last_end = matched.end_byte;
                    }
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
        }
    }

    fn call_string_search(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "search")?;
        let needle = self
            .string_search_pattern(args.first().cloned().unwrap_or(Value::Undefined), "search")?;
        Ok(Value::Number(match needle {
            StringSearchPattern::Literal(needle) => find_string_pattern(&value, &needle, 0)
                .map(|index| index as f64)
                .unwrap_or(-1.0),
            StringSearchPattern::RegExp { regex, .. } => self
                .first_regexp_match_from_state(&regex, &value, 0)?
                .map(|matched| matched.start_index as f64)
                .unwrap_or(-1.0),
        }))
    }

    fn call_string_match(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "match")?;
        let needle =
            self.string_search_pattern(args.first().cloned().unwrap_or(Value::Undefined), "match")?;
        match needle {
            StringSearchPattern::Literal(needle) => {
                let Some(index) = find_string_pattern(&value, &needle, 0) else {
                    return Ok(Value::Null);
                };
                let match_array = self.insert_array(
                    vec![Value::String(needle.clone())],
                    IndexMap::from([
                        ("index".to_string(), Value::Number(index as f64)),
                        ("input".to_string(), Value::String(value)),
                    ]),
                )?;
                Ok(Value::Array(match_array))
            }
            StringSearchPattern::RegExp { object, regex } => {
                if regex.flags.contains('g') {
                    self.regexp_object_mut(object)?.last_index = 0;
                    self.refresh_object_accounting(object)?;
                    let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                    if matches.is_empty() {
                        return Ok(Value::Null);
                    }
                    let array = self.insert_array(
                        matches
                            .into_iter()
                            .map(|matched| {
                                Value::String(
                                    value[matched.start_byte..matched.end_byte].to_string(),
                                )
                            })
                            .collect(),
                        IndexMap::new(),
                    )?;
                    Ok(Value::Array(array))
                } else {
                    let Some(matched) = self.first_regexp_match_from_state(&regex, &value, 0)?
                    else {
                        return Ok(Value::Null);
                    };
                    self.regexp_match_array_value(&value, &matched)
                }
            }
        }
    }

    fn call_string_match_all(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "matchAll")?;
        let needle = self.string_search_pattern(
            args.first().cloned().unwrap_or(Value::Undefined),
            "matchAll",
        )?;
        let matches = match needle {
            StringSearchPattern::Literal(needle) => collect_literal_matches(&value, &needle),
            StringSearchPattern::RegExp { object, regex } => {
                if !regex.flags.contains('g') {
                    return Err(JsliteError::runtime(
                        "TypeError: String.prototype.matchAll requires a global RegExp",
                    ));
                }
                self.regexp_object_mut(object)?.last_index = 0;
                self.refresh_object_accounting(object)?;
                self.collect_regexp_matches_from_state(&regex, &value, true)?
            }
        };
        let mut values = Vec::with_capacity(matches.len());
        for matched in matches {
            values.push(self.regexp_match_array_value(&value, &matched)?);
        }
        let array = self.insert_array(values, IndexMap::new())?;
        self.call_array_values(Value::Array(array))
    }

    fn call_date_get_time(&self, this_value: Value) -> JsliteResult<Value> {
        let date = self.date_receiver(this_value, "getTime")?;
        Ok(Value::Number(self.date_object(date)?.timestamp_ms))
    }

    fn map_receiver(&self, value: Value, method: &str) -> JsliteResult<MapKey> {
        match value {
            Value::Map(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Map.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn set_receiver(&self, value: Value, method: &str) -> JsliteResult<SetKey> {
        match value {
            Value::Set(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Set.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn iterator_receiver(&self, value: Value, method: &str) -> JsliteResult<IteratorKey> {
        match value {
            Value::Iterator(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: iterator.{method} called on incompatible receiver",
            ))),
        }
    }

    fn regexp_receiver(&self, value: Value, method: &str) -> JsliteResult<ObjectKey> {
        match value {
            Value::Object(key) if self.is_regexp_object(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: RegExp.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn regexp_match_array_value(
        &mut self,
        input: &str,
        matched: &RegExpMatchData,
    ) -> JsliteResult<Value> {
        let mut groups = IndexMap::new();
        for (name, value) in &matched.named_groups {
            groups.insert(
                name.clone(),
                value.clone().map_or(Value::Undefined, Value::String),
            );
        }
        let mut properties = IndexMap::from([
            (
                "index".to_string(),
                Value::Number(matched.start_index as f64),
            ),
            ("input".to_string(), Value::String(input.to_string())),
        ]);
        if !groups.is_empty() {
            properties.insert(
                "groups".to_string(),
                Value::Object(self.insert_object(groups, ObjectKind::Plain)?),
            );
        }
        let mut elements = Vec::with_capacity(matched.captures.len() + 1);
        elements.push(Value::String(
            input[matched.start_byte..matched.end_byte].to_string(),
        ));
        elements.extend(
            matched
                .captures
                .iter()
                .map(|value| value.clone().map_or(Value::Undefined, Value::String)),
        );
        Ok(Value::Array(self.insert_array(elements, properties)?))
    }

    fn call_regexp_exec(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let regex = self.regexp_receiver(this_value, "exec")?;
        let input = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        let Some(matched) = self.first_regexp_match(regex, &input)? else {
            return Ok(Value::Null);
        };
        self.regexp_match_array_value(&input, &matched)
    }

    fn call_regexp_test(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let regex = self.regexp_receiver(this_value, "test")?;
        let input = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        Ok(Value::Bool(
            self.first_regexp_match(regex, &input)?.is_some(),
        ))
    }

    fn call_map_get(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "get")?;
        Ok(self
            .map_get(map, &key)?
            .map(|entry| entry.value)
            .unwrap_or(Value::Undefined))
    }

    fn call_map_set(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "set")?;
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let value = args.get(1).cloned().unwrap_or(Value::Undefined);
        self.map_set(map, key, value)?;
        Ok(Value::Map(map))
    }

    fn call_map_has(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "has")?;
        Ok(Value::Bool(self.map_get(map, &key)?.is_some()))
    }

    fn call_map_delete(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.map_delete(map, &key)?))
    }

    fn call_map_clear(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "clear")?;
        self.map_clear(map)?;
        Ok(Value::Undefined)
    }

    fn call_map_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapEntries(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    fn call_map_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapKeys(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    fn call_map_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapValues(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    fn call_set_add(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "add")?;
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        self.set_add(set, value)?;
        Ok(Value::Set(set))
    }

    fn call_set_has(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "has")?;
        Ok(Value::Bool(self.set_contains(set, &value)?))
    }

    fn call_set_delete(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.set_delete(set, &value)?))
    }

    fn call_set_clear(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "clear")?;
        self.set_clear(set)?;
        Ok(Value::Undefined)
    }

    fn call_set_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetEntries(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    fn call_set_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    fn call_set_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    fn call_iterator_next(&mut self, this_value: Value) -> JsliteResult<Value> {
        let iterator = self.iterator_receiver(this_value, "next")?;
        let (value, done) = self.iterator_next(Value::Iterator(iterator))?;
        let result = self.insert_object(
            IndexMap::from([
                ("value".to_string(), value),
                ("done".to_string(), Value::Bool(done)),
            ]),
            ObjectKind::Plain,
        )?;
        Ok(Value::Object(result))
    }

    fn promise_receiver(&self, value: Value, method: &str) -> JsliteResult<PromiseKey> {
        match value {
            Value::Promise(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Promise.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn collect_iterable_values(&mut self, iterable: Value) -> JsliteResult<Vec<Value>> {
        let iterator = self.create_iterator(iterable)?;
        let mut values = Vec::new();
        loop {
            let (value, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            values.push(value);
        }
        Ok(values)
    }

    fn call_promise_then(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "then")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let on_fulfilled = args.first().cloned().filter(is_callable);
        let on_rejected = args.get(1).cloned().filter(is_callable);
        self.attach_promise_reaction(
            promise,
            PromiseReaction::Then {
                target,
                on_fulfilled,
                on_rejected,
            },
        )?;
        Ok(Value::Promise(target))
    }

    fn call_promise_catch(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "catch")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let on_rejected = args.first().cloned().filter(is_callable);
        self.attach_promise_reaction(
            promise,
            PromiseReaction::Then {
                target,
                on_fulfilled: None,
                on_rejected,
            },
        )?;
        Ok(Value::Promise(target))
    }

    fn call_promise_finally(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "finally")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let callback = args.first().cloned().filter(is_callable);
        self.attach_promise_reaction(promise, PromiseReaction::Finally { target, callback })?;
        Ok(Value::Promise(target))
    }

    fn call_promise_all(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let array = Value::Array(self.insert_array(Vec::new(), IndexMap::new())?);
            self.resolve_promise(target, array)?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::All {
            remaining: values.len(),
            values: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::All,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn call_promise_race(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        for value in
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?
        {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index: 0,
                    kind: PromiseCombinatorKind::Race,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn call_promise_any(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let error = self.make_aggregate_error(Vec::new())?;
            self.reject_promise(
                target,
                PromiseRejection {
                    value: error,
                    span: None,
                    traceback: self.traceback_snapshots(),
                },
            )?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::Any {
            remaining: values.len(),
            reasons: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::Any,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn call_promise_all_settled(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let array = Value::Array(self.insert_array(Vec::new(), IndexMap::new())?);
            self.resolve_promise(target, array)?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::AllSettled {
            remaining: values.len(),
            results: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::AllSettled,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn map_get(&self, map: MapKey, key: &Value) -> JsliteResult<Option<MapEntry>> {
        let normalized = canonicalize_collection_key(key.clone());
        Ok(self
            .maps
            .get(map)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .entries
            .iter()
            .find(|entry| same_value_zero(&entry.key, &normalized))
            .cloned())
    }

    fn map_set(&mut self, map: MapKey, key: Value, value: Value) -> JsliteResult<()> {
        let key = canonicalize_collection_key(key);
        {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries;
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| same_value_zero(&entry.key, &key))
            {
                entry.value = value;
            } else {
                entries.push(MapEntry { key, value });
            }
        }
        self.refresh_map_accounting(map)
    }

    fn map_delete(&mut self, map: MapKey, key: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(key.clone());
        let removed = {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(&entry.key, &normalized))
            {
                entries.remove(index);
                true
            } else {
                false
            }
        };
        if removed {
            self.refresh_map_accounting(map)?;
        }
        Ok(removed)
    }

    fn map_clear(&mut self, map: MapKey) -> JsliteResult<()> {
        self.maps
            .get_mut(map)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .entries
            .clear();
        self.refresh_map_accounting(map)
    }

    fn set_add(&mut self, set: SetKey, value: Value) -> JsliteResult<()> {
        let value = canonicalize_collection_key(value);
        {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries;
            if !entries.iter().any(|entry| same_value_zero(entry, &value)) {
                entries.push(value);
            }
        }
        self.refresh_set_accounting(set)
    }

    fn set_contains(&self, set: SetKey, value: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        Ok(self
            .sets
            .get(set)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .entries
            .iter()
            .any(|entry| same_value_zero(entry, &normalized)))
    }

    fn set_delete(&mut self, set: SetKey, value: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        let removed = {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(entry, &normalized))
            {
                entries.remove(index);
                true
            } else {
                false
            }
        };
        if removed {
            self.refresh_set_accounting(set)?;
        }
        Ok(removed)
    }

    fn set_clear(&mut self, set: SetKey) -> JsliteResult<()> {
        self.sets
            .get_mut(set)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .entries
            .clear();
        self.refresh_set_accounting(set)
    }

    fn date_timestamp_ms_from_value(&self, value: Value) -> JsliteResult<f64> {
        match value {
            Value::Number(value) => Ok(value),
            Value::String(value) => Ok(parse_date_timestamp_ms(&value)),
            Value::Object(object) if self.is_date_object(object) => {
                Ok(self.date_object(object)?.timestamp_ms)
            }
            Value::Undefined => Ok(f64::NAN),
            _ => Err(JsliteError::runtime(
                "TypeError: Date currently supports only numeric, string, or Date arguments",
            )),
        }
    }

    fn make_regexp_value(&mut self, pattern: String, flags: String) -> JsliteResult<Value> {
        self.validate_regexp_flags(&flags)?;
        self.compile_regexp(&pattern, &flags)?;
        let object = self.insert_object(
            IndexMap::new(),
            ObjectKind::RegExp(RegExpObject {
                pattern,
                flags,
                last_index: 0,
            }),
        )?;
        Ok(Value::Object(object))
    }

    fn validate_regexp_flags(&self, flags: &str) -> JsliteResult<RegExpFlagsState> {
        let mut state = RegExpFlagsState {
            global: false,
            ignore_case: false,
            multiline: false,
            dot_all: false,
            unicode: false,
            sticky: false,
        };
        let mut seen = HashSet::new();
        for flag in flags.chars() {
            if !seen.insert(flag) {
                return Err(JsliteError::runtime(format!(
                    "SyntaxError: duplicate regular expression flag `{flag}`",
                )));
            }
            match flag {
                'g' => state.global = true,
                'i' => state.ignore_case = true,
                'm' => state.multiline = true,
                's' => state.dot_all = true,
                'u' => state.unicode = true,
                'y' => state.sticky = true,
                _ => {
                    return Err(JsliteError::runtime(format!(
                        "SyntaxError: unsupported regular expression flag `{flag}`",
                    )));
                }
            }
        }
        Ok(state)
    }

    fn compile_regexp(&self, pattern: &str, flags: &str) -> JsliteResult<Regex> {
        let flags = self.validate_regexp_flags(flags)?;
        let mut engine_flags = String::new();
        if flags.ignore_case {
            engine_flags.push('i');
        }
        if flags.multiline {
            engine_flags.push('m');
        }
        if flags.dot_all {
            engine_flags.push('s');
        }
        if flags.unicode {
            engine_flags.push('u');
        }
        Regex::with_flags(pattern, engine_flags.as_str()).map_err(|error| {
            JsliteError::runtime(format!("SyntaxError: invalid regular expression: {error}"))
        })
    }

    fn is_regexp_object(&self, key: ObjectKey) -> bool {
        self.objects
            .get(key)
            .is_some_and(|object| matches!(object.kind, ObjectKind::RegExp(_)))
    }

    fn is_date_object(&self, key: ObjectKey) -> bool {
        self.objects
            .get(key)
            .is_some_and(|object| matches!(object.kind, ObjectKind::Date(_)))
    }

    fn date_object(&self, key: ObjectKey) -> JsliteResult<&DateObject> {
        match &self
            .objects
            .get(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .kind
        {
            ObjectKind::Date(date) => Ok(date),
            _ => Err(JsliteError::runtime("date missing")),
        }
    }

    fn regexp_object(&self, key: ObjectKey) -> JsliteResult<&RegExpObject> {
        match &self
            .objects
            .get(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .kind
        {
            ObjectKind::RegExp(regex) => Ok(regex),
            _ => Err(JsliteError::runtime("regexp missing")),
        }
    }

    fn regexp_object_mut(&mut self, key: ObjectKey) -> JsliteResult<&mut RegExpObject> {
        match &mut self
            .objects
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .kind
        {
            ObjectKind::RegExp(regex) => Ok(regex),
            _ => Err(JsliteError::runtime("regexp missing")),
        }
    }

    fn first_regexp_match_from_state(
        &self,
        regex: &RegExpObject,
        text: &str,
        start_index: usize,
    ) -> JsliteResult<Option<RegExpMatchData>> {
        let flags = self.validate_regexp_flags(&regex.flags)?;
        let compiled = self.compile_regexp(&regex.pattern, &regex.flags)?;
        let start_byte = char_index_to_byte_index(text, start_index);
        let matched = compiled.find_from(text, start_byte).next();
        let Some(matched) = matched else {
            return Ok(None);
        };
        if flags.sticky && matched.start() != start_byte {
            return Ok(None);
        }
        let named_groups = matched
            .named_groups()
            .map(|(name, range)| {
                (
                    name.to_string(),
                    range.map(|range| text[range.start..range.end].to_string()),
                )
            })
            .collect::<IndexMap<_, _>>();
        Ok(Some(RegExpMatchData {
            start_byte: matched.start(),
            end_byte: matched.end(),
            start_index: byte_index_to_char_index(text, matched.start()),
            end_index: byte_index_to_char_index(text, matched.end()),
            captures: matched
                .captures
                .iter()
                .map(|range| {
                    range
                        .clone()
                        .map(|range| text[range.start..range.end].to_string())
                })
                .collect(),
            named_groups,
        }))
    }

    fn first_regexp_match(
        &mut self,
        regex_key: ObjectKey,
        text: &str,
    ) -> JsliteResult<Option<RegExpMatchData>> {
        let regex = self.regexp_object(regex_key)?.clone();
        let flags = self.validate_regexp_flags(&regex.flags)?;
        let start_index = if flags.global || flags.sticky {
            regex.last_index
        } else {
            0
        };
        let matched = self.first_regexp_match_from_state(&regex, text, start_index)?;
        if flags.global || flags.sticky {
            let next_index = matched.as_ref().map_or(0, |matched| {
                if matched.start_byte == matched.end_byte {
                    advance_char_index(text, matched.start_index)
                } else {
                    matched.end_index
                }
            });
            self.regexp_object_mut(regex_key)?.last_index = next_index;
            self.refresh_object_accounting(regex_key)?;
        }
        Ok(matched)
    }

    fn collect_regexp_matches_from_state(
        &self,
        regex: &RegExpObject,
        text: &str,
        all: bool,
    ) -> JsliteResult<Vec<RegExpMatchData>> {
        let mut matches = Vec::new();
        let mut start_index = 0usize;
        loop {
            let Some(matched) = self.first_regexp_match_from_state(regex, text, start_index)?
            else {
                break;
            };
            let next_index = if matched.start_byte == matched.end_byte {
                advance_char_index(text, matched.start_index)
            } else {
                matched.end_index
            };
            matches.push(matched);
            if !all {
                break;
            }
            if next_index < start_index {
                break;
            }
            start_index = next_index;
        }
        Ok(matches)
    }

    fn literal_match_data(
        &self,
        value: &str,
        needle: &str,
        start_index: usize,
    ) -> Option<RegExpMatchData> {
        let start_index = find_string_pattern(value, needle, start_index)?;
        let start_byte = char_index_to_byte_index(value, start_index);
        let end_index = start_index + needle.chars().count();
        let end_byte = char_index_to_byte_index(value, end_index);
        Some(RegExpMatchData {
            start_byte,
            end_byte,
            start_index,
            end_index,
            captures: Vec::new(),
            named_groups: IndexMap::new(),
        })
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
        _ => strict_equal(left, right),
    }
}

fn normalize_relative_bound(index: i64, len: usize) -> usize {
    let len = len as i64;
    if index < 0 {
        (len + index).max(0) as usize
    } else {
        index.min(len) as usize
    }
}

fn normalize_search_index(index: i64, len: usize) -> usize {
    if index < 0 {
        normalize_relative_bound(index, len)
    } else {
        clamp_index(index, len)
    }
}

fn collect_literal_matches(value: &str, needle: &str) -> Vec<RegExpMatchData> {
    if needle.is_empty() {
        let total = value.chars().count();
        return (0..=total)
            .map(|index| {
                let byte = char_index_to_byte_index(value, index);
                RegExpMatchData {
                    start_byte: byte,
                    end_byte: byte,
                    start_index: index,
                    end_index: index,
                    captures: Vec::new(),
                    named_groups: IndexMap::new(),
                }
            })
            .collect();
    }

    let mut matches = Vec::new();
    let mut start_index = 0usize;
    while let Some(matched) = find_string_pattern(value, needle, start_index).map(|index| {
        let start_byte = char_index_to_byte_index(value, index);
        let end_index = index + needle.chars().count();
        let end_byte = char_index_to_byte_index(value, end_index);
        RegExpMatchData {
            start_byte,
            end_byte,
            start_index: index,
            end_index,
            captures: Vec::new(),
            named_groups: IndexMap::new(),
        }
    }) {
        start_index = matched.end_index;
        matches.push(matched);
    }
    matches
}

fn current_time_millis() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn parse_date_timestamp_ms(value: &str) -> f64 {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|parsed| parsed.unix_timestamp_nanos() as f64 / 1_000_000.0)
        .unwrap_or(f64::NAN)
}

fn clamp_index(index: i64, len: usize) -> usize {
    index.max(0).min(len as i64) as usize
}

fn char_index_to_byte_index(value: &str, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(index)
        .map_or_else(|| value.len(), |(byte_index, _)| byte_index)
}

fn byte_index_to_char_index(value: &str, byte_index: usize) -> usize {
    value[..byte_index.min(value.len())].chars().count()
}

fn advance_char_index(value: &str, index: usize) -> usize {
    let total = value.chars().count();
    (index + 1).min(total)
}

fn find_char_subsequence(haystack: &[char], needle: &[char], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

fn find_string_pattern(value: &str, needle: &str, start: usize) -> Option<usize> {
    let haystack = value.chars().collect::<Vec<_>>();
    let needle = needle.chars().collect::<Vec<_>>();
    find_char_subsequence(&haystack, &needle, start)
}

fn split_string_by_pattern(value: &str, separator: Option<&str>, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let haystack = value.chars().collect::<Vec<_>>();
    let Some(separator) = separator else {
        return vec![value.to_string()];
    };
    let needle = separator.chars().collect::<Vec<_>>();
    if needle.is_empty() {
        return haystack
            .into_iter()
            .take(limit)
            .map(|ch| ch.to_string())
            .collect();
    }
    let mut parts = Vec::new();
    let mut start = 0usize;
    while parts.len() < limit {
        let Some(index) = find_char_subsequence(&haystack, &needle, start) else {
            break;
        };
        parts.push(haystack[start..index].iter().collect());
        start = index + needle.len();
        if parts.len() == limit {
            return parts;
        }
    }
    if parts.len() < limit {
        parts.push(haystack[start..].iter().collect());
    }
    parts
}

fn replace_first_string_match(value: &str, search: &str, replacement: &str) -> String {
    let haystack = value.chars().collect::<Vec<_>>();
    let needle = search.chars().collect::<Vec<_>>();
    let Some(index) = find_char_subsequence(&haystack, &needle, 0) else {
        return value.to_string();
    };
    if needle.is_empty() {
        return format!("{replacement}{value}");
    }
    let prefix = haystack[..index].iter().collect::<String>();
    let suffix = haystack[index + needle.len()..].iter().collect::<String>();
    format!("{prefix}{replacement}{suffix}")
}

fn replace_all_string_matches(value: &str, search: &str, replacement: &str) -> String {
    let haystack = value.chars().collect::<Vec<_>>();
    let needle = search.chars().collect::<Vec<_>>();
    if needle.is_empty() {
        let mut result = String::new();
        result.push_str(replacement);
        for ch in haystack {
            result.push(ch);
            result.push_str(replacement);
        }
        return result;
    }
    let mut result = String::new();
    let mut start = 0usize;
    while let Some(index) = find_char_subsequence(&haystack, &needle, start) {
        result.push_str(&haystack[start..index].iter().collect::<String>());
        result.push_str(replacement);
        start = index + needle.len();
    }
    result.push_str(&haystack[start..].iter().collect::<String>());
    result
}

fn expand_regexp_replacement_template(
    template: &str,
    input: &str,
    matched: &RegExpMatchData,
) -> String {
    let full_match = &input[matched.start_byte..matched.end_byte];
    let mut result = String::new();
    let chars = template.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] != '$' {
            result.push(chars[index]);
            index += 1;
            continue;
        }
        let Some(next) = chars.get(index + 1).copied() else {
            result.push('$');
            break;
        };
        match next {
            '$' => {
                result.push('$');
                index += 2;
            }
            '&' => {
                result.push_str(full_match);
                index += 2;
            }
            '`' => {
                result.push_str(&input[..matched.start_byte]);
                index += 2;
            }
            '\'' => {
                result.push_str(&input[matched.end_byte..]);
                index += 2;
            }
            '<' => {
                let mut end = index + 2;
                while end < chars.len() && chars[end] != '>' {
                    end += 1;
                }
                if end < chars.len() {
                    let name = chars[index + 2..end].iter().collect::<String>();
                    if let Some(value) = matched
                        .named_groups
                        .get(&name)
                        .and_then(|value| value.as_ref())
                    {
                        result.push_str(value);
                    }
                    index = end + 1;
                } else {
                    result.push('$');
                    index += 1;
                }
            }
            digit if digit.is_ascii_digit() => {
                let mut end = index + 2;
                while end < chars.len() && end < index + 3 && chars[end].is_ascii_digit() {
                    end += 1;
                }
                let capture = chars[index + 1..end].iter().collect::<String>();
                if let Ok(group) = capture.parse::<usize>()
                    && group > 0
                    && let Some(Some(value)) = matched.captures.get(group - 1)
                {
                    result.push_str(value);
                    index = end;
                    continue;
                }
                result.push('$');
                index += 1;
            }
            _ => {
                result.push('$');
                index += 1;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;
