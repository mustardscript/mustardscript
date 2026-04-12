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
                ObjectKind::Plain
                | ObjectKind::FunctionPrototype(_)
                | ObjectKind::NumberObject(_)
                | ObjectKind::StringObject(_)
                | ObjectKind::BooleanObject(_) => Ok(()),
                _ => Err(Self::object_helper_type_error()),
            },
            Value::Array(array) => {
                self.arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                Ok(())
            }
            Value::Closure(closure) => {
                self.closures
                    .get(closure)
                    .ok_or_else(|| JsliteError::runtime("closure missing"))?;
                Ok(())
            }
            Value::BuiltinFunction(_) | Value::HostFunction(_) => Ok(()),
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
                    match &object.kind {
                        ObjectKind::StringObject(value) => {
                            let mut keys = (0..value.chars().count())
                                .map(|index| index.to_string())
                                .collect::<Vec<_>>();
                            keys.extend(ordered_own_property_keys(&object.properties));
                            (keys.len(), keys)
                        }
                        _ => (
                            object.properties.len(),
                            ordered_own_property_keys(&object.properties),
                        ),
                    }
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
                        array
                            .elements
                            .iter()
                            .enumerate()
                            .filter_map(|(index, value)| value.as_ref().map(|_| index.to_string()))
                            .collect::<Vec<_>>(),
                        array.properties.len(),
                        ordered_own_property_keys(&array.properties),
                    )
                };
                let mut keys = array_len;
                self.charge_native_helper_work(keys.len())?;
                self.charge_native_helper_work(extra_len)?;
                keys.extend(extra_keys);
                Ok(keys)
            }
            Value::Closure(closure) => {
                let keys = ordered_own_property_keys(
                    &self
                        .closures
                        .get(closure)
                        .ok_or_else(|| JsliteError::runtime("closure missing"))?
                        .properties,
                );
                self.charge_native_helper_work(keys.len())?;
                Ok(keys)
            }
            Value::BuiltinFunction(function) => {
                let keys = self.builtin_function_custom_keys(function)?;
                self.charge_native_helper_work(keys.len())?;
                Ok(keys)
            }
            Value::HostFunction(capability) => {
                let keys = self.host_function_custom_keys(&capability)?;
                self.charge_native_helper_work(keys.len())?;
                Ok(keys)
            }
            _ => Err(Self::object_helper_type_error()),
        }
    }

    fn enumerable_value(&self, target: Value, key: &str) -> JsliteResult<Value> {
        match target {
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?;
                if let ObjectKind::StringObject(value) = &object.kind
                    && let Some(index) = array_index_from_property_key(key)
                    && let Some(ch) = value.chars().nth(index)
                {
                    return Ok(Value::String(ch.to_string()));
                }
                object
                    .properties
                    .get(key)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("object property missing"))
            }
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
                        .flatten()
                        .unwrap_or(Value::Undefined))
                } else {
                    array
                        .properties
                        .get(key)
                        .cloned()
                        .ok_or_else(|| JsliteError::runtime("array property missing"))
                }
            }
            Value::Closure(closure) => self
                .closures
                .get(closure)
                .and_then(|closure| closure.properties.get(key))
                .cloned()
                .ok_or_else(|| JsliteError::runtime("closure property missing")),
            Value::BuiltinFunction(function) => self
                .builtin_function_custom_property(function, key)?
                .ok_or_else(|| JsliteError::runtime("builtin function property missing")),
            Value::HostFunction(capability) => self
                .host_function_custom_property(&capability, key)?
                .ok_or_else(|| JsliteError::runtime("host function property missing")),
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

    pub(crate) fn call_object_ctor(&mut self, args: &[Value]) -> JsliteResult<Value> {
        match args.first().cloned().unwrap_or(Value::Undefined) {
            Value::Undefined | Value::Null => Ok(Value::Object(
                self.insert_object(IndexMap::new(), ObjectKind::Plain)?,
            )),
            Value::Bool(value) => Ok(Value::Object(
                self.insert_object(IndexMap::new(), ObjectKind::BooleanObject(value))?,
            )),
            Value::Number(value) => Ok(Value::Object(
                self.insert_object(IndexMap::new(), ObjectKind::NumberObject(value))?,
            )),
            Value::String(value) => Ok(Value::Object(
                self.insert_object(IndexMap::new(), ObjectKind::StringObject(value))?,
            )),
            value @ (Value::Object(_)
            | Value::Array(_)
            | Value::Map(_)
            | Value::Set(_)
            | Value::Iterator(_)
            | Value::Promise(_)
            | Value::Closure(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_)) => Ok(value),
            Value::BigInt(_) => Err(JsliteError::runtime(
                "TypeError: BigInt values cannot be boxed with Object in the supported surface",
            )),
        }
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
                    let items: Vec<Value> = match entry {
                        Value::Array(array) => runtime
                            .arrays
                            .get(array)
                            .ok_or_else(|| JsliteError::runtime("array missing"))?
                            .elements
                            .iter()
                            .map(|value| value.clone().unwrap_or(Value::Undefined))
                            .collect(),
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
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?;
                object.properties.contains_key(&key)
                    || matches!(&object.kind, ObjectKind::FunctionPrototype(_) if key == "constructor")
                    || matches!(&object.kind, ObjectKind::StringObject(_) if key == "length")
                    || matches!(&object.kind, ObjectKind::StringObject(value)
                        if array_index_from_property_key(&key)
                            .is_some_and(|index| value.chars().nth(index).is_some()))
                    || matches!(&object.kind, ObjectKind::BoundFunction(_) if matches!(key.as_str(), "name" | "length"))
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                key == "length"
                    || array_index_from_property_key(&key)
                        .is_some_and(|index| array.elements.get(index).is_some_and(Option::is_some))
                    || array.properties.contains_key(&key)
            }
            Value::Closure(closure) => self.closure_has_own_property(closure, &key)?,
            Value::BuiltinFunction(function) => {
                self.builtin_function_custom_property(function, &key)?
                    .is_some()
                    || self.builtin_function_own_property(function, &key).is_some()
            }
            Value::HostFunction(capability) => {
                self.host_function_custom_property(&capability, &key)?
                    .is_some()
                    || matches!(key.as_str(), "name" | "length")
            }
            _ => return Err(Self::object_helper_type_error()),
        };
        Ok(Value::Bool(has_key))
    }
}
