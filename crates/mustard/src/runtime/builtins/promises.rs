use super::*;

pub(crate) struct PromiseSetupPolicy<'a> {
    pub(crate) non_callable_message: &'a str,
    pub(crate) host_suspension_message: &'a str,
    pub(crate) async_message: &'a str,
}

impl Runtime {
    fn reject_promise_from_setup_error(
        &mut self,
        target: PromiseKey,
        error: MustardError,
    ) -> MustardResult<()> {
        match error {
            MustardError::Message {
                kind: DiagnosticKind::Runtime,
                message,
                ..
            } if message == super::super::INTERNAL_CALLBACK_THROW_MARKER => {
                let rejection = self.pending_internal_exception.take().ok_or_else(|| {
                    MustardError::runtime("missing internal callback exception state")
                })?;
                self.reject_promise(target, rejection)
            }
            other => self.reject_promise_from_error(target, other),
        }
    }

    pub(crate) fn promise_settler(&self, target: PromiseKey, rejected: bool) -> Value {
        Value::BuiltinFunction(if rejected {
            BuiltinFunction::PromiseRejectFunction(target)
        } else {
            BuiltinFunction::PromiseResolveFunction(target)
        })
    }

    pub(crate) fn promise_thenable_handler(&self, value: &Value) -> MustardResult<Option<Value>> {
        match value {
            Value::Object(_) | Value::Array(_) => {
                let then =
                    self.get_property(value.clone(), Value::String("then".to_string()), false)?;
                if is_callable(&then) {
                    Ok(Some(then))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn call_promise_setup_callback(
        &mut self,
        target: PromiseKey,
        callback: Value,
        this_arg: Value,
        args: &[Value],
        policy: PromiseSetupPolicy<'_>,
    ) -> MustardResult<()> {
        let result = match callback {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| MustardError::runtime("closure not found"))?;
                let function = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .ok_or_else(|| MustardError::runtime("function not found"))?;
                if function.is_async {
                    Err(MustardError::runtime(policy.async_message))
                } else {
                    let env = self.new_env(Some(closure.env))?;
                    let outcome = self.insert_promise(PromiseState::Pending)?;
                    let base_depth = self.frames.len();
                    self.push_frame(closure.function_id, env, args, this_arg, Some(outcome))?;
                    if let Err(error) =
                        self.run_until_frame_depth(base_depth, policy.host_suspension_message)
                    {
                        self.frames.truncate(base_depth);
                        self.suspended_host_call = None;
                        self.pending_resume_behavior = ResumeBehavior::Value;
                        return self.reject_promise_from_setup_error(target, error);
                    }
                    match self.promise_outcome(outcome)? {
                        Some(PromiseOutcome::Rejected(rejection)) => {
                            self.reject_promise(target, rejection)
                        }
                        Some(PromiseOutcome::Fulfilled(_)) | None => Ok(()),
                    }
                }
            }
            Value::BuiltinFunction(function) => {
                self.call_builtin(function, this_arg, args).map(|_| ())
            }
            Value::HostFunction(capability) => {
                match self.call_callable(Value::HostFunction(capability), this_arg, args) {
                    Ok(RunState::Completed(_) | RunState::StartedAsync(_)) => Ok(()),
                    Ok(RunState::PushedFrame) => Err(MustardError::runtime(
                        "promise setup callback unexpectedly pushed a frame",
                    )),
                    Ok(RunState::Suspended { .. }) => {
                        Err(MustardError::runtime(policy.host_suspension_message))
                    }
                    Err(error) => Err(error),
                }
            }
            _ => Err(MustardError::runtime(policy.non_callable_message)),
        };
        match result {
            Ok(()) => Ok(()),
            Err(error) => self.reject_promise_from_setup_error(target, error),
        }
    }

    pub(crate) fn construct_promise(&mut self, args: &[Value]) -> MustardResult<Value> {
        let executor = args.first().cloned().unwrap_or(Value::Undefined);
        if !is_callable(&executor) {
            return Err(MustardError::runtime(
                "TypeError: Promise constructor expects a callable executor",
            ));
        }
        let target = self.insert_promise(PromiseState::Pending)?;
        let resolve = self.promise_settler(target, false);
        let reject = self.promise_settler(target, true);
        self.call_promise_setup_callback(
            target,
            executor,
            Value::Undefined,
            &[resolve, reject],
            PromiseSetupPolicy {
                non_callable_message: "TypeError: Promise constructor expects a callable executor",
                host_suspension_message:
                    "TypeError: Promise executors do not support synchronous host suspensions",
                async_message:
                    "TypeError: Promise executors must be synchronous in the supported surface",
            },
        )?;
        Ok(Value::Promise(target))
    }

    fn promise_receiver(&self, value: Value, method: &str) -> MustardResult<PromiseKey> {
        match value {
            Value::Promise(key) => Ok(key),
            _ => Err(MustardError::runtime(format!(
                "TypeError: Promise.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn collect_iterable_values(&mut self, iterable: Value) -> MustardResult<Vec<Value>> {
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

    pub(crate) fn call_promise_then(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
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

    pub(crate) fn call_promise_catch(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
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

    pub(crate) fn call_promise_finally(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let promise = self.promise_receiver(this_value, "finally")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let callback = args.first().cloned().filter(is_callable);
        self.attach_promise_reaction(promise, PromiseReaction::Finally { target, callback })?;
        Ok(Value::Promise(target))
    }

    pub(crate) fn call_promise_all(&mut self, args: &[Value]) -> MustardResult<Value> {
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
            .ok_or_else(|| MustardError::runtime("promise missing"))?
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

    pub(crate) fn call_promise_race(&mut self, args: &[Value]) -> MustardResult<Value> {
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

    pub(crate) fn call_promise_any(&mut self, args: &[Value]) -> MustardResult<Value> {
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
            .ok_or_else(|| MustardError::runtime("promise missing"))?
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

    pub(crate) fn call_promise_all_settled(&mut self, args: &[Value]) -> MustardResult<Value> {
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
            .ok_or_else(|| MustardError::runtime("promise missing"))?
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
}
