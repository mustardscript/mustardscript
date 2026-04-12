use std::cmp::Ordering;

use super::*;

impl Runtime {
    fn array_receiver(&self, value: Value, method: &str) -> JsliteResult<ArrayKey> {
        match value {
            Value::Array(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Array.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn call_array_of(&mut self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Array(
            self.insert_array(args.to_vec(), IndexMap::new())?,
        ))
    }

    pub(crate) fn call_array_from(&mut self, args: &[Value]) -> JsliteResult<Value> {
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
                runtime.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_push(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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

    pub(crate) fn call_array_pop(&mut self, this_value: Value) -> JsliteResult<Value> {
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

    pub(crate) fn call_array_slice(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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

    pub(crate) fn call_array_concat(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "concat")?;
        let mut elements = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .clone();
        for value in args {
            match value {
                Value::Array(other) => {
                    let other = self
                        .arrays
                        .get(*other)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?;
                    elements.extend(other.elements.iter().cloned());
                }
                other => elements.push(other.clone()),
            }
        }
        Ok(Value::Array(self.insert_array(elements, IndexMap::new())?))
    }

    pub(crate) fn call_array_at(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "at")?;
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements;
        let index = self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?;
        let index = if index < 0 {
            elements.len() as i64 + index
        } else {
            index
        };
        if index < 0 || index >= elements.len() as i64 {
            Ok(Value::Undefined)
        } else {
            Ok(elements[index as usize].clone())
        }
    }

    pub(crate) fn call_array_join(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "join")?;
        let separator = match args.first() {
            Some(value) => self.to_string(value.clone())?,
            None => ",".to_string(),
        };
        let elements = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .clone();
        let mut parts = Vec::with_capacity(elements.len());
        for value in &elements {
            self.charge_native_helper_work(1)?;
            parts.push(match value {
                Value::Undefined | Value::Null => String::new(),
                other => self.to_string(other.clone())?,
            });
        }
        Ok(Value::String(parts.join(&separator)))
    }

    pub(crate) fn call_array_includes(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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

    pub(crate) fn call_array_index_of(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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
        self.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_sort(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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
                runtime.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::Array(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    pub(crate) fn call_array_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::ArrayKeys(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    pub(crate) fn call_array_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
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

    pub(crate) fn call_array_for_each(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "forEach")?;
        let (callback, this_arg) = self.array_callback(args, "forEach")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_map(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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
                    runtime.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_filter(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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
                    runtime.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_find(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "find")?;
        let (callback, this_arg) = self.array_callback(args, "find")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_find_index(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "findIndex")?;
        let (callback, this_arg) = self.array_callback(args, "findIndex")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_some(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "some")?;
        let (callback, this_arg) = self.array_callback(args, "some")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_every(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "every")?;
        let (callback, this_arg) = self.array_callback(args, "every")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
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

    pub(crate) fn call_array_reduce(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
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
            self.charge_native_helper_work(1)?;
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
}
