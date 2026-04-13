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
    ) -> MustardResult<Option<PromiseOutcome>> {
        let promise = self
            .promises
            .get(promise)
            .ok_or_else(|| MustardError::runtime("promise missing"))?;
        Ok(match &promise.state {
            PromiseState::Pending => None,
            PromiseState::Fulfilled(value) => Some(PromiseOutcome::Fulfilled(value.clone())),
            PromiseState::Rejected(rejection) => Some(PromiseOutcome::Rejected(rejection.clone())),
        })
    }

    fn resolve_promise_from_settled_source(
        &mut self,
        promise: PromiseKey,
        source: PromiseKey,
    ) -> MustardResult<()> {
        let outcome = self
            .promise_outcome(source)?
            .ok_or_else(|| MustardError::runtime("promise source pending"))?;
        self.resolve_promise_with_outcome(promise, outcome)
    }

    pub(in crate::runtime) fn coerce_to_promise(
        &mut self,
        value: Value,
    ) -> MustardResult<PromiseKey> {
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
    ) -> MustardResult<()> {
        let added_bytes = Self::promise_awaiters_bytes(1);
        self.promises
            .get_mut(promise)
            .ok_or_else(|| MustardError::runtime("promise missing"))?
            .awaiters
            .push(continuation);
        self.apply_promise_component_delta(promise, 0, added_bytes)
    }

    pub(in crate::runtime) fn attach_dependent(
        &mut self,
        promise: PromiseKey,
        dependent: PromiseKey,
    ) -> MustardResult<()> {
        let added_bytes = Self::promise_dependents_bytes(1);
        self.promises
            .get_mut(promise)
            .ok_or_else(|| MustardError::runtime("promise missing"))?
            .dependents
            .push(dependent);
        self.apply_promise_component_delta(promise, 0, added_bytes)
    }

    pub(in crate::runtime) fn attach_promise_reaction(
        &mut self,
        promise: PromiseKey,
        reaction: PromiseReaction,
    ) -> MustardResult<()> {
        let is_pending = matches!(
            self.promises
                .get(promise)
                .ok_or_else(|| MustardError::runtime("promise missing"))?
                .state,
            PromiseState::Pending
        );
        if let PromiseReaction::Combinator {
            target,
            index,
            kind,
        } = reaction
            && !is_pending
        {
            return self.schedule_promise_combinator(
                target,
                index,
                kind,
                PromiseCombinatorInput::Promise(promise),
            );
        }
        if is_pending {
            let added_bytes = Self::promise_reaction_bytes(&reaction);
            self.promises
                .get_mut(promise)
                .ok_or_else(|| MustardError::runtime("promise missing"))?
                .reactions
                .push(reaction);
            self.apply_promise_component_delta(promise, 0, added_bytes)
        } else {
            self.schedule_promise_reaction(reaction, promise)
        }
    }

    pub(in crate::runtime) fn schedule_promise_reaction(
        &mut self,
        reaction: PromiseReaction,
        source: PromiseKey,
    ) -> MustardResult<()> {
        self.microtasks
            .push_back(MicrotaskJob::PromiseReaction { reaction, source });
        Ok(())
    }

    pub(in crate::runtime) fn schedule_promise_combinator(
        &mut self,
        target: PromiseKey,
        index: usize,
        kind: PromiseCombinatorKind,
        input: PromiseCombinatorInput,
    ) -> MustardResult<()> {
        self.microtasks.push_back(MicrotaskJob::PromiseCombinator {
            target,
            index,
            kind,
            input,
        });
        Ok(())
    }

    pub(in crate::runtime) fn settle_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> MustardResult<()> {
        let settled_state = match outcome {
            PromiseOutcome::Fulfilled(value) => PromiseState::Fulfilled(value),
            PromiseOutcome::Rejected(rejection) => PromiseState::Rejected(rejection),
        };
        let (old_dynamic_bytes, new_dynamic_bytes, awaiters, dependents, reactions) = {
            let promise_ref = self
                .promises
                .get_mut(promise)
                .ok_or_else(|| MustardError::runtime("promise missing"))?;
            if !matches!(promise_ref.state, PromiseState::Pending) {
                return Ok(());
            }
            let old_dynamic_bytes = Self::promise_dynamic_bytes(promise_ref);
            promise_ref.state = settled_state;
            promise_ref.driver = None;
            let awaiters = std::mem::take(&mut promise_ref.awaiters);
            let dependents = std::mem::take(&mut promise_ref.dependents);
            let reactions = std::mem::take(&mut promise_ref.reactions);
            let new_dynamic_bytes = Self::promise_state_bytes(&promise_ref.state);
            (
                old_dynamic_bytes,
                new_dynamic_bytes,
                awaiters,
                dependents,
                reactions,
            )
        };
        self.apply_promise_component_delta(promise, old_dynamic_bytes, new_dynamic_bytes)?;
        for continuation in awaiters {
            self.microtasks.push_back(MicrotaskJob::ResumeAsync {
                continuation,
                source: promise,
            });
        }
        for dependent in dependents {
            self.resolve_promise_from_settled_source(dependent, promise)?;
        }
        for reaction in reactions {
            match reaction {
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind,
                } => self.schedule_promise_combinator(
                    target,
                    index,
                    kind,
                    PromiseCombinatorInput::Promise(promise),
                )?,
                other => self.schedule_promise_reaction(other, promise)?,
            }
        }
        Ok(())
    }

    pub(in crate::runtime) fn resolve_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> MustardResult<()> {
        match outcome {
            PromiseOutcome::Fulfilled(value) => self.resolve_promise(promise, value),
            PromiseOutcome::Rejected(rejection) => self.reject_promise(promise, rejection),
        }
    }

    pub(in crate::runtime) fn resolve_promise(
        &mut self,
        promise: PromiseKey,
        value: Value,
    ) -> MustardResult<()> {
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
                .ok_or_else(|| MustardError::runtime("promise missing"))?
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
            self.replace_promise_driver(
                promise,
                Some(PromiseDriver::Thenable {
                    value: value.clone(),
                }),
            )?;
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
    ) -> MustardResult<()> {
        self.settle_promise_with_outcome(promise, PromiseOutcome::Rejected(rejection))
    }

    pub(in crate::runtime) fn suspend_async_await(&mut self, value: Value) -> MustardResult<()> {
        let boundary = self.current_async_boundary_index().ok_or_else(|| {
            MustardError::runtime("await is only supported inside async functions")
        })?;
        let promise = self.coerce_to_promise(value)?;
        let continuation = AsyncContinuation {
            frames: self.frames.split_off(boundary),
        };
        let is_settled = !matches!(
            self.promises
                .get(promise)
                .ok_or_else(|| MustardError::runtime("promise missing"))?
                .state,
            PromiseState::Pending
        );
        if is_settled {
            self.microtasks.push_back(MicrotaskJob::ResumeAsync {
                continuation,
                source: promise,
            });
        } else {
            self.attach_awaiter(promise, continuation)?;
        }
        Ok(())
    }
}
