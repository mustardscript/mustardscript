use std::collections::HashSet;

use oxc_syntax::number::ToJsString;
use rand::random;
use time::OffsetDateTime;

use super::*;

#[derive(Default)]
struct JsonStringifyTraversalState {
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

    pub(crate) fn call_math_random(&self) -> Value {
        Value::Number(random::<f64>())
    }

    pub(crate) fn call_json_stringify(&self, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let mut traversal = JsonStringifyTraversalState::default();
        match self.json_stringify_value(&value, &mut traversal)? {
            Some(json) => Ok(Value::String(json)),
            None => Ok(Value::Undefined),
        }
    }

    pub(crate) fn call_json_parse(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let source = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        let parsed: serde_json::Value = serde_json::from_str(&source)
            .map_err(|error| JsliteError::runtime(error.to_string()))?;
        self.value_from_json(parsed)
    }

    fn json_stringify_value(
        &self,
        value: &Value,
        traversal: &mut JsonStringifyTraversalState,
    ) -> JsliteResult<Option<String>> {
        match value {
            Value::Undefined => Ok(None),
            Value::Null => Ok(Some("null".to_string())),
            Value::Bool(value) => Ok(Some(value.to_string())),
            Value::Number(value) => Ok(Some(json_number_to_string(*value))),
            Value::BigInt(_) => Err(JsliteError::runtime(
                "TypeError: Do not know how to serialize a BigInt",
            )),
            Value::String(value) => serde_json::to_string(value)
                .map(Some)
                .map_err(|error| JsliteError::runtime(error.to_string())),
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => Ok(None),
            Value::Array(array) => {
                if !traversal.arrays.insert(*array) {
                    return Err(json_stringify_cycle_error());
                }
                let result = (|| {
                    let mut serialized = Vec::new();
                    for value in &self
                        .arrays
                        .get(*array)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?
                        .elements
                    {
                        serialized.push(
                            self.json_stringify_value(value, traversal)?
                                .unwrap_or_else(|| "null".to_string()),
                        );
                    }
                    Ok(Some(format!("[{}]", serialized.join(","))))
                })();
                traversal.arrays.remove(array);
                result
            }
            Value::Object(object) => self.json_stringify_object(*object, traversal),
            Value::Map(_) | Value::Set(_) | Value::Iterator(_) | Value::Promise(_) => {
                Ok(Some("{}".to_string()))
            }
        }
    }

    fn json_stringify_object(
        &self,
        object: ObjectKey,
        traversal: &mut JsonStringifyTraversalState,
    ) -> JsliteResult<Option<String>> {
        let object_ref = self
            .objects
            .get(object)
            .ok_or_else(|| JsliteError::runtime("object missing"))?;
        if let ObjectKind::Date(date) = &object_ref.kind {
            return self.json_stringify_date(date.timestamp_ms).map(Some);
        }

        if !traversal.objects.insert(object) {
            return Err(json_stringify_cycle_error());
        }

        let result = (|| {
            let keys = match &self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?
                .kind
            {
                ObjectKind::Error(_) => ordered_own_property_keys_filtered(
                    &self
                        .objects
                        .get(object)
                        .ok_or_else(|| JsliteError::runtime("object missing"))?
                        .properties,
                    |key, _| key != "name" && key != "message",
                ),
                _ => ordered_own_property_keys(
                    &self
                        .objects
                        .get(object)
                        .ok_or_else(|| JsliteError::runtime("object missing"))?
                        .properties,
                ),
            };

            let mut serialized = Vec::new();
            for key in keys {
                let value = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("object property missing"))?;
                let Some(value) = self.json_stringify_value(&value, traversal)? else {
                    continue;
                };
                let key = serde_json::to_string(&key)
                    .map_err(|error| JsliteError::runtime(error.to_string()))?;
                serialized.push(format!("{key}:{value}"));
            }

            Ok(Some(format!("{{{}}}", serialized.join(","))))
        })();

        traversal.objects.remove(&object);
        result
    }

    fn json_stringify_date(&self, timestamp_ms: f64) -> JsliteResult<String> {
        if !timestamp_ms.is_finite() {
            return Ok("null".to_string());
        }

        let timestamp_nanos = timestamp_ms * 1_000_000.0;
        if !timestamp_nanos.is_finite()
            || timestamp_nanos < i128::MIN as f64
            || timestamp_nanos > i128::MAX as f64
        {
            return Ok("null".to_string());
        }

        let Ok(datetime) =
            OffsetDateTime::from_unix_timestamp_nanos(timestamp_nanos.trunc() as i128)
        else {
            return Ok("null".to_string());
        };
        let rendered = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            datetime.year(),
            datetime.month() as u8,
            datetime.day(),
            datetime.hour(),
            datetime.minute(),
            datetime.second(),
            datetime.millisecond(),
        );
        serde_json::to_string(&rendered).map_err(|error| JsliteError::runtime(error.to_string()))
    }
}

fn json_number_to_string(value: f64) -> String {
    if !value.is_finite() {
        "null".to_string()
    } else {
        value.to_js_string()
    }
}

fn json_stringify_cycle_error() -> JsliteError {
    JsliteError::runtime("TypeError: Converting circular structure to JSON")
}
