use super::*;

const STRUCTURED_BOUNDARY_MAX_DEPTH: usize = 128;

#[derive(Default)]
struct StructuredTraversalState {
    active_arrays: HashSet<ArrayKey>,
    active_objects: HashSet<ObjectKey>,
    seen_arrays: HashSet<ArrayKey>,
    seen_objects: HashSet<ObjectKey>,
}

impl Runtime {
    pub(in crate::runtime) fn value_from_structured(
        &mut self,
        value: StructuredValue,
    ) -> MustardResult<Value> {
        self.value_from_structured_inner(value, 1)
    }

    fn value_from_structured_inner(
        &mut self,
        value: StructuredValue,
        depth: usize,
    ) -> MustardResult<Value> {
        ensure_nested_depth(depth, structured_boundary_depth_error)?;
        self.charge_native_helper_work(1)?;
        Ok(match value {
            StructuredValue::Undefined => Value::Undefined,
            StructuredValue::Null => Value::Null,
            StructuredValue::Hole => {
                return Err(MustardError::runtime(
                    "array holes can only appear inside structured arrays",
                ));
            }
            StructuredValue::Bool(value) => Value::Bool(value),
            StructuredValue::String(value) => {
                self.ensure_heap_capacity(value.len())?;
                Value::String(value)
            }
            StructuredValue::Number(number) => Value::Number(number.to_f64()),
            StructuredValue::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(match item {
                        StructuredValue::Hole => None,
                        other => Some(self.value_from_structured_inner(other, depth + 1)?),
                    });
                }
                let array = self.insert_sparse_array(values, IndexMap::new())?;
                Value::Array(array)
            }
            StructuredValue::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_structured_inner(value, depth + 1)?);
                }
                let object = self.insert_object(properties, ObjectKind::Plain)?;
                Value::Object(object)
            }
        })
    }

    pub(in crate::runtime) fn value_to_structured(
        &mut self,
        value: Value,
    ) -> MustardResult<StructuredValue> {
        let mut traversal = StructuredTraversalState::default();
        self.value_to_structured_inner(value, &mut traversal, 1)
    }

    fn value_to_structured_inner(
        &mut self,
        value: Value,
        traversal: &mut StructuredTraversalState,
        depth: usize,
    ) -> MustardResult<StructuredValue> {
        ensure_nested_depth(depth, structured_boundary_depth_error)?;
        self.charge_native_helper_work(1)?;
        Ok(match value {
            Value::Undefined => StructuredValue::Undefined,
            Value::Null => StructuredValue::Null,
            Value::Bool(value) => StructuredValue::Bool(value),
            Value::Number(value) => StructuredValue::Number(StructuredNumber::from_f64(value)),
            Value::BigInt(_) => {
                return Err(MustardError::runtime(
                    "BigInt values cannot cross the structured host boundary",
                ));
            }
            Value::String(value) => {
                self.ensure_heap_capacity(value.len())?;
                StructuredValue::String(value)
            }
            Value::Array(array) => {
                if !traversal.active_arrays.insert(array) {
                    return Err(structured_boundary_cycle_error());
                }
                if !traversal.seen_arrays.insert(array) {
                    traversal.active_arrays.remove(&array);
                    return Err(structured_boundary_shared_reference_error());
                }
                let elements = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .clone();
                let result = (|| {
                    Ok(StructuredValue::Array(
                        elements
                            .iter()
                            .map(|value| match value {
                                Some(value) => self.value_to_structured_inner(
                                    value.clone(),
                                    traversal,
                                    depth + 1,
                                ),
                                None => Ok(StructuredValue::Hole),
                            })
                            .collect::<MustardResult<Vec<_>>>()?,
                    ))
                })();
                traversal.active_arrays.remove(&array);
                result?
            }
            Value::Object(object) => {
                let object_ref = self
                    .objects
                    .get(object)
                    .ok_or_else(|| MustardError::runtime("object missing"))?;
                if matches!(object_ref.kind, ObjectKind::Date(_)) {
                    return Err(MustardError::runtime(
                        "Date values cannot cross the structured host boundary",
                    ));
                }
                if !traversal.active_objects.insert(object) {
                    return Err(structured_boundary_cycle_error());
                }
                if !traversal.seen_objects.insert(object) {
                    traversal.active_objects.remove(&object);
                    return Err(structured_boundary_shared_reference_error());
                }
                let properties = object_ref.properties.clone();
                let result = (|| {
                    Ok(StructuredValue::Object(
                        properties
                            .iter()
                            .map(|(key, value)| {
                                Ok((
                                    key.clone(),
                                    self.value_to_structured_inner(
                                        value.clone(),
                                        traversal,
                                        depth + 1,
                                    )?,
                                ))
                            })
                            .collect::<MustardResult<IndexMap<_, _>>>()?,
                    ))
                })();
                traversal.active_objects.remove(&object);
                result?
            }
            Value::Map(_) | Value::Set(_) => {
                return Err(MustardError::runtime(
                    "Map and Set values cannot cross the structured host boundary",
                ));
            }
            Value::Iterator(_)
            | Value::Promise(_)
            | Value::Closure(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {
                return Err(MustardError::runtime(
                    "functions cannot cross the structured host boundary",
                ));
            }
        })
    }

    pub(in crate::runtime) fn value_from_json(
        &mut self,
        value: serde_json::Value,
    ) -> MustardResult<Value> {
        self.value_from_json_inner(value, 1)
    }

    fn value_from_json_inner(
        &mut self,
        value: serde_json::Value,
        depth: usize,
    ) -> MustardResult<Value> {
        ensure_nested_depth(depth, json_value_depth_error)?;
        self.charge_native_helper_work(1)?;
        match value {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(value) => Ok(Value::Bool(value)),
            serde_json::Value::Number(number) => Ok(Value::Number(number.as_f64().unwrap_or(0.0))),
            serde_json::Value::String(value) => {
                self.ensure_heap_capacity(value.len())?;
                Ok(Value::String(value))
            }
            serde_json::Value::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.value_from_json_inner(item, depth + 1)?);
                }
                let array = self.insert_array(values, IndexMap::new())?;
                Ok(Value::Array(array))
            }
            serde_json::Value::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_json_inner(value, depth + 1)?);
                }
                let object = self.insert_object(properties, ObjectKind::Plain)?;
                Ok(Value::Object(object))
            }
        }
    }
}

#[allow(dead_code)]
pub(in crate::runtime) fn structured_to_json(
    value: StructuredValue,
) -> MustardResult<serde_json::Value> {
    Ok(match value {
        StructuredValue::Undefined => serde_json::Value::Null,
        StructuredValue::Null => serde_json::Value::Null,
        StructuredValue::Hole => {
            return Err(MustardError::runtime(
                "array holes cannot appear outside structured arrays",
            ));
        }
        StructuredValue::Bool(value) => serde_json::Value::Bool(value),
        StructuredValue::String(value) => serde_json::Value::String(value),
        StructuredValue::Number(number) => match number {
            StructuredNumber::Finite(value) => serde_json::Number::from_f64(value)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            StructuredNumber::NaN
            | StructuredNumber::Infinity
            | StructuredNumber::NegInfinity
            | StructuredNumber::NegZero => serde_json::Value::Null,
        },
        StructuredValue::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(structured_to_json)
                .collect::<MustardResult<Vec<_>>>()?,
        ),
        StructuredValue::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, structured_to_json(value)?)))
                .collect::<MustardResult<serde_json::Map<_, _>>>()?,
        ),
    })
}

fn structured_boundary_cycle_error() -> MustardError {
    MustardError::runtime("cyclic values cannot cross the structured host boundary")
}

fn structured_boundary_shared_reference_error() -> MustardError {
    MustardError::runtime("shared references cannot cross the structured host boundary")
}

fn structured_boundary_depth_error() -> MustardError {
    MustardError::runtime("structured host boundary nesting limit exceeded")
}

fn json_value_depth_error() -> MustardError {
    MustardError::runtime("JSON value nesting limit exceeded")
}

fn ensure_nested_depth(
    depth: usize,
    make_error: impl FnOnce() -> MustardError,
) -> MustardResult<()> {
    if depth > STRUCTURED_BOUNDARY_MAX_DEPTH {
        return Err(make_error());
    }
    Ok(())
}
