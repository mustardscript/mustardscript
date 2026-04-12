use super::*;

impl Runtime {
    fn promise_combinator_slot_len(
        promise: &PromiseObject,
        kind: PromiseCombinatorKind,
    ) -> JsliteResult<Option<usize>> {
        match (kind, promise.driver.as_ref()) {
            (PromiseCombinatorKind::Race, _) => Ok(None),
            (PromiseCombinatorKind::All, Some(PromiseDriver::All { values, .. })) => {
                Ok(Some(values.len()))
            }
            (
                PromiseCombinatorKind::AllSettled,
                Some(PromiseDriver::AllSettled { results, .. }),
            ) => Ok(Some(results.len())),
            (PromiseCombinatorKind::Any, Some(PromiseDriver::Any { reasons, .. })) => {
                Ok(Some(reasons.len()))
            }
            (_, Some(_)) => Err(JsliteError::runtime("promise combinator kind mismatch")),
            (_, None) => Err(JsliteError::runtime("promise combinator state missing")),
        }
    }

    pub(in crate::runtime) fn promise_reaction_target(
        &self,
        reaction: &PromiseReaction,
    ) -> PromiseKey {
        match reaction {
            PromiseReaction::Then { target, .. }
            | PromiseReaction::Finally { target, .. }
            | PromiseReaction::FinallyPassThrough { target, .. }
            | PromiseReaction::Combinator { target, .. } => *target,
        }
    }

    pub(in crate::runtime) fn invoke_promise_handler(
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

    pub(in crate::runtime) fn make_promise_all_settled_result(
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

    pub(in crate::runtime) fn make_aggregate_error(
        &mut self,
        reasons: Vec<Value>,
    ) -> JsliteResult<Value> {
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

    pub(in crate::runtime) fn activate_promise_combinator(
        &mut self,
        target: PromiseKey,
        index: usize,
        kind: PromiseCombinatorKind,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        if self.promise_outcome(target)?.is_some() {
            return Ok(());
        }
        let promise = self
            .promises
            .get(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?;
        if let Some(len) = Self::promise_combinator_slot_len(promise, kind)?
            && index >= len
        {
            return Err(serialization_error(
                "snapshot validation failed: promise combinator index out of range",
            ));
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

    pub(in crate::runtime) fn activate_promise_reaction(
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
}
