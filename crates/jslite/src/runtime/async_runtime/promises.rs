use crate::runtime::builtins::PromiseSetupPolicy;

use super::*;

impl Runtime {
    pub(in crate::runtime) fn current_async_boundary_index(&self) -> Option<usize> {
        self.frames
            .iter()
            .rposition(|frame| frame.async_promise.is_some())
    }

    pub(in crate::runtime) fn promise_outcome(
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

    pub(in crate::runtime) fn coerce_to_promise(
        &mut self,
        value: Value,
    ) -> JsliteResult<PromiseKey> {
        match value {
            Value::Promise(promise) => Ok(promise),
            other => {
                let promise = self.insert_promise(PromiseState::Pending)?;
                self.resolve_promise(promise, other)?;
                Ok(promise)
            }
        }
    }

    pub(in crate::runtime) fn attach_awaiter(
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

    pub(in crate::runtime) fn attach_dependent(
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

    pub(in crate::runtime) fn attach_promise_reaction(
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

    pub(in crate::runtime) fn schedule_promise_reaction(
        &mut self,
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        self.microtasks
            .push_back(MicrotaskJob::PromiseReaction { reaction, outcome });
        Ok(())
    }

    pub(in crate::runtime) fn settle_promise_with_outcome(
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

    pub(in crate::runtime) fn resolve_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        match outcome {
            PromiseOutcome::Fulfilled(value) => self.resolve_promise(promise, value),
            PromiseOutcome::Rejected(rejection) => self.reject_promise(promise, rejection),
        }
    }

    pub(in crate::runtime) fn resolve_promise(
        &mut self,
        promise: PromiseKey,
        value: Value,
    ) -> JsliteResult<()> {
        if self.promise_outcome(promise)?.is_some() {
            return Ok(());
        }
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
        } else if let Some(then) = self.promise_thenable_handler(&value)? {
            let tracked_thenable = self
                .promises
                .get(promise)
                .ok_or_else(|| JsliteError::runtime("promise missing"))?
                .driver
                .as_ref()
                .and_then(|driver| match driver {
                    PromiseDriver::Thenable { value } => Some(value),
                    _ => None,
                });
            if tracked_thenable.is_some_and(|tracked| strict_equal(tracked, &value)) {
                let error_value = self
                    .value_from_runtime_message("TypeError: thenable cannot resolve to itself")?;
                return self.reject_promise(
                    promise,
                    PromiseRejection {
                        value: error_value,
                        span: None,
                        traceback: self.traceback_snapshots(),
                    },
                );
            }
            self.promises
                .get_mut(promise)
                .ok_or_else(|| JsliteError::runtime("promise missing"))?
                .driver = Some(PromiseDriver::Thenable {
                value: value.clone(),
            });
            self.refresh_promise_accounting(promise)?;
            let resolve = self.promise_settler(promise, false);
            let reject = self.promise_settler(promise, true);
            self.call_promise_setup_callback(
                promise,
                then,
                value,
                &[resolve, reject],
                PromiseSetupPolicy {
                    non_callable_message: "TypeError: adopted thenable `.then` must be callable",
                    host_suspension_message:
                        "TypeError: adopted thenables do not support synchronous host suspensions",
                    async_message:
                        "TypeError: adopted thenables must use synchronous `.then` handlers",
                },
            )
        } else {
            self.settle_promise_with_outcome(promise, PromiseOutcome::Fulfilled(value))
        }
    }

    pub(in crate::runtime) fn reject_promise(
        &mut self,
        promise: PromiseKey,
        rejection: PromiseRejection,
    ) -> JsliteResult<()> {
        self.settle_promise_with_outcome(promise, PromiseOutcome::Rejected(rejection))
    }

    pub(in crate::runtime) fn suspend_async_await(&mut self, value: Value) -> JsliteResult<()> {
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
}
