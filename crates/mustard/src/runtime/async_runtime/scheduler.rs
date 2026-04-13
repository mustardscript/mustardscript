use super::*;

impl Runtime {
    pub(in crate::runtime) fn activate_microtask(
        &mut self,
        job: MicrotaskJob,
    ) -> MustardResult<()> {
        if !self.frames.is_empty() {
            return Err(MustardError::runtime(
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
                            MustardError::runtime("async continuation resumed without frames")
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

    pub(in crate::runtime) fn has_pending_async_work(&self) -> bool {
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

    pub(in crate::runtime) fn suspend_host_request(
        &mut self,
        request: PendingHostCall,
    ) -> ExecutionStep {
        let capability = request.capability.clone();
        let args = request.args.clone();
        self.suspended_host_call = Some(request);
        self.snapshot_nonce = next_snapshot_nonce();
        ExecutionStep::Suspended(Box::new(Suspension {
            capability,
            args,
            snapshot: ExecutionSnapshot {
                runtime: self.clone(),
            },
        }))
    }

    pub(in crate::runtime) fn process_idle_state(
        &mut self,
    ) -> MustardResult<Option<ExecutionStep>> {
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
                    None => Err(MustardError::runtime(
                        "async root promise could not make progress",
                    )),
                },
                value => {
                    if self.has_pending_async_work() {
                        return Err(MustardError::runtime(
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
            return Err(MustardError::runtime(
                "async execution became idle before producing a root result",
            ));
        }
        Err(MustardError::runtime("vm lost all frames"))
    }

    pub(in crate::runtime) fn resume(
        &mut self,
        payload: ResumePayload,
    ) -> MustardResult<ExecutionStep> {
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
                        return Err(self.annotate_runtime_error(MustardError::runtime(
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
                    return Err(self.annotate_runtime_error(MustardError::runtime(
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
