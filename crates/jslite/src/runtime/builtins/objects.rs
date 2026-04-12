use super::*;

impl Runtime {
    fn object_helper_type_error() -> JsliteError {
        JsliteError::runtime(
            "TypeError: Object helpers currently only support plain objects and arrays",
        )
    }

    fn object_spread_type_error() -> JsliteError {
        JsliteError::runtime(
            "TypeError: object spread currently only supports plain objects and arrays",
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

    fn ensure_object_spread_source(&self, value: Value) -> JsliteResult<()> {
        match value {
            Value::Object(object) => match self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?
                .kind
            {
                ObjectKind::Plain => Ok(()),
                _ => Err(Self::object_spread_type_error()),
            },
            Value::Array(array) => {
                self.arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                Ok(())
            }
            _ => Err(Self::object_spread_type_error()),
        }
    }
    fn enumerable_keys(&mut self, value: Value) -> JsliteResult<Vec<String>> {
        match value {
            Value::Object(object) => {
                let (count, keys) = {
                    let object = self
                        .objects
                        .get(object)
                        .ok_or_else(|| JsliteError::runtime("object missing"))?;
                    (
                        object.properties.len(),
                        ordered_own_property_keys(&object.properties),
                    )
                };
                self.charge_native_helper_work(count)?;
                Ok(keys)
            }
            Value::Array(array) => {
                let (array_len, extra_len, extra_keys) = {
                    let array = self
                        .arrays
                        .get(array)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?;
                    (
                        array.elements.len(),
                        array.properties.len(),
                        ordered_own_property_keys(&array.properties),
                    )
                };
                let mut keys = (0..array_len)
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>();
                self.charge_native_helper_work(keys.len())?;
                self.charge_native_helper_work(extra_len)?;
                keys.extend(extra_keys);
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
                if let Some(index) = array_index_from_property_key(key) {
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

    pub(crate) fn copy_data_properties(
        &mut self,
        target: Value,
        source: Value,
    ) -> JsliteResult<()> {
        self.ensure_assign_target(target.clone())?;
        if matches!(source, Value::Null | Value::Undefined) {
            return Ok(());
        }

        self.ensure_object_spread_source(source.clone())?;
        let keys = self.enumerable_keys(source.clone())?;
        for key in keys {
            let value = self.enumerable_value(source.clone(), &key)?;
            self.set_property(target.clone(), Value::String(key), value)?;
        }
        Ok(())
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
                array_index_from_property_key(&key)
                    .is_some_and(|index| index < array.elements.len())
                    || array.properties.contains_key(&key)
            }
            _ => return Err(Self::object_helper_type_error()),
        };
        Ok(Value::Bool(has_key))
    }
}
