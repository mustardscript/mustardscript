use super::*;

struct SetLikeRecord {
    object: Value,
    size: f64,
    has: Value,
    keys: Value,
}

impl Runtime {
    fn set_like_record(&mut self, object: Value) -> MustardResult<SetLikeRecord> {
        if matches!(
            object,
            Value::Undefined
                | Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::BigInt(_)
                | Value::String(_)
        ) {
            return Err(MustardError::runtime(
                "TypeError: Set methods require a set-like object",
            ));
        }
        let raw_size = self.get_property_static(object.clone(), "size", false)?;
        let size = self.coerce_number(raw_size)?;
        if size.is_nan() {
            return Err(MustardError::runtime(
                "TypeError: set-like size must not be NaN",
            ));
        }
        let size = size.trunc();
        if size < 0.0 {
            return Err(MustardError::runtime(
                "RangeError: set-like size must be nonnegative",
            ));
        }
        let has = self.get_property_static(object.clone(), "has", false)?;
        if !self.is_callable_value(&has)? {
            return Err(MustardError::runtime(
                "TypeError: set-like has must be callable",
            ));
        }
        let keys = self.get_property_static(object.clone(), "keys", false)?;
        if !self.is_callable_value(&keys)? {
            return Err(MustardError::runtime(
                "TypeError: set-like keys must be callable",
            ));
        }
        Ok(SetLikeRecord {
            object,
            size,
            has,
            keys,
        })
    }

    fn call_set_like(
        &mut self,
        record: &SetLikeRecord,
        keys: bool,
        args: &[Value],
    ) -> MustardResult<Value> {
        self.call_callback(
            if keys { record.keys.clone() } else { record.has.clone() }, record.object.clone(), args,
            CallbackCallOptions {
                non_callable_message: "TypeError: set-like method must be callable",
                host_suspension_message: "TypeError: Set methods do not support synchronous host suspensions",
                unsettled_message: "synchronous set-like method did not settle",
                allow_host_suspension: false,
                allow_pending_promise_result: true,
            },
        )
    }

    fn set_like_keys(&mut self, record: &SetLikeRecord) -> MustardResult<Value> {
        let iterator = self.call_set_like(record, true, &[])?;
        if !matches!(iterator, Value::Iterator(_)) {
            return Err(MustardError::runtime(
                "TypeError: set-like keys must return a supported native iterator",
            ));
        }
        Ok(iterator)
    }

    fn copy_set_data(&mut self, set: SetKey) -> MustardResult<SetKey> {
        let result = self.insert_set(Vec::new())?;
        self.with_temporary_roots(&[Value::Set(result)], |runtime| {
            let mut next_index = 0;
            let mut epoch = runtime
                .sets
                .get(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?
                .clear_epoch;
            while let Some(value) =
                runtime.next_set_value_from_state(set, &mut next_index, &mut epoch)?
            {
                runtime.set_add(result, value)?;
            }
            Ok(result)
        })
    }

    pub(crate) fn call_set_algebra(
        &mut self,
        method: BuiltinFunction,
        receiver: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        use BuiltinFunction::*;
        let set = self.set_receiver(receiver, Self::builtin_function_name(method))?;
        let other = self.set_like_record(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.with_temporary_roots(
            &[other.object.clone(), other.has.clone(), other.keys.clone()],
            |runtime| {
                let size = runtime
                    .sets
                    .get(set)
                    .ok_or_else(|| MustardError::runtime("set missing"))?
                    .live_len as f64;
                if (method == SetIsSubsetOf && size > other.size)
                    || (method == SetIsSupersetOf && size < other.size)
                {
                    return Ok(Value::Bool(false));
                }
                let use_has = method == SetIsSubsetOf
                    || (matches!(method, SetIntersection | SetDifference | SetIsDisjointFrom)
                        && size <= other.size);
                // Difference copies before calling keys; union/symmetricDifference
                // call keys first, so mutations made by keys are included in the copy.
                let initial = if method == SetDifference {
                    Value::Set(runtime.copy_set_data(set)?)
                } else {
                    Value::Undefined
                };
                runtime.with_temporary_roots(std::slice::from_ref(&initial), |runtime| {
                    let iterator = if use_has {
                        runtime.create_iterator(if method == SetDifference {
                            initial.clone()
                        } else {
                            Value::Set(set)
                        })?
                    } else {
                        runtime.set_like_keys(&other)?
                    };
                    runtime.with_temporary_roots(std::slice::from_ref(&iterator), |runtime| {
                        let result = match method {
                            SetUnion | SetSymmetricDifference => {
                                Value::Set(runtime.copy_set_data(set)?)
                            }
                            SetDifference => initial.clone(),
                            SetIntersection => Value::Set(runtime.insert_set(Vec::new())?),
                            _ => Value::Undefined,
                        };
                        runtime.with_temporary_roots(std::slice::from_ref(&result), |runtime| {
                            loop {
                                runtime.charge_native_helper_work(1)?;
                                let (value, done) = runtime.iterator_next(iterator.clone())?;
                                if done {
                                    break;
                                }
                                let early = runtime.with_temporary_roots(
                                    std::slice::from_ref(&value),
                                    |runtime| {
                                        let present = if use_has {
                                            let answer = runtime.call_set_like(
                                                &other,
                                                false,
                                                std::slice::from_ref(&value),
                                            )?;
                                            is_truthy(&answer)
                                        } else {
                                            runtime.set_contains(set, &value)?
                                        };
                                        if (method == SetIsSubsetOf || method == SetIsSupersetOf)
                                            && !present
                                        {
                                            return Ok(Some(false));
                                        }
                                        if method == SetIsDisjointFrom && present {
                                            return Ok(Some(false));
                                        }
                                        if let Value::Set(output) = result {
                                            match method {
                                                SetUnion => {
                                                    runtime.set_add(output, value.clone())?
                                                }
                                                SetIntersection if present => {
                                                    runtime.set_add(output, value.clone())?
                                                }
                                                SetDifference if !use_has || present => {
                                                    runtime.set_delete(output, &value)?;
                                                }
                                                SetSymmetricDifference if present => {
                                                    runtime.set_delete(output, &value)?;
                                                }
                                                SetSymmetricDifference => {
                                                    runtime.set_add(output, value.clone())?
                                                }
                                                _ => {}
                                            }
                                        }
                                        Ok(None)
                                    },
                                )?;
                                if let Some(answer) = early {
                                    return Ok(Value::Bool(answer));
                                }
                            }
                            Ok(if matches!(result, Value::Undefined) {
                                Value::Bool(true)
                            } else {
                                result.clone()
                            })
                        })
                    })
                })
            },
        )
    }
}
