use super::*;

impl Runtime {
    pub(in crate::runtime) fn to_number(&self, value: Value) -> JsliteResult<f64> {
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
                return Err(JsliteError::runtime(
                    "TypeError: cannot coerce BigInt values to numbers",
                ));
            }
            Value::String(value) => value.parse::<f64>().unwrap_or(f64::NAN),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?
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
                    return Err(JsliteError::runtime(
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
                return Err(JsliteError::runtime(
                    "cannot coerce complex value to number",
                ));
            }
        })
    }

    pub(in crate::runtime) fn to_integer(&self, value: Value) -> JsliteResult<i64> {
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

    pub(in crate::runtime) fn to_string(&self, value: Value) -> JsliteResult<String> {
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
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
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
                .ok_or_else(|| JsliteError::runtime("object missing"))?
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
                _ => self
                    .error_summary(object)?
                    .unwrap_or_else(|| "[object Object]".to_string()),
            },
            Value::Iterator(_) => "[object Iterator]".to_string(),
            Value::Promise(_) => "[object Promise]".to_string(),
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => {
                "[Function]".to_string()
            }
        })
    }

    pub(in crate::runtime) fn to_property_key(&self, value: Value) -> JsliteResult<String> {
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

    pub(in crate::runtime) fn to_array_items(&self, value: Value) -> JsliteResult<Vec<Value>> {
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
                .ok_or_else(|| JsliteError::runtime("array missing")),
            Value::Undefined | Value::Null => Ok(Vec::new()),
            _ => Err(JsliteError::runtime(
                "value is not destructurable as an array",
            )),
        }
    }
}
