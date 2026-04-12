use super::*;

impl Runtime {
    fn object_helper_type_error() -> JsliteError {
        JsliteError::runtime(
            "TypeError: Object helpers currently only support plain objects and arrays",
        )
    }

    fn ensure_assign_target(&self, value: Value) -> JsliteResult<()> {
        match value {
            Value::Object(object) => match self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?
                .kind
            {
                ObjectKind::Plain => Ok(()),
                _ => Err(Self::object_helper_type_error()),
            },
            Value::Array(array) => {
                self.arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                Ok(())
            }
            _ => Err(Self::object_helper_type_error()),
        }
    }

    fn merge_property_keys(
        &mut self,
        source: &[String],
        target: &mut [String],
        start: usize,
        mid: usize,
        end: usize,
    ) -> JsliteResult<()> {
        let mut left = start;
        let mut right = mid;
        let mut write = start;

        while left < mid && right < end {
            self.charge_native_helper_work(1)?;
            if source[left] <= source[right] {
                target[write] = source[left].clone();
                left += 1;
            } else {
                target[write] = source[right].clone();
                right += 1;
            }
            write += 1;
        }

        while left < mid {
            self.charge_native_helper_work(1)?;
            target[write] = source[left].clone();
            left += 1;
            write += 1;
        }

        while right < end {
            self.charge_native_helper_work(1)?;
            target[write] = source[right].clone();
            right += 1;
            write += 1;
        }

        Ok(())
    }

    fn sort_property_keys(&mut self, keys: &mut Vec<String>) -> JsliteResult<()> {
        let len = keys.len();
        if len <= 1 {
            return Ok(());
        }

        let mut source = std::mem::take(keys);
        let mut target = source.clone();
        let mut width = 1usize;
        let mut source_is_current = true;

        while width < len {
            if source_is_current {
                let mut start = 0usize;
                while start < len {
                    let mid = (start + width).min(len);
                    let end = (start + width.saturating_mul(2)).min(len);
                    self.merge_property_keys(&source, &mut target, start, mid, end)?;
                    start += width.saturating_mul(2);
                }
            } else {
                let mut start = 0usize;
                while start < len {
                    let mid = (start + width).min(len);
                    let end = (start + width.saturating_mul(2)).min(len);
                    self.merge_property_keys(&target, &mut source, start, mid, end)?;
                    start += width.saturating_mul(2);
                }
            }
            width = width.saturating_mul(2);
            source_is_current = !source_is_current;
        }

        *keys = if source_is_current { source } else { target };
        Ok(())
    }

    fn enumerable_keys(&mut self, value: Value) -> JsliteResult<Vec<String>> {
        match value {
            Value::Object(object) => {
                let mut keys = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                self.charge_native_helper_work(keys.len())?;
                self.sort_property_keys(&mut keys)?;
                Ok(keys)
            }
            Value::Array(array) => {
                let (array_len, mut extra) = {
                    let array = self
                        .arrays
                        .get(array)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?;
                    (
                        array.elements.len(),
                        array.properties.keys().cloned().collect::<Vec<_>>(),
                    )
                };
                let mut keys = (0..array_len)
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>();
                self.charge_native_helper_work(keys.len())?;
                self.charge_native_helper_work(extra.len())?;
                self.sort_property_keys(&mut extra)?;
                keys.extend(extra);
                Ok(keys)
            }
            _ => Err(Self::object_helper_type_error()),
        }
    }

    fn enumerable_value(&self, target: Value, key: &str) -> JsliteResult<Value> {
        match target {
            Value::Object(object) => self
                .objects
                .get(object)
                .and_then(|object| object.properties.get(key))
                .cloned()
                .ok_or_else(|| JsliteError::runtime("object property missing")),
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                if let Ok(index) = key.parse::<usize>() {
                    Ok(array
                        .elements
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Undefined))
                } else {
                    array
                        .properties
                        .get(key)
                        .cloned()
                        .ok_or_else(|| JsliteError::runtime("array property missing"))
                }
            }
            _ => Err(Self::object_helper_type_error()),
        }
    }

    pub(crate) fn call_object_assign(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        self.ensure_assign_target(target.clone())?;

        for source in args.iter().skip(1).cloned() {
            if matches!(source, Value::Null | Value::Undefined) {
                continue;
            }
            self.ensure_assign_target(source.clone())?;
            let keys = self.enumerable_keys(source.clone())?;
            for key in keys {
                let value = self.enumerable_value(source.clone(), &key)?;
                self.set_property(target.clone(), Value::String(key), value)?;
            }
        }

        Ok(target)
    }

    pub(crate) fn reject_object_create(&self) -> JsliteResult<Value> {
        Err(JsliteError::runtime(
            "TypeError: Object.create is unsupported because prototype semantics are deferred",
        ))
    }

    pub(crate) fn reject_object_freeze(&self) -> JsliteResult<Value> {
        Err(JsliteError::runtime(
            "TypeError: Object.freeze is unsupported because property descriptor semantics are deferred",
        ))
    }

    pub(crate) fn reject_object_seal(&self) -> JsliteResult<Value> {
        Err(JsliteError::runtime(
            "TypeError: Object.seal is unsupported because property descriptor semantics are deferred",
        ))
    }

    pub(crate) fn call_object_keys(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self
            .enumerable_keys(target)?
            .into_iter()
            .map(Value::String)
            .collect();
        Ok(Value::Array(self.insert_array(keys, IndexMap::new())?))
    }

    pub(crate) fn call_object_from_entries(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        let iterator = self.create_iterator(iterable.clone())?;
        let result = self.insert_object(IndexMap::new(), ObjectKind::Plain)?;
        self.with_temporary_roots(
            &[iterable, iterator.clone(), Value::Object(result)],
            |runtime| {
                loop {
                    let (entry, done) = runtime.iterator_next(iterator.clone())?;
                    if done {
                        break;
                    }
                    let items = match entry {
                        Value::Array(array) => runtime
                            .arrays
                            .get(array)
                            .ok_or_else(|| JsliteError::runtime("array missing"))?
                            .elements
                            .clone(),
                        _ => {
                            return Err(JsliteError::runtime(
                                "TypeError: Object.fromEntries expects an iterable of [key, value] pairs",
                            ));
                        }
                    };
                    let key = runtime
                        .to_property_key(items.first().cloned().unwrap_or(Value::Undefined))?;
                    let value = items.get(1).cloned().unwrap_or(Value::Undefined);
                    runtime
                        .objects
                        .get_mut(result)
                        .ok_or_else(|| JsliteError::runtime("object missing"))?
                        .properties
                        .insert(key, value);
                    runtime.refresh_object_accounting(result)?;
                }
                Ok(Value::Object(result))
            },
        )
    }

    pub(crate) fn call_object_values(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self.enumerable_keys(target.clone())?;
        let mut values = Vec::with_capacity(keys.len());
        for key in keys {
            self.charge_native_helper_work(1)?;
            values.push(self.enumerable_value(target.clone(), &key)?);
        }
        Ok(Value::Array(self.insert_array(values, IndexMap::new())?))
    }

    pub(crate) fn call_object_entries(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self.enumerable_keys(target.clone())?;
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            self.charge_native_helper_work(1)?;
            let pair = self.insert_array(
                vec![
                    Value::String(key.clone()),
                    self.enumerable_value(target.clone(), &key)?,
                ],
                IndexMap::new(),
            )?;
            entries.push(Value::Array(pair));
        }
        Ok(Value::Array(self.insert_array(entries, IndexMap::new())?))
    }

    pub(crate) fn call_object_has_own(&self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let key = self.to_property_key(args.get(1).cloned().unwrap_or(Value::Undefined))?;
        let has_key = match target {
            Value::Object(object) => self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?
                .properties
                .contains_key(&key),
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                key.parse::<usize>()
                    .ok()
                    .is_some_and(|index| index < array.elements.len())
                    || array.properties.contains_key(&key)
            }
            _ => return Err(Self::object_helper_type_error()),
        };
        Ok(Value::Bool(has_key))
    }
}
