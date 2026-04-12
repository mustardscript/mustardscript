use super::*;

impl Runtime {
    pub(super) fn current_async_boundary_index(&self) -> Option<usize> {
        self.frames
            .iter()
            .rposition(|frame| frame.async_promise.is_some())
    }

    pub(super) fn promise_outcome(
        &self,
        promise: PromiseKey,
    ) -> JsliteResult<Option<PromiseOutcome>> {
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

    pub(super) fn coerce_to_promise(&mut self, value: Value) -> JsliteResult<PromiseKey> {
        match value {
            Value::Promise(promise) => Ok(promise),
            other => self.insert_promise(PromiseState::Fulfilled(other)),
        }
    }

    pub(super) fn attach_awaiter(
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

    pub(super) fn attach_dependent(
        &mut self,
        promise: PromiseKey,
        dependent: PromiseKey,
    ) -> JsliteResult<()> {
        self.promises
            .get_mut(promise)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .dependents
            .push(dependent);
        self.refresh_promise_accounting(promise)
    }

    pub(super) fn attach_promise_reaction(
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

    pub(super) fn schedule_promise_reaction(
        &mut self,
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        self.microtasks
            .push_back(MicrotaskJob::PromiseReaction { reaction, outcome });
        Ok(())
    }

    pub(super) fn settle_promise_with_outcome(
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

    pub(super) fn resolve_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        match outcome {
            PromiseOutcome::Fulfilled(value) => self.resolve_promise(promise, value),
            PromiseOutcome::Rejected(rejection) => self.reject_promise(promise, rejection),
        }
    }

    pub(super) fn resolve_promise(
        &mut self,
        promise: PromiseKey,
        value: Value,
    ) -> JsliteResult<()> {
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

    pub(super) fn reject_promise(
        &mut self,
        promise: PromiseKey,
        rejection: PromiseRejection,
    ) -> JsliteResult<()> {
        self.settle_promise_with_outcome(promise, PromiseOutcome::Rejected(rejection))
    }

    pub(super) fn suspend_async_await(&mut self, value: Value) -> JsliteResult<()> {
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

    pub(super) fn promise_reaction_target(&self, reaction: &PromiseReaction) -> PromiseKey {
        match reaction {
            PromiseReaction::Then { target, .. }
            | PromiseReaction::Finally { target, .. }
            | PromiseReaction::FinallyPassThrough { target, .. }
            | PromiseReaction::Combinator { target, .. } => *target,
        }
    }

    pub(super) fn invoke_promise_handler(
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
                    promise: Some(promise),
                    resume_behavior: ResumeBehavior::Value,
                    traceback: self.traceback_snapshots(),
                });
                Ok(())
            }
            _ => Err(JsliteError::runtime("value is not callable")),
        }
    }

    pub(super) fn make_promise_all_settled_result(
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

    pub(super) fn make_aggregate_error(&mut self, reasons: Vec<Value>) -> JsliteResult<Value> {
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

    pub(super) fn activate_promise_combinator(
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

    pub(super) fn activate_promise_reaction(
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

    pub(super) fn activate_microtask(&mut self, job: MicrotaskJob) -> JsliteResult<()> {
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

    pub(super) fn has_pending_async_work(&self) -> bool {
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

    pub(super) fn suspend_host_request(&mut self, request: PendingHostCall) -> ExecutionStep {
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

    pub(super) fn process_idle_state(&mut self) -> JsliteResult<Option<ExecutionStep>> {
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

    pub(super) fn resume(&mut self, payload: ResumePayload) -> JsliteResult<ExecutionStep> {
        if let Err(error) = self.check_cancellation() {
            if let Some(request) = self.suspended_host_call.as_ref() {
                return Err(error.with_traceback(self.compose_traceback(&request.traceback)));
            }
            return Err(self.annotate_runtime_error(error));
        }
        self.collect_garbage()
            .map_err(|error| self.annotate_runtime_error(error))?;
        if let Some(request) = self.suspended_host_call.take() {
            if let Some(promise) = request.promise {
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
                self.resolve_promise_with_outcome(promise, outcome)
                    .map_err(|error| self.annotate_runtime_error(error))?;
                return self.run();
            }

            return match payload {
                ResumePayload::Value(value) => {
                    let value = match request.resume_behavior {
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
                    self.run()
                }
                ResumePayload::Error(error) => {
                    self.pending_resume_behavior = ResumeBehavior::Value;
                    let value = self
                        .value_from_host_error(error)
                        .map_err(|error| self.annotate_runtime_error(error))?;
                    match self.raise_exception(value, None) {
                        Ok(StepAction::Continue) => self.run(),
                        Ok(StepAction::Return(step)) => Ok(step),
                        Err(error) => Err(self.annotate_runtime_error(error)),
                    }
                }
                ResumePayload::Cancelled => {
                    Err(self.annotate_runtime_error(limit_error("execution cancelled")))
                }
            };
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
}
