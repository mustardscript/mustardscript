use super::*;

#[derive(Default)]
struct StructuredTraversalState {
    arrays: HashSet<ArrayKey>,
    objects: HashSet<ObjectKey>,
}

impl Runtime {
    pub(in crate::runtime) fn value_from_structured(
        &mut self,
        value: StructuredValue,
    ) -> JsliteResult<Value> {
        Ok(match value {
            StructuredValue::Undefined => Value::Undefined,
            StructuredValue::Null => Value::Null,
            StructuredValue::Bool(value) => Value::Bool(value),
            StructuredValue::String(value) => Value::String(value),
            StructuredValue::Number(number) => Value::Number(number.to_f64()),
            StructuredValue::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.value_from_structured(item)?);
                }
                let array = self.insert_array(values, IndexMap::new())?;
                Value::Array(array)
            }
            StructuredValue::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_structured(value)?);
                }
                let object = self.insert_object(properties, ObjectKind::Plain)?;
                Value::Object(object)
            }
        })
    }

    pub(in crate::runtime) fn value_to_structured(
        &self,
        value: Value,
    ) -> JsliteResult<StructuredValue> {
        let mut traversal = StructuredTraversalState::default();
        self.value_to_structured_inner(value, &mut traversal)
    }

    fn value_to_structured_inner(
        &self,
        value: Value,
        traversal: &mut StructuredTraversalState,
    ) -> JsliteResult<StructuredValue> {
        Ok(match value {
            Value::Undefined => StructuredValue::Undefined,
            Value::Null => StructuredValue::Null,
            Value::Bool(value) => StructuredValue::Bool(value),
            Value::Number(value) => StructuredValue::Number(StructuredNumber::from_f64(value)),
            Value::BigInt(_) => {
                return Err(JsliteError::runtime(
                    "BigInt values cannot cross the structured host boundary",
                ));
            }
            Value::String(value) => StructuredValue::String(value),
            Value::Array(array) => {
                if !traversal.arrays.insert(array) {
                    return Err(structured_boundary_cycle_error());
                }
                let result = (|| {
                    Ok(StructuredValue::Array(
                        self.arrays
                            .get(array)
                            .ok_or_else(|| JsliteError::runtime("array missing"))?
                            .elements
                            .iter()
                            .cloned()
                            .map(|value| self.value_to_structured_inner(value, traversal))
                            .collect::<JsliteResult<Vec<_>>>()?,
                    ))
                })();
                traversal.arrays.remove(&array);
                result?
            }
            Value::Object(object) => {
                let object_ref = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?;
                if matches!(object_ref.kind, ObjectKind::Date(_)) {
                    return Err(JsliteError::runtime(
                        "Date values cannot cross the structured host boundary",
                    ));
                }
                if !traversal.objects.insert(object) {
                    return Err(structured_boundary_cycle_error());
                }
                let result = (|| {
                    Ok(StructuredValue::Object(
                        object_ref
                            .properties
                            .iter()
                            .map(|(key, value)| {
                                Ok((
                                    key.clone(),
                                    self.value_to_structured_inner(value.clone(), traversal)?,
                                ))
                            })
                            .collect::<JsliteResult<IndexMap<_, _>>>()?,
                    ))
                })();
                traversal.objects.remove(&object);
                result?
            }
            Value::Map(_) | Value::Set(_) => {
                return Err(JsliteError::runtime(
                    "Map and Set values cannot cross the structured host boundary",
                ));
            }
            Value::Iterator(_)
            | Value::Promise(_)
            | Value::Closure(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {
                return Err(JsliteError::runtime(
                    "functions cannot cross the structured host boundary",
                ));
            }
        })
    }

    pub(in crate::runtime) fn value_from_json(
        &mut self,
        value: serde_json::Value,
    ) -> JsliteResult<Value> {
        match value {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(value) => Ok(Value::Bool(value)),
            serde_json::Value::Number(number) => Ok(Value::Number(number.as_f64().unwrap_or(0.0))),
            serde_json::Value::String(value) => Ok(Value::String(value)),
            serde_json::Value::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.value_from_json(item)?);
                }
                let array = self.insert_array(values, IndexMap::new())?;
                Ok(Value::Array(array))
            }
            serde_json::Value::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_json(value)?);
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
) -> JsliteResult<serde_json::Value> {
    Ok(match value {
        StructuredValue::Undefined => serde_json::Value::Null,
        StructuredValue::Null => serde_json::Value::Null,
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
                .collect::<JsliteResult<Vec<_>>>()?,
        ),
        StructuredValue::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, structured_to_json(value)?)))
                .collect::<JsliteResult<serde_json::Map<_, _>>>()?,
        ),
    })
}

fn structured_boundary_cycle_error() -> JsliteError {
    JsliteError::runtime("cyclic values cannot cross the structured host boundary")
}
