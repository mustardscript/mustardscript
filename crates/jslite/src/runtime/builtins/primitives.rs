use std::{
    collections::HashSet,
    io::{self, Read, Write},
};

use oxc_syntax::number::ToJsString;
use rand::random;
use time::OffsetDateTime;

use super::*;

const JSON_HELPER_IO_CHUNK_BYTES: usize = 256;

#[derive(Default)]
struct JsonStringifyTraversalState {
    arrays: HashSet<ArrayKey>,
    objects: HashSet<ObjectKey>,
}

struct BudgetedJsonReader<'runtime, 'source> {
    runtime: &'runtime mut Runtime,
    source: &'source [u8],
    offset: usize,
    failure: Option<JsliteError>,
}

impl<'runtime, 'source> BudgetedJsonReader<'runtime, 'source> {
    fn new(runtime: &'runtime mut Runtime, source: &'source [u8]) -> Self {
        Self {
            runtime,
            source,
            offset: 0,
            failure: None,
        }
    }

    fn into_failure(self) -> Option<JsliteError> {
        self.failure
    }
}

impl Read for BudgetedJsonReader<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.offset >= self.source.len() {
            return Ok(0);
        }
        let chunk_len = buf
            .len()
            .min(self.source.len() - self.offset)
            .min(JSON_HELPER_IO_CHUNK_BYTES);
        if let Err(error) = self.runtime.charge_native_helper_work(1) {
            self.failure = Some(error);
            return Err(io::Error::other("jslite-json-parse-aborted"));
        }
        buf[..chunk_len].copy_from_slice(&self.source[self.offset..self.offset + chunk_len]);
        self.offset += chunk_len;
        Ok(chunk_len)
    }
}

struct JsonOutputWriter<'runtime, 'output> {
    runtime: &'runtime mut Runtime,
    output: &'output mut String,
    failure: Option<JsliteError>,
}

impl<'runtime, 'output> JsonOutputWriter<'runtime, 'output> {
    fn new(runtime: &'runtime mut Runtime, output: &'output mut String) -> Self {
        Self {
            runtime,
            output,
            failure: None,
        }
    }

    fn into_failure(self) -> Option<JsliteError> {
        self.failure
    }
}

impl Write for JsonOutputWriter<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let chunk_len = buf.len().min(JSON_HELPER_IO_CHUNK_BYTES);
        let next_len = self
            .output
            .len()
            .checked_add(chunk_len)
            .ok_or_else(|| io::Error::other("json output overflow"))?;
        if let Err(error) = self.runtime.ensure_heap_capacity(next_len) {
            self.failure = Some(error);
            return Err(io::Error::other("jslite-json-stringify-aborted"));
        }
        if let Err(error) = self.runtime.charge_native_helper_work(1) {
            self.failure = Some(error);
            return Err(io::Error::other("jslite-json-stringify-aborted"));
        }
        let chunk = std::str::from_utf8(&buf[..chunk_len]).map_err(io::Error::other)?;
        self.output.push_str(chunk);
        Ok(chunk_len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
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

    pub(crate) fn call_json_stringify(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let mut traversal = JsonStringifyTraversalState::default();
        let mut output = String::new();
        if self.json_stringify_value(&value, &mut traversal, &mut output)? {
            Ok(Value::String(output))
        } else {
            Ok(Value::Undefined)
        }
    }

    pub(crate) fn call_json_parse(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let source = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        let mut reader = BudgetedJsonReader::new(self, source.as_bytes());
        let parsed: serde_json::Value = match serde_json::from_reader(&mut reader) {
            Ok(parsed) => parsed,
            Err(error) => {
                if let Some(runtime_error) = reader.into_failure() {
                    return Err(runtime_error);
                }
                return Err(JsliteError::runtime(error.to_string()));
            }
        };
        drop(reader);
        self.value_from_json(parsed)
    }

    fn json_stringify_value(
        &mut self,
        value: &Value,
        traversal: &mut JsonStringifyTraversalState,
        output: &mut String,
    ) -> JsliteResult<bool> {
        self.charge_native_helper_work(1)?;
        match value {
            Value::Undefined => Ok(false),
            Value::Null => {
                self.push_json_fragment(output, "null")?;
                Ok(true)
            }
            Value::Bool(value) => {
                if *value {
                    self.push_json_fragment(output, "true")?;
                } else {
                    self.push_json_fragment(output, "false")?;
                }
                Ok(true)
            }
            Value::Number(value) => {
                self.push_json_fragment(output, &json_number_to_string(*value))?;
                Ok(true)
            }
            Value::BigInt(_) => Err(JsliteError::runtime(
                "TypeError: Do not know how to serialize a BigInt",
            )),
            Value::String(value) => {
                self.push_json_string(output, value)?;
                Ok(true)
            }
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => Ok(false),
            Value::Array(array) => {
                if !traversal.arrays.insert(*array) {
                    return Err(json_stringify_cycle_error());
                }
                let elements = self
                    .arrays
                    .get(*array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .clone();
                let result = (|| {
                    self.push_json_fragment(output, "[")?;
                    for (index, value) in elements.iter().enumerate() {
                        if index > 0 {
                            self.push_json_fragment(output, ",")?;
                        }
                        let value = value.as_ref().unwrap_or(&Value::Undefined);
                        if !self.json_stringify_value(value, traversal, output)? {
                            self.push_json_fragment(output, "null")?;
                        }
                    }
                    self.push_json_fragment(output, "]")?;
                    Ok(true)
                })();
                traversal.arrays.remove(array);
                result
            }
            Value::Object(object) => self.json_stringify_object(*object, traversal, output),
            Value::Map(_) | Value::Set(_) | Value::Iterator(_) | Value::Promise(_) => {
                self.push_json_fragment(output, "{}")?;
                Ok(true)
            }
        }
    }

    fn json_stringify_object(
        &mut self,
        object: ObjectKey,
        traversal: &mut JsonStringifyTraversalState,
        output: &mut String,
    ) -> JsliteResult<bool> {
        let date_timestamp_ms = {
            let object_ref = self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?;
            match &object_ref.kind {
                ObjectKind::Date(date) => Some(date.timestamp_ms),
                _ => None,
            }
        };
        if let Some(timestamp_ms) = date_timestamp_ms {
            self.push_json_fragment(output, &self.json_stringify_date(timestamp_ms)?)?;
            return Ok(true);
        }

        if !traversal.objects.insert(object) {
            return Err(json_stringify_cycle_error());
        }

        let keys = {
            let object_ref = self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?;
            match &object_ref.kind {
                ObjectKind::Error(_) => {
                    ordered_own_property_keys_filtered(&object_ref.properties, |key, _| {
                        key != "name" && key != "message"
                    })
                }
                _ => ordered_own_property_keys(&object_ref.properties),
            }
        };
        let result = (|| {
            self.push_json_fragment(output, "{")?;
            let mut wrote_any = false;
            for key in keys {
                let value = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("object property missing"))?;
                let rewind = output.len();
                if wrote_any {
                    self.push_json_fragment(output, ",")?;
                }
                self.push_json_string(output, &key)?;
                self.push_json_fragment(output, ":")?;
                if !self.json_stringify_value(&value, traversal, output)? {
                    output.truncate(rewind);
                    continue;
                }
                wrote_any = true;
            }
            self.push_json_fragment(output, "}")?;
            Ok(true)
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

    fn push_json_fragment(&mut self, output: &mut String, fragment: &str) -> JsliteResult<()> {
        let next_len = output
            .len()
            .checked_add(fragment.len())
            .ok_or_else(|| limit_error("heap limit exceeded"))?;
        self.ensure_heap_capacity(next_len)?;
        let units = fragment.len().max(1).div_ceil(JSON_HELPER_IO_CHUNK_BYTES);
        self.charge_native_helper_work(units)?;
        output.push_str(fragment);
        Ok(())
    }

    fn push_json_string(&mut self, output: &mut String, value: &str) -> JsliteResult<()> {
        let mut writer = JsonOutputWriter::new(self, output);
        let result = serde_json::to_writer(&mut writer, value);
        match result {
            Ok(()) => {
                if let Some(runtime_error) = writer.into_failure() {
                    Err(runtime_error)
                } else {
                    Ok(())
                }
            }
            Err(error) => {
                if let Some(runtime_error) = writer.into_failure() {
                    Err(runtime_error)
                } else {
                    Err(JsliteError::runtime(error.to_string()))
                }
            }
        }
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
