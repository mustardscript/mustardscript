use super::*;

impl Runtime {
    pub(in crate::runtime) fn to_number(&self, value: Value) -> MustardResult<f64> {
        Ok(match value {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(value) => {
                if value {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Number(value) => value,
            Value::BigInt(_) => {
                return Err(MustardError::runtime(
                    "TypeError: cannot coerce BigInt values to numbers",
                ));
            }
            Value::String(value) => value.parse::<f64>().unwrap_or(f64::NAN),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::NumberObject(value) => *value,
                ObjectKind::StringObject(value) => value.parse::<f64>().unwrap_or(f64::NAN),
                ObjectKind::BooleanObject(value) => {
                    if *value {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => {
                    return Err(MustardError::runtime(
                        "cannot coerce complex value to number",
                    ));
                }
            },
            Value::Array(_)
            | Value::Map(_)
            | Value::Set(_)
            | Value::Iterator(_)
            | Value::Promise(_)
            | Value::Closure(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {
                return Err(MustardError::runtime(
                    "cannot coerce complex value to number",
                ));
            }
        })
    }

    pub(in crate::runtime) fn to_integer(&self, value: Value) -> MustardResult<i64> {
        let number = self.to_number(value)?;
        if number.is_nan() || number == 0.0 {
            Ok(0)
        } else if number.is_infinite() {
            Ok(if number.is_sign_positive() {
                i64::MAX
            } else {
                i64::MIN
            })
        } else {
            let truncated = number.trunc();
            if truncated >= i64::MAX as f64 {
                Ok(i64::MAX)
            } else if truncated <= i64::MIN as f64 {
                Ok(i64::MIN)
            } else {
                Ok(truncated as i64)
            }
        }
    }

    pub(in crate::runtime) fn to_string(&self, value: Value) -> MustardResult<String> {
        Ok(match value {
            Value::Undefined => "undefined".to_string(),
            Value::Null => "null".to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => {
                if value.fract() == 0.0 {
                    format!("{}", value as i64)
                } else {
                    value.to_string()
                }
            }
            Value::BigInt(value) => value.to_string(),
            Value::String(value) => value,
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?;
                let mut parts = Vec::new();
                for value in &array.elements {
                    parts.push(match value {
                        None | Some(Value::Undefined) | Some(Value::Null) => String::new(),
                        Some(value) => self.to_string(value.clone())?,
                    });
                }
                parts.join(",")
            }
            Value::Map(_) => "[object Map]".to_string(),
            Value::Set(_) => "[object Set]".to_string(),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::Date(_) => "[object Date]".to_string(),
                ObjectKind::RegExp(regex) => format!("/{}/{}", regex.pattern, regex.flags),
                ObjectKind::NumberObject(value) => {
                    if value.fract() == 0.0 {
                        format!("{}", *value as i64)
                    } else {
                        value.to_string()
                    }
                }
                ObjectKind::StringObject(value) => value.clone(),
                ObjectKind::BooleanObject(value) => value.to_string(),
                ObjectKind::BoundFunction(_) => {
                    self.callable_display_string(&Value::Object(object))?
                }
                _ => self
                    .error_summary(object)?
                    .unwrap_or_else(|| "[object Object]".to_string()),
            },
            Value::Iterator(_) => "[object Iterator]".to_string(),
            Value::Promise(_) => "[object Promise]".to_string(),
            callable @ (Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_)) => {
                self.callable_display_string(&callable)?
            }
        })
    }

    pub(in crate::runtime) fn to_property_key(&self, value: Value) -> MustardResult<String> {
        match value {
            Value::String(value) => Ok(value),
            Value::Number(value) => Ok(format_number_key(value)),
            Value::BigInt(value) => Ok(value.to_string()),
            Value::Bool(value) => Ok(value.to_string()),
            Value::Null => Ok("null".to_string()),
            Value::Undefined => Ok("undefined".to_string()),
            _ => self.to_string(value),
        }
    }

    pub(in crate::runtime) fn to_array_items(&self, value: Value) -> MustardResult<Vec<Value>> {
        match value {
            Value::Array(array) => self
                .arrays
                .get(array)
                .map(|array| {
                    array
                        .elements
                        .iter()
                        .map(|value| value.clone().unwrap_or(Value::Undefined))
                        .collect()
                })
                .ok_or_else(|| MustardError::runtime("array missing")),
            Value::Undefined | Value::Null => Ok(Vec::new()),
            _ => Err(MustardError::runtime(
                "value is not destructurable as an array",
            )),
        }
    }

    fn callable_display_string(&self, value: &Value) -> MustardResult<String> {
        Ok(match value {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(*closure)
                    .ok_or_else(|| MustardError::runtime("closure missing"))?;
                let function = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .ok_or_else(|| MustardError::runtime("function not found"))?;
                if function.display_source.is_empty() {
                    let name = self.callable_name(value)?;
                    if name.is_empty() {
                        "function () { [mustard code] }".to_string()
                    } else {
                        format!("function {name}() {{ [mustard code] }}")
                    }
                } else {
                    function.display_source.clone()
                }
            }
            Value::BuiltinFunction(function) => {
                format!(
                    "function {}() {{ [native code] }}",
                    self.callable_name(&Value::BuiltinFunction(*function))?
                )
            }
            Value::HostFunction(_) => "function () { [host code] }".to_string(),
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(*object)
                    .ok_or_else(|| MustardError::runtime("object missing"))?;
                match object.kind {
                    ObjectKind::BoundFunction(_) => "function () { [native code] }".to_string(),
                    _ => "[object Object]".to_string(),
                }
            }
            _ => self.to_string(value.clone())?,
        })
    }
}
