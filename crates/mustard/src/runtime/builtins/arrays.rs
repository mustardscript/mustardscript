use std::cmp::Ordering;

use super::*;

impl Runtime {
    pub(in crate::runtime) fn iterable_length_hint(
        &self,
        value: &Value,
    ) -> MustardResult<Option<usize>> {
        match value {
            Value::Array(array) => Ok(Some(
                self.arrays
                    .get(*array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .len(),
            )),
            Value::Map(map) => Ok(Some(
                self.maps
                    .get(*map)
                    .ok_or_else(|| MustardError::runtime("map missing"))?
                    .live_len,
            )),
            Value::Set(set) => Ok(Some(
                self.sets
                    .get(*set)
                    .ok_or_else(|| MustardError::runtime("set missing"))?
                    .live_len,
            )),
            Value::String(value) => Ok(Some(value.chars().count())),
            _ => Ok(None),
        }
    }

    pub(crate) fn call_array_ctor(&mut self, args: &[Value]) -> MustardResult<Value> {
        if args.len() == 1
            && let Value::Number(length) = args[0]
        {
            if !length.is_finite()
                || length < 0.0
                || length.fract() != 0.0
                || length > u32::MAX as f64
            {
                return Err(MustardError::runtime("RangeError: Invalid array length"));
            }
            let length = length as usize;
            self.ensure_array_slot_capacity(length)?;
            return Ok(Value::Array(
                self.insert_sparse_array(vec![None; length], IndexMap::new())?,
            ));
        }
        self.call_array_of(args)
    }

    fn array_slots(&self, array: ArrayKey) -> MustardResult<Vec<Option<Value>>> {
        Ok(self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .clone())
    }

    pub(in crate::runtime) fn array_entry_pair(
        &self,
        entry: Value,
        invalid_entry_message: &'static str,
    ) -> MustardResult<(Value, Value)> {
        let array = match entry {
            Value::Array(array) => array,
            _ => return Err(MustardError::runtime(invalid_entry_message)),
        };
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements;
        let key = elements
            .first()
            .cloned()
            .flatten()
            .unwrap_or(Value::Undefined);
        let value = elements
            .get(1)
            .cloned()
            .flatten()
            .unwrap_or(Value::Undefined);
        Ok((key, value))
    }

    pub(in crate::runtime) fn array_receiver(
        &self,
        value: Value,
        method: &str,
    ) -> MustardResult<ArrayKey> {
        match value {
            Value::Array(key) => Ok(key),
            _ => Err(MustardError::runtime(format!(
                "TypeError: Array.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn set_array_length(&mut self, array: ArrayKey, value: Value) -> MustardResult<()> {
        let requested = self.to_number(value)?;
        if !requested.is_finite()
            || requested < 0.0
            || (requested.fract() != 0.0 && requested != 0.0)
            || requested > u32::MAX as f64
        {
            return Err(MustardError::runtime("RangeError: Invalid array length"));
        }

        let new_length = if requested == 0.0 {
            0
        } else {
            requested as usize
        };

        let old_length = self.array_length(array)?;
        if new_length > old_length {
            self.ensure_array_slot_capacity(new_length - old_length)?;
        }
        self.charge_native_helper_work(old_length.abs_diff(new_length))?;
        let empty_slot_bytes = Self::array_slot_bytes(None);
        let (old_component_bytes, new_component_bytes) = {
            let array_ref = self
                .arrays
                .get_mut(array)
                .ok_or_else(|| MustardError::runtime("array missing"))?;
            let old_length = array_ref.elements.len();
            let removed_length_property_bytes = array_ref
                .properties
                .get("length")
                .map(|existing| Self::property_entry_bytes("length", existing))
                .unwrap_or(0);
            let removed_slots_bytes = if new_length < old_length {
                array_ref.elements[new_length..]
                    .iter()
                    .map(|value| Self::array_slot_bytes(value.as_ref()))
                    .sum::<usize>()
            } else {
                0
            };
            let added_slots_bytes = if new_length > old_length {
                new_length
                    .checked_sub(old_length)
                    .and_then(|added| added.checked_mul(empty_slot_bytes))
                    .ok_or_else(|| MustardError::runtime("array accounting overflow"))?
            } else {
                0
            };
            array_ref.elements.resize(new_length, None);
            array_ref.properties.shift_remove("length");
            (
                removed_length_property_bytes
                    .checked_add(removed_slots_bytes)
                    .ok_or_else(|| MustardError::runtime("array accounting overflow"))?,
                added_slots_bytes,
            )
        };
        self.apply_array_component_delta(array, old_component_bytes, new_component_bytes)
    }

    pub(crate) fn call_array_of(&mut self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Array(
            self.insert_array(args.to_vec(), IndexMap::new())?,
        ))
    }

    pub(crate) fn call_array_from(&mut self, args: &[Value]) -> MustardResult<Value> {
        self.with_temporary_roots(args, |runtime| runtime.array_from_inner(args))
    }

    fn array_from_inner(&mut self, args: &[Value]) -> MustardResult<Value> {
        let source = args.first().cloned().unwrap_or(Value::Undefined);
        let map_fn = match args.get(1).cloned() {
            Some(Value::Undefined) | None => None,
            Some(value) if self.is_callable_value(&value)? => Some(value),
            Some(_) => {
                return Err(MustardError::runtime(
                    "TypeError: Array.from expects a callable map function",
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
        let (iterator, length_hint) = if let Some(iterable) = iterable {
            let length = self.iterable_length_hint(&iterable)?;
            (Some(self.create_iterator(iterable)?), length)
        } else {
            let length = self.array_like_length(source.clone())?;
            (None, Some(length))
        };
        let roots = iterator.iter().cloned().collect::<Vec<_>>();
        self.with_temporary_roots(&roots, |runtime| {
            if let Some(length) = length_hint { runtime.ensure_array_slot_capacity(length)?; }
            let result = runtime.insert_array(Vec::new(), IndexMap::new())?;
            runtime.with_temporary_roots(&[Value::Array(result)], |runtime| {
                let mut index = 0usize;
                loop {
                    runtime.charge_native_helper_work(1)?;
                    let value = if let Some(iterator) = &iterator {
                        let (value, done) = runtime.iterator_next(iterator.clone())?;
                        if done { break; }
                        value
                    } else {
                        if index >= length_hint.expect("array-like length") { break; }
                        runtime.get_property_by_key(source.clone(), &index.to_string(), false)?
                    };
                    let mapped = if let Some(map_fn) = &map_fn {
                        runtime.with_temporary_roots(std::slice::from_ref(&value), |runtime| runtime.call_callback(
                            map_fn.clone(), this_arg.clone(), &[value.clone(), Value::Number(index as f64)],
                            CallbackCallOptions {
                                non_callable_message: "TypeError: Array.from expects a callable map function",
                                host_suspension_message: "TypeError: Array.from mapping does not support host suspensions",
                                unsettled_message: "synchronous Array.from mapping did not settle",
                                allow_host_suspension: false,
                                allow_pending_promise_result: true,
                            },
                        ))?
                    } else { value };
                    runtime.push_array_element(result, Some(mapped))?;
                    index += 1;
                }
                Ok(Value::Array(result))
            })
        })
    }

    pub(in crate::runtime) fn array_like_length(&self, value: Value) -> MustardResult<usize> {
        let length = self.get_property_by_key(value, "length", false)?;
        let length = self.to_integer(length)?.max(0) as u64;
        if length > u32::MAX as u64 {
            return Err(MustardError::runtime("RangeError: Invalid array length"));
        }
        Ok(length as usize)
    }

    pub(in crate::runtime) fn ensure_array_slot_capacity(
        &mut self,
        length: usize,
    ) -> MustardResult<()> {
        if length > u32::MAX as usize {
            return Err(MustardError::runtime("RangeError: Invalid array length"));
        }
        let bytes = length
            .checked_mul(Self::array_slot_bytes(None))
            .ok_or_else(|| limit_error("heap limit exceeded"))?;
        self.ensure_heap_capacity(bytes)
    }

    pub(crate) fn call_array_push(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "push")?;
        self.extend_array_elements(array, args.iter().cloned().map(Some))?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        Ok(Value::Number(length as f64))
    }

    pub(crate) fn call_array_pop(&mut self, this_value: Value) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "pop")?;
        let value = self.pop_array_element(array)?.unwrap_or(Value::Undefined);
        Ok(value)
    }

    pub(crate) fn call_array_slice(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "slice")?;
        let elements = self.array_slots(array)?;
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
        Ok(Value::Array(self.insert_sparse_array(
            elements[start..end].to_vec(),
            IndexMap::new(),
        )?))
    }

    pub(crate) fn call_array_splice(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let mut roots = args.to_vec();
        roots.push(this_value.clone());
        self.with_temporary_roots(&roots, |runtime| {
            runtime.array_splice_inner(this_value, args)
        })
    }

    fn array_splice_inner(&mut self, this_value: Value, args: &[Value]) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "splice")?;
        if args.is_empty() {
            return Ok(Value::Array(
                self.insert_array(Vec::new(), IndexMap::new())?,
            ));
        }

        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        let start = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?,
            length,
        );
        let delete_count = if args.len() == 1 {
            length - start
        } else {
            clamp_index(self.to_integer(args[1].clone())?, length - start)
        };
        let inserted = args.get(2..).unwrap_or(&[]);
        let new_length = length
            .checked_sub(delete_count)
            .and_then(|length| length.checked_add(inserted.len()))
            .ok_or_else(|| limit_error("heap limit exceeded"))?;
        if new_length > u32::MAX as usize {
            return Err(MustardError::runtime("RangeError: Invalid array length"));
        }
        if new_length > length {
            self.ensure_array_slot_capacity(new_length - length)?;
        }
        self.charge_native_helper_work(length.saturating_add(inserted.len()))?;
        let inserted_bytes = inserted
            .iter()
            .map(|value| Self::array_slot_bytes(Some(value)))
            .sum::<usize>();
        let (removed, removed_bytes) = {
            let array_ref = self
                .arrays
                .get_mut(array)
                .ok_or_else(|| MustardError::runtime("array missing"))?;
            let removed = array_ref
                .elements
                .splice(
                    start..start + delete_count,
                    inserted.iter().cloned().map(Some),
                )
                .collect::<Vec<Option<Value>>>();
            let removed_bytes = removed
                .iter()
                .map(|value| Self::array_slot_bytes(value.as_ref()))
                .sum::<usize>();
            (removed, removed_bytes)
        };
        let removed_roots = removed.iter().flatten().cloned().collect::<Vec<_>>();
        self.with_temporary_roots(&removed_roots, |runtime| {
            if removed_bytes != inserted_bytes {
                runtime.apply_array_component_delta(array, removed_bytes, inserted_bytes)?;
            }
            Ok(Value::Array(
                runtime.insert_sparse_array(removed, IndexMap::new())?,
            ))
        })
    }

    pub(crate) fn call_array_concat(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "concat")?;
        let mut elements = self.array_slots(array)?;
        for value in args {
            match value {
                Value::Array(other) => {
                    let other = self
                        .arrays
                        .get(*other)
                        .ok_or_else(|| MustardError::runtime("array missing"))?;
                    elements.extend(other.elements.iter().cloned());
                }
                other => elements.push(Some(other.clone())),
            }
        }
        Ok(Value::Array(
            self.insert_sparse_array(elements, IndexMap::new())?,
        ))
    }

    pub(crate) fn call_array_at(&self, this_value: Value, args: &[Value]) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "at")?;
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
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
            Ok(elements[index as usize].clone().unwrap_or(Value::Undefined))
        }
    }

    fn normalized_flat_depth(&self, args: &[Value]) -> MustardResult<i64> {
        match args.first() {
            None | Some(Value::Undefined) => Ok(1),
            Some(value) => Ok(self.to_integer(value.clone())?.max(0)),
        }
    }

    fn collect_flattened_value(
        &self,
        flattened: &mut Vec<Value>,
        value: Value,
        depth: i64,
    ) -> MustardResult<()> {
        if depth > 0
            && let Value::Array(array) = value
        {
            let elements = self.array_slots(array)?;
            for element in elements.into_iter().flatten() {
                self.collect_flattened_value(flattened, element, depth - 1)?;
            }
            return Ok(());
        }
        flattened.push(value);
        Ok(())
    }

    pub(crate) fn call_array_flat(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "flat")?;
        let depth = self.normalized_flat_depth(args)?;
        let elements = self.array_slots(array)?;
        let mut flattened = Vec::new();
        for element in elements.into_iter().flatten() {
            self.collect_flattened_value(&mut flattened, element, depth)?;
        }
        Ok(Value::Array(self.insert_array(flattened, IndexMap::new())?))
    }

    pub(crate) fn call_array_to_string(&mut self, this_value: Value) -> MustardResult<Value> {
        if matches!(this_value, Value::Null | Value::Undefined) {
            return Err(MustardError::runtime(
                "TypeError: Array.toString requires a non-nullish receiver",
            ));
        }
        self.with_temporary_roots(std::slice::from_ref(&this_value), |runtime| {
            let join = runtime.get_property_by_key(this_value.clone(), "join", false)?;
            if !runtime.is_callable_value(&join)? {
                return runtime.call_object_to_string(this_value.clone());
            }
            runtime.call_callback(join, this_value.clone(), &[], CallbackCallOptions {
                non_callable_message: "TypeError: join is not callable",
                host_suspension_message: "TypeError: Array.toString does not support synchronous host suspensions",
                unsettled_message: "synchronous Array.toString callback did not settle",
                allow_host_suspension: false,
                allow_pending_promise_result: true,
            })
        })
    }

    pub(crate) fn call_array_join(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "join")?;
        let separator = match args.first() {
            None | Some(Value::Undefined) => ",".to_string(),
            Some(value) => self.to_string(value.clone())?,
        };
        let mut work = 0;
        let result = self.stringify_array(array, &separator, &mut Vec::new(), &mut work)?;
        self.charge_native_helper_work(work)?;
        Ok(Value::String(result))
    }

    pub(crate) fn call_array_includes(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "includes")?;
        let search = args.first().cloned().unwrap_or(Value::Undefined);
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements;
        let start = normalize_search_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        Ok(Value::Bool(elements.iter().skip(start).any(|value| {
            same_value_zero(&value.clone().unwrap_or(Value::Undefined), &search)
        })))
    }

    pub(crate) fn call_array_index_of(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "indexOf")?;
        let search = args.first().cloned().unwrap_or(Value::Undefined);
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements;
        let start = normalize_search_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        let index = elements
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, value)| {
                value
                    .as_ref()
                    .is_some_and(|value| strict_equal(value, &search))
            })
            .map(|(index, _)| index as f64)
            .unwrap_or(-1.0);
        Ok(Value::Number(index))
    }

    pub(crate) fn call_array_last_index_of(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "lastIndexOf")?;
        let search = args.first().cloned().unwrap_or(Value::Undefined);
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements;
        if elements.is_empty() {
            return Ok(Value::Number(-1.0));
        }
        let start = match args.get(1) {
            Some(value) => {
                let index = self.to_integer(value.clone())?;
                if index < 0 {
                    let adjusted = elements.len() as i64 + index;
                    if adjusted < 0 {
                        return Ok(Value::Number(-1.0));
                    }
                    adjusted as usize
                } else {
                    (index as usize).min(elements.len() - 1)
                }
            }
            None => elements.len() - 1,
        };
        let index = elements
            .iter()
            .enumerate()
            .take(start + 1)
            .rev()
            .find(|(_, value)| {
                value
                    .as_ref()
                    .is_some_and(|value| strict_equal(value, &search))
            })
            .map(|(index, _)| index as f64)
            .unwrap_or(-1.0);
        Ok(Value::Number(index))
    }

    pub(crate) fn call_array_reverse(&mut self, this_value: Value) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "reverse")?;
        self.arrays
            .get_mut(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .reverse();
        Ok(Value::Array(array))
    }

    pub(crate) fn call_array_fill(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "fill")?;
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        let start = normalize_relative_bound(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            length,
        );
        let end = normalize_relative_bound(
            match args.get(2) {
                Some(value) => self.to_integer(value.clone())?,
                None => length as i64,
            },
            length,
        );
        let new_slot_bytes = Self::array_slot_bytes(Some(&value));
        let (old_component_bytes, new_component_bytes) = {
            let elements = &mut self
                .arrays
                .get_mut(array)
                .ok_or_else(|| MustardError::runtime("array missing"))?
                .elements;
            let mut old_component_bytes = 0usize;
            let mut new_component_bytes = 0usize;
            for slot in elements.iter_mut().take(end).skip(start) {
                old_component_bytes = old_component_bytes
                    .checked_add(Self::array_slot_bytes(slot.as_ref()))
                    .ok_or_else(|| MustardError::runtime("array accounting overflow"))?;
                new_component_bytes = new_component_bytes
                    .checked_add(new_slot_bytes)
                    .ok_or_else(|| MustardError::runtime("array accounting overflow"))?;
                *slot = Some(value.clone());
            }
            (old_component_bytes, new_component_bytes)
        };
        if old_component_bytes != new_component_bytes {
            self.apply_array_component_delta(array, old_component_bytes, new_component_bytes)?;
        }
        Ok(Value::Array(array))
    }

    fn sort_compare(
        &mut self,
        comparator: Option<Value>,
        left: Value,
        right: Value,
    ) -> MustardResult<Ordering> {
        self.charge_native_helper_work(1)?;
        if matches!(left, Value::Undefined) || matches!(right, Value::Undefined) {
            return Ok(match (left, right) {
                (Value::Undefined, Value::Undefined) => Ordering::Equal,
                (Value::Undefined, _) => Ordering::Greater,
                _ => Ordering::Less,
            });
        }
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
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "sort")?;
        let comparator = match args.first().cloned() {
            Some(Value::Undefined) | None => None,
            Some(value) if self.is_callable_value(&value)? => Some(value),
            Some(_) => {
                return Err(MustardError::runtime(
                    "TypeError: Array.prototype.sort expects a callable comparator",
                ));
            }
        };
        if comparator.is_some() {
            self.record_comparator_sort_invocation();
        }
        let mut roots = vec![Value::Array(array)];
        if let Some(comparator) = &comparator {
            roots.push(comparator.clone());
        }
        self.with_temporary_roots(&roots, |runtime| {
            let length = runtime.array_length(array)?;
            let present_roots = runtime
                .array_slots(array)?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            runtime.with_temporary_roots(&present_roots, |runtime| {
                let mut present = present_roots.clone();
                for index in 1..present.len() {
                    runtime.charge_native_helper_work(1)?;
                    let current = present[index].clone();
                    let mut position = index;
                    while position > 0
                        && runtime.sort_compare(
                            comparator.clone(),
                            current.clone(),
                            present[position - 1].clone(),
                        )? == Ordering::Less
                    {
                        present[position] = present[position - 1].clone();
                        position -= 1;
                    }
                    present[position] = current;
                }
                for (index, value) in present.iter().enumerate() {
                    runtime.charge_native_helper_work(1)?;
                    runtime.set_array_element_at(array, index, value.clone())?;
                }
                for index in present.len()..length {
                    runtime.charge_native_helper_work(1)?;
                    runtime.delete_property_by_key(Value::Array(array), &index.to_string())?;
                }
                Ok(Value::Array(array))
            })
        })
    }

    pub(crate) fn call_array_values(&mut self, this_value: Value) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::Array(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    pub(crate) fn call_array_keys(&mut self, this_value: Value) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::ArrayKeys(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    pub(crate) fn call_array_entries(&mut self, this_value: Value) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::ArrayEntries(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    fn array_callback(&self, args: &[Value], method: &str) -> MustardResult<(Value, Value)> {
        let callback = args.first().cloned().unwrap_or(Value::Undefined);
        if !is_callable(&callback) {
            return Err(MustardError::runtime(format!(
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
    ) -> MustardResult<Value> {
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

    fn array_callback_value(&self, array: ArrayKey, index: usize) -> MustardResult<Value> {
        self.array_value_at(array, index)
    }

    fn append_flat_map_value(&mut self, result: ArrayKey, value: Value) -> MustardResult<()> {
        match value {
            Value::Array(array) => {
                let elements = self.array_slots(array)?;
                self.extend_array_elements(result, elements.into_iter().flatten().map(Some))?;
            }
            other => {
                self.push_array_element(result, Some(other))?;
            }
        }
        Ok(())
    }

    pub(crate) fn call_array_for_each(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "forEach")?;
        let (callback, this_arg) = self.array_callback(args, "forEach")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
            if !self.array_has_index(array, index)? {
                continue;
            }
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
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "map")?;
        let (callback, this_arg) = self.array_callback(args, "map")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        let result = self.insert_sparse_array(vec![None; length], IndexMap::new())?;
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
                    if !runtime.array_has_index(array, index)? {
                        continue;
                    }
                    let value = runtime.array_callback_value(array, index)?;
                    let mapped = runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[value, Value::Number(index as f64), Value::Array(array)],
                    )?;
                    runtime.set_array_element_at(result, index, mapped)?;
                }
                Ok(Value::Array(result))
            },
        )
    }

    pub(crate) fn call_array_filter(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "filter")?;
        let (callback, this_arg) = self.array_callback(args, "filter")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
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
                    if !runtime.array_has_index(array, index)? {
                        continue;
                    }
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
                        runtime.push_array_element(result, Some(value))?;
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
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "find")?;
        let (callback, this_arg) = self.array_callback(args, "find")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
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
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "findIndex")?;
        let (callback, this_arg) = self.array_callback(args, "findIndex")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
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
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "some")?;
        let (callback, this_arg) = self.array_callback(args, "some")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
            if !self.array_has_index(array, index)? {
                continue;
            }
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
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "every")?;
        let (callback, this_arg) = self.array_callback(args, "every")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        for index in 0..length {
            self.charge_native_helper_work(1)?;
            if !self.array_has_index(array, index)? {
                continue;
            }
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

    pub(crate) fn call_array_flat_map(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "flatMap")?;
        let (callback, this_arg) = self.array_callback(args, "flatMap")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
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
                    if !runtime.array_has_index(array, index)? {
                        continue;
                    }
                    let value = runtime.array_callback_value(array, index)?;
                    let mapped = runtime.call_array_callback(
                        callback.clone(),
                        this_arg.clone(),
                        &[value, Value::Number(index as f64), Value::Array(array)],
                    )?;
                    runtime.append_flat_map_value(result, mapped)?;
                }
                Ok(Value::Array(result))
            },
        )
    }

    pub(crate) fn call_array_reduce(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "reduce")?;
        let callback = args.first().cloned().unwrap_or(Value::Undefined);
        if !is_callable(&callback) {
            return Err(MustardError::runtime(
                "TypeError: Array.prototype.reduce expects a callable callback",
            ));
        }
        let this_arg = Value::Undefined;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        let (mut accumulator, start_index) = match args.get(1).cloned() {
            Some(initial) => (initial, 0),
            None => {
                let Some(index) = (0..length).find(|index| {
                    self.array_has_index(array, *index)
                        .expect("array existence should be stable during reduce")
                }) else {
                    return Err(MustardError::runtime(
                        "TypeError: Array.prototype.reduce requires an initial value for empty arrays",
                    ));
                };
                (self.array_callback_value(array, index)?, index + 1)
            }
        };
        for index in start_index..length {
            self.charge_native_helper_work(1)?;
            if !self.array_has_index(array, index)? {
                continue;
            }
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

    pub(crate) fn call_array_reduce_right(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "reduceRight")?;
        let callback = args.first().cloned().unwrap_or(Value::Undefined);
        if !is_callable(&callback) {
            return Err(MustardError::runtime(
                "TypeError: Array.prototype.reduceRight expects a callable callback",
            ));
        }
        let this_arg = Value::Undefined;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        let (mut accumulator, mut index) = match args.get(1).cloned() {
            Some(initial) => (initial, length as isize - 1),
            None => {
                let Some(found_index) = (0..length).rev().find(|index| {
                    self.array_has_index(array, *index)
                        .expect("array existence should be stable during reduceRight")
                }) else {
                    return Err(MustardError::runtime(
                        "TypeError: Array.prototype.reduceRight requires an initial value for empty arrays",
                    ));
                };
                (
                    self.array_callback_value(array, found_index)?,
                    found_index as isize - 1,
                )
            }
        };
        while index >= 0 {
            let current_index = index as usize;
            self.charge_native_helper_work(1)?;
            if self.array_has_index(array, current_index)? {
                let value = self.array_callback_value(array, current_index)?;
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
                                Value::Number(current_index as f64),
                                Value::Array(array),
                            ],
                        )
                    },
                )?;
            }
            index -= 1;
        }
        Ok(accumulator)
    }

    pub(crate) fn call_array_find_last(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "findLast")?;
        let (callback, this_arg) = self.array_callback(args, "findLast")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        for index in (0..length).rev() {
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

    pub(crate) fn call_array_find_last_index(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(this_value, "findLastIndex")?;
        let (callback, this_arg) = self.array_callback(args, "findLastIndex")?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len();
        for index in (0..length).rev() {
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
}

impl Runtime {
    pub(crate) fn call_array_shift(&mut self, receiver: Value) -> MustardResult<Value> {
        let array = self.array_receiver(receiver, "shift")?;
        self.charge_native_helper_work(1)?;
        let (removed, bytes) = {
            let elements = &mut self
                .arrays
                .get_mut(array)
                .ok_or_else(|| MustardError::runtime("array missing"))?
                .elements;
            if elements.is_empty() {
                return Ok(Value::Undefined);
            }
            // Move the remaining slots in place. No guest values are cloned and
            // no throwaway splice-result array is allocated or rooted.
            let removed = elements.remove(0);
            let bytes = Self::array_slot_bytes(removed.as_ref());
            (removed, bytes)
        };
        self.apply_array_component_delta(array, bytes, 0)?;
        Ok(removed.unwrap_or(Value::Undefined))
    }

    pub(crate) fn call_array_unshift(
        &mut self,
        receiver: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(receiver.clone(), "unshift")?;
        if args.is_empty() {
            return Ok(Value::Number(self.array_length(array)? as f64));
        }
        let mut roots = args.to_vec();
        roots.push(receiver.clone());
        self.with_temporary_roots(&roots, |runtime| {
            let mut splice_args = vec![Value::Number(0.0), Value::Number(0.0)];
            splice_args.extend_from_slice(args);
            runtime.call_array_splice(receiver.clone(), &splice_args)?;
            Ok(Value::Number(runtime.array_length(array)? as f64))
        })
    }

    pub(crate) fn call_array_copy(
        &mut self,
        receiver: Value,
        args: &[Value],
        method: BuiltinFunction,
    ) -> MustardResult<Value> {
        let name = match method {
            BuiltinFunction::ArrayToSorted => "toSorted",
            BuiltinFunction::ArrayToReversed => "toReversed",
            BuiltinFunction::ArrayToSpliced => "toSpliced",
            BuiltinFunction::ArrayWith => "with",
            _ => unreachable!(),
        };
        let array = self.array_receiver(receiver.clone(), name)?;
        let mut roots = args.to_vec();
        roots.push(receiver);
        self.with_temporary_roots(&roots, |runtime| {
            let length = runtime.array_length(array)?;
            runtime.ensure_array_slot_capacity(length)?;
            runtime.charge_native_helper_work(length)?;
            let elements = runtime
                .array_slots(array)?
                .into_iter()
                .map(|value| value.unwrap_or(Value::Undefined))
                .collect();
            let result = runtime.insert_array(elements, IndexMap::new())?;
            runtime.with_temporary_roots(&[Value::Array(result)], |runtime| {
                match method {
                    BuiltinFunction::ArrayToReversed => {
                        runtime.call_array_reverse(Value::Array(result))?;
                    }
                    BuiltinFunction::ArrayToSorted => {
                        runtime.call_array_sort(Value::Array(result), args)?;
                    }
                    BuiltinFunction::ArrayToSpliced => {
                        runtime.call_array_splice(Value::Array(result), args)?;
                    }
                    BuiltinFunction::ArrayWith => {
                        let index = runtime
                            .to_integer(args.first().cloned().unwrap_or(Value::Undefined))?;
                        let index = if index < 0 {
                            (length as i64).saturating_add(index)
                        } else {
                            index
                        };
                        if index < 0 || index >= length as i64 {
                            return Err(MustardError::runtime(
                                "RangeError: Array.prototype.with index out of range",
                            ));
                        }
                        runtime.set_array_element_at(
                            result,
                            index as usize,
                            args.get(1).cloned().unwrap_or(Value::Undefined),
                        )?;
                    }
                    _ => unreachable!(),
                }
                Ok(Value::Array(result))
            })
        })
    }

    pub(crate) fn call_array_copy_within(
        &mut self,
        receiver: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let array = self.array_receiver(receiver.clone(), "copyWithin")?;
        let length = self.array_length(array)?;
        let target = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?,
            length,
        );
        let start = normalize_relative_bound(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Undefined))?,
            length,
        );
        let end = match args.get(2) {
            None | Some(Value::Undefined) => length,
            Some(value) => normalize_relative_bound(self.to_integer(value.clone())?, length),
        };
        let count = end.saturating_sub(start).min(length - target);
        self.charge_native_helper_work(count)?;
        self.with_temporary_roots(std::slice::from_ref(&receiver), |runtime| {
            let backward = target > start && target < start + count;
            for offset in 0..count {
                let offset = if backward { count - offset - 1 } else { offset };
                if runtime.array_has_index(array, start + offset)? {
                    let value = runtime.array_value_at(array, start + offset)?;
                    runtime.set_array_element_at(array, target + offset, value)?;
                } else {
                    runtime.delete_property_by_key(
                        Value::Array(array),
                        &(target + offset).to_string(),
                    )?;
                }
            }
            Ok(receiver.clone())
        })
    }
}
