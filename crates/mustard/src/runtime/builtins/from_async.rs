use super::*;

impl Runtime {
    pub(in crate::runtime) fn promise_resolvers(
        &mut self,
        target: PromiseKey,
    ) -> MustardResult<(Value, Value)> {
        self.with_temporary_roots(&[Value::Promise(target)], |runtime| {
            let guard = runtime.insert_object(
                IndexMap::from([
                    ("promise".to_string(), Value::Promise(target)),
                    ("resolved".to_string(), Value::Bool(false)),
                ]),
                ObjectKind::Plain,
            )?;
            Ok((
                Value::BuiltinFunction(BuiltinFunction::PromiseResolveOnce(guard)),
                Value::BuiltinFunction(BuiltinFunction::PromiseRejectOnce(guard)),
            ))
        })
    }

    pub(crate) fn call_promise_with_resolvers(&mut self, receiver: Value) -> MustardResult<Value> {
        if !matches!(
            receiver,
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor)
        ) {
            return Err(MustardError::runtime(
                "TypeError: Promise.withResolvers requires the supported Promise constructor",
            ));
        }
        let target = self.insert_promise(PromiseState::Pending)?;
        let (resolve, reject) = self.promise_resolvers(target)?;
        self.with_temporary_roots(
            &[Value::Promise(target), resolve.clone(), reject.clone()],
            |runtime| {
                Ok(Value::Object(runtime.insert_object(
                    IndexMap::from([
                        ("promise".to_string(), Value::Promise(target)),
                        ("resolve".to_string(), resolve),
                        ("reject".to_string(), reject),
                    ]),
                    ObjectKind::Plain,
                )?))
            },
        )
    }

    pub(crate) fn call_promise_once(
        &mut self,
        guard: ObjectKey,
        rejected: bool,
        args: &[Value],
    ) -> MustardResult<Value> {
        self.with_temporary_roots(&[Value::Object(guard)], |runtime| {
            if matches!(
                runtime.get_property_static(Value::Object(guard), "resolved", false)?,
                Value::Bool(true)
            ) {
                return Ok(Value::Undefined);
            }
            let Value::Promise(target) =
                runtime.get_property_static(Value::Object(guard), "promise", false)?
            else {
                return Err(MustardError::runtime("invalid Promise resolver state"));
            };
            // Mark before adoption, not settlement: a later reject must not win
            // while the first resolve is waiting on another promise.
            runtime.set_property_static(Value::Object(guard), "resolved", Value::Bool(true))?;
            let value = args.first().cloned().unwrap_or(Value::Undefined);
            if rejected {
                runtime.reject_promise(
                    target,
                    PromiseRejection {
                        value,
                        span: None,
                        traceback: runtime.traceback_snapshots(),
                    },
                )?;
            } else {
                runtime.resolve_promise(target, value)?;
            }
            Ok(Value::Undefined)
        })
    }

    pub(crate) fn call_array_from_async(&mut self, args: &[Value]) -> MustardResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        self.with_temporary_roots(&[Value::Promise(target)], |runtime| {
            if let Err(error) = runtime.setup_array_from_async(target, args) {
                runtime.reject_promise_from_setup_error(target, error)?;
            }
            Ok(Value::Promise(target))
        })
    }

    fn setup_array_from_async(&mut self, target: PromiseKey, args: &[Value]) -> MustardResult<()> {
        let source = args.first().cloned().unwrap_or(Value::Undefined);
        let mapper = match args.get(1).cloned() {
            Some(Value::Undefined) | None => None,
            Some(value) if self.is_callable_value(&value)? => Some(value),
            _ => {
                return Err(MustardError::runtime(
                    "TypeError: Array.fromAsync expects a callable map function",
                ));
            }
        };
        let this_arg = args.get(2).cloned().unwrap_or(Value::Undefined);
        let iterable = match &source {
            Value::Array(_)
            | Value::String(_)
            | Value::Map(_)
            | Value::Set(_)
            | Value::Iterator(_) => Some(source.clone()),
            Value::Object(object) => match &self
                .objects
                .get(*object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::StringObject(string) => Some(Value::String(string.clone())),
                _ => None,
            },
            _ => None,
        };
        let (iterator, length) = if let Some(iterable) = iterable {
            let length = self.iterable_length_hint(&iterable)?.unwrap_or(0);
            let Value::Iterator(iterator) = self.create_iterator(iterable)? else {
                unreachable!()
            };
            (Some(iterator), length)
        } else {
            (None, self.array_like_length(source.clone())?)
        };
        let roots = iterator
            .map(Value::Iterator)
            .into_iter()
            .collect::<Vec<_>>();
        self.with_temporary_roots(&roots, |runtime| {
            runtime.ensure_array_slot_capacity(length)?;
            let result = runtime.insert_array(Vec::new(), IndexMap::new())?;
            runtime.with_temporary_roots(&[Value::Array(result)], |runtime| {
                runtime.replace_promise_driver(
                    target,
                    Some(PromiseDriver::ArrayFromAsync(Box::new(
                        ArrayFromAsyncState {
                            source: if iterator.is_some() {
                                Value::Undefined
                            } else {
                                source
                            },
                            iterator,
                            length,
                            index: 0,
                            result,
                            mapper,
                            this_arg,
                            waiting: None,
                            phase: ArrayFromAsyncPhase::Value,
                            done: false,
                        },
                    ))),
                )?;
                runtime.advance_array_from_async(target)
            })
        })
    }

    fn array_from_async_state(&self, target: PromiseKey) -> MustardResult<ArrayFromAsyncState> {
        match self
            .promises
            .get(target)
            .and_then(|promise| promise.driver.as_ref())
        {
            Some(PromiseDriver::ArrayFromAsync(state)) => Ok(state.as_ref().clone()),
            _ => Err(MustardError::runtime("Array.fromAsync driver missing")),
        }
    }

    fn await_array_from_async(
        &mut self,
        target: PromiseKey,
        source: PromiseKey,
        phase: ArrayFromAsyncPhase,
    ) -> MustardResult<()> {
        let Some(PromiseDriver::ArrayFromAsync(state)) = self
            .promises
            .get_mut(target)
            .and_then(|p| p.driver.as_mut())
        else {
            return Err(MustardError::runtime("Array.fromAsync driver missing"));
        };
        state.waiting = Some(source);
        state.phase = phase;
        self.attach_promise_reaction(source, PromiseReaction::ArrayFromAsync { target, phase })
    }

    fn advance_array_from_async(&mut self, target: PromiseKey) -> MustardResult<()> {
        self.charge_native_helper_work(1)?;
        let state = self.array_from_async_state(target)?;
        let (value, done) = if let Some(iterator) = state.iterator {
            self.iterator_next(Value::Iterator(iterator))?
        } else if state.index >= state.length {
            return self.resolve_promise(target, Value::Array(state.result));
        } else {
            (
                self.get_property_static(state.source, &state.index.to_string(), false)?,
                false,
            )
        };
        self.with_temporary_roots(std::slice::from_ref(&value), |runtime| {
            let source = runtime.coerce_to_promise(value.clone())?;
            if let Some(PromiseDriver::ArrayFromAsync(state)) = runtime
                .promises
                .get_mut(target)
                .and_then(|p| p.driver.as_mut())
            {
                state.done = done;
            }
            runtime.with_temporary_roots(&[Value::Promise(source)], |runtime| {
                runtime.await_array_from_async(
                    target,
                    source,
                    if state.iterator.is_some() {
                        ArrayFromAsyncPhase::IteratorValue
                    } else {
                        ArrayFromAsyncPhase::Value
                    },
                )
            })
        })
    }

    fn invoke_from_async_mapper(
        &mut self,
        mapper: Value,
        this_arg: Value,
        args: &[Value],
        target: PromiseKey,
    ) -> MustardResult<()> {
        match self.call_callable(mapper, this_arg, args)? {
            RunState::Completed(value) | RunState::StartedAsync(value) => {
                self.resolve_promise(target, value)
            }
            RunState::PushedFrame => {
                self.frames
                    .last_mut()
                    .ok_or_else(|| MustardError::runtime("mapper frame missing"))?
                    .async_promise = Some(target);
                Ok(())
            }
            RunState::Suspended {
                capability,
                args,
                resume_behavior,
            } => {
                let outstanding =
                    self.pending_host_calls.len() + usize::from(self.suspended_host_call.is_some());
                if outstanding >= self.limits.max_outstanding_host_calls {
                    return Err(limit_error("outstanding host-call limit exhausted"));
                }
                self.pending_host_calls.push_back(PendingHostCall {
                    capability,
                    args,
                    promise: Some(target),
                    resume_behavior,
                    traceback: self.traceback_snapshots(),
                });
                Ok(())
            }
        }
    }

    pub(in crate::runtime) fn activate_array_from_async(
        &mut self,
        target: PromiseKey,
        phase: ArrayFromAsyncPhase,
        outcome: PromiseOutcome,
    ) -> MustardResult<()> {
        let mut roots = vec![Value::Promise(target)];
        roots.push(match &outcome {
            PromiseOutcome::Fulfilled(value) => value.clone(),
            PromiseOutcome::Rejected(rejection) => rejection.value.clone(),
        });
        self.with_temporary_roots(&roots, |runtime| {
            if runtime.promise_outcome(target)?.is_some() {
                return Ok(());
            }
            let result = (|| {
                runtime.charge_native_helper_work(1)?;
                let state = runtime.array_from_async_state(target)?;
                if state.phase != phase {
                    return Err(MustardError::runtime("Array.fromAsync phase mismatch"));
                }
                let value = match outcome {
                    PromiseOutcome::Rejected(rejection) => {
                        return runtime.reject_promise(target, rejection);
                    }
                    PromiseOutcome::Fulfilled(value) => value,
                };
                if phase == ArrayFromAsyncPhase::IteratorValue {
                    // Async-from-sync iterator continuation precedes the await
                    // of its iterator result, including the final done result.
                    let source = runtime.insert_promise(PromiseState::Fulfilled(value))?;
                    return runtime.with_temporary_roots(&[Value::Promise(source)], |runtime| {
                        runtime.await_array_from_async(target, source, ArrayFromAsyncPhase::Value)
                    });
                }
                if state.done {
                    return runtime.resolve_promise(target, Value::Array(state.result));
                }
                if phase == ArrayFromAsyncPhase::Value
                    && let Some(mapper) = state.mapper
                {
                    let mapped = runtime.insert_promise(PromiseState::Pending)?;
                    return runtime.with_temporary_roots(&[Value::Promise(mapped)], |runtime| {
                        runtime.await_array_from_async(
                            target,
                            mapped,
                            ArrayFromAsyncPhase::Mapper,
                        )?;
                        runtime.invoke_from_async_mapper(
                            mapper,
                            state.this_arg,
                            &[value, Value::Number(state.index as f64)],
                            mapped,
                        )
                    });
                }
                runtime.push_array_element(state.result, Some(value))?;
                if let Some(PromiseDriver::ArrayFromAsync(state)) = runtime
                    .promises
                    .get_mut(target)
                    .and_then(|p| p.driver.as_mut())
                {
                    state.index += 1;
                }
                runtime.advance_array_from_async(target)
            })();
            if let Err(error) = result {
                runtime.reject_promise_from_setup_error(target, error)?;
            }
            Ok(())
        })
    }
}
