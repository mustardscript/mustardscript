use std::collections::HashSet;

use crate::runtime::conversions::structured_to_json;

use super::*;

#[derive(Default)]
struct JsonBigIntTraversalState {
    arrays: HashSet<ArrayKey>,
    objects: HashSet<ObjectKey>,
}

impl Runtime {
    pub(crate) fn construct_date(&mut self, args: &[Value]) -> JsliteResult<Value> {
        if args.len() > 1 {
            return Err(JsliteError::runtime(
                "TypeError: Date currently supports zero or one constructor argument",
            ));
        }
        let timestamp_ms = match args {
            [] => current_time_millis(),
            [value] => self.date_timestamp_ms_from_value(value.clone())?,
            _ => unreachable!(),
        };
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::Date(DateObject { timestamp_ms }),
        )?))
    }

    fn date_receiver(&self, value: Value, method: &str) -> JsliteResult<ObjectKey> {
        match value {
            Value::Object(key) if self.is_date_object(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Date.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn call_date_get_time(&self, this_value: Value) -> JsliteResult<Value> {
        let date = self.date_receiver(this_value, "getTime")?;
        Ok(Value::Number(self.date_object(date)?.timestamp_ms))
    }

    fn date_timestamp_ms_from_value(&self, value: Value) -> JsliteResult<f64> {
        match value {
            Value::Number(value) => Ok(value),
            Value::String(value) => Ok(parse_date_timestamp_ms(&value)),
            Value::Object(object) if self.is_date_object(object) => {
                Ok(self.date_object(object)?.timestamp_ms)
            }
            Value::Undefined => Ok(f64::NAN),
            _ => Err(JsliteError::runtime(
                "TypeError: Date currently supports only numeric, string, or Date arguments",
            )),
        }
    }

    pub(crate) fn call_error_ctor(&mut self, args: &[Value], name: &str) -> JsliteResult<Value> {
        self.make_error_object(name, args, None, None)
    }

    pub(crate) fn call_number_ctor(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(self.to_number(
            args.first().cloned().unwrap_or(Value::Undefined),
        )?))
    }

    pub(crate) fn call_string_ctor(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::String(self.to_string(
            args.first().cloned().unwrap_or(Value::Undefined),
        )?))
    }

    pub(crate) fn call_boolean_ctor(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Bool(is_truthy(
            args.first().unwrap_or(&Value::Undefined),
        )))
    }

    pub(crate) fn call_math_abs(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .abs(),
        ))
    }

    pub(crate) fn call_math_max(&self, args: &[Value]) -> JsliteResult<Value> {
        let mut value = f64::NEG_INFINITY;
        for arg in args {
            value = value.max(self.to_number(arg.clone())?);
        }
        Ok(Value::Number(value))
    }

    pub(crate) fn call_math_min(&self, args: &[Value]) -> JsliteResult<Value> {
        let mut value = f64::INFINITY;
        for arg in args {
            value = value.min(self.to_number(arg.clone())?);
        }
        Ok(Value::Number(value))
    }

    pub(crate) fn call_math_floor(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .floor(),
        ))
    }

    pub(crate) fn call_math_ceil(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .ceil(),
        ))
    }

    pub(crate) fn call_math_round(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .round(),
        ))
    }

    pub(crate) fn call_math_pow(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .powf(self.to_number(args.get(1).cloned().unwrap_or(Value::Undefined))?),
        ))
    }

    pub(crate) fn call_math_sqrt(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .sqrt(),
        ))
    }

    pub(crate) fn call_math_trunc(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .trunc(),
        ))
    }

    pub(crate) fn call_math_sign(&self, args: &[Value]) -> JsliteResult<Value> {
        let value = self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?;
        Ok(Value::Number(if value.is_nan() {
            f64::NAN
        } else if value == 0.0 {
            value
        } else if value.is_sign_positive() {
            1.0
        } else {
            -1.0
        }))
    }

    pub(crate) fn call_math_log(&self, args: &[Value]) -> JsliteResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .ln(),
        ))
    }

    pub(crate) fn call_json_stringify(&self, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let mut traversal = JsonBigIntTraversalState::default();
        if self.json_value_contains_bigint(&value, &mut traversal)? {
            return Err(JsliteError::runtime(
                "TypeError: JSON.stringify does not support BigInt values",
            ));
        }
        let structured = self.value_to_structured(value)?;
        let json = serde_json::to_string(&structured_to_json(structured)?)
            .map_err(|error| JsliteError::runtime(error.to_string()))?;
        Ok(Value::String(json))
    }

    pub(crate) fn call_json_parse(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let source = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        let parsed: serde_json::Value = serde_json::from_str(&source)
            .map_err(|error| JsliteError::runtime(error.to_string()))?;
        self.value_from_json(parsed)
    }

    fn json_value_contains_bigint(
        &self,
        value: &Value,
        traversal: &mut JsonBigIntTraversalState,
    ) -> JsliteResult<bool> {
        match value {
            Value::BigInt(_) => Ok(true),
            Value::Array(array) => {
                if !traversal.arrays.insert(*array) {
                    return Ok(false);
                }
                let result = (|| {
                    for value in &self
                        .arrays
                        .get(*array)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?
                        .elements
                    {
                        if self.json_value_contains_bigint(value, traversal)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                })();
                traversal.arrays.remove(array);
                result
            }
            Value::Object(object) => {
                if !traversal.objects.insert(*object) {
                    return Ok(false);
                }
                let result = (|| {
                    for value in self
                        .objects
                        .get(*object)
                        .ok_or_else(|| JsliteError::runtime("object missing"))?
                        .properties
                        .values()
                    {
                        if self.json_value_contains_bigint(value, traversal)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                })();
                traversal.objects.remove(object);
                result
            }
            _ => Ok(false),
        }
    }
}
