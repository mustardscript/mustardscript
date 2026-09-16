use super::*;
use oxc_syntax::number::ToJsString;
use std::{
    collections::HashSet,
    io::{self, Read, Write},
};

const JSON_HELPER_IO_CHUNK_BYTES: usize = 256;
const JSON_MAX_DEPTH: usize = 128;

#[derive(Default)]
struct JsonStringifyTraversalState {
    arrays: HashSet<ArrayKey>,
    objects: HashSet<ObjectKey>,
    replacer: Option<Value>,
    property_list: Option<Vec<String>>,
    gap: String,
    depth: usize,
}

struct BudgetedJsonReader<'runtime, 'source> {
    runtime: &'runtime mut Runtime,
    source: &'source [u8],
    offset: usize,
    failure: Option<MustardError>,
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

    fn into_failure(self) -> Option<MustardError> {
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
            return Err(io::Error::other("mustard-json-parse-aborted"));
        }
        buf[..chunk_len].copy_from_slice(&self.source[self.offset..self.offset + chunk_len]);
        self.offset += chunk_len;
        Ok(chunk_len)
    }
}

struct JsonOutputWriter<'runtime, 'output> {
    runtime: &'runtime mut Runtime,
    output: &'output mut String,
    failure: Option<MustardError>,
}

impl<'runtime, 'output> JsonOutputWriter<'runtime, 'output> {
    fn new(runtime: &'runtime mut Runtime, output: &'output mut String) -> Self {
        Self {
            runtime,
            output,
            failure: None,
        }
    }

    fn into_failure(self) -> Option<MustardError> {
        self.failure
    }
}

impl Write for JsonOutputWriter<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let text = std::str::from_utf8(buf).map_err(io::Error::other)?;
        let mut chunk_len = buf.len().min(JSON_HELPER_IO_CHUNK_BYTES);
        while !text.is_char_boundary(chunk_len) {
            chunk_len -= 1;
        }
        let next_len = self
            .output
            .len()
            .checked_add(chunk_len)
            .ok_or_else(|| io::Error::other("json output overflow"))?;
        if let Err(error) = self.runtime.ensure_heap_capacity(next_len) {
            self.failure = Some(error);
            return Err(io::Error::other("mustard-json-stringify-aborted"));
        }
        if let Err(error) = self.runtime.charge_native_helper_work(1) {
            self.failure = Some(error);
            return Err(io::Error::other("mustard-json-stringify-aborted"));
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
    pub(crate) fn call_json_stringify(&mut self, args: &[Value]) -> MustardResult<Value> {
        self.with_temporary_roots(args, |runtime| {
            let mut traversal = runtime.json_stringify_options(args)?;
            let holder = runtime.insert_object(
                IndexMap::from([(
                    String::new(),
                    args.first().cloned().unwrap_or(Value::Undefined),
                )]),
                ObjectKind::Plain,
            )?;
            runtime.with_temporary_roots(&[Value::Object(holder)], |runtime| {
                let mut output = String::new();
                if runtime.json_stringify_property(
                    Value::Object(holder),
                    "",
                    &mut traversal,
                    &mut output,
                )? {
                    Ok(Value::String(output))
                } else {
                    Ok(Value::Undefined)
                }
            })
        })
    }

    fn json_unbox_primitive(&self, value: Value) -> MustardResult<Value> {
        if let Value::Object(object) = value {
            let object = self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?;
            match &object.kind {
                ObjectKind::NumberObject(number) => return Ok(Value::Number(*number)),
                ObjectKind::StringObject(string) => return Ok(Value::String(string.clone())),
                ObjectKind::BooleanObject(boolean) => return Ok(Value::Bool(*boolean)),
                _ => {}
            }
        }
        Ok(value)
    }

    fn json_stringify_options(
        &mut self,
        args: &[Value],
    ) -> MustardResult<JsonStringifyTraversalState> {
        let mut state = JsonStringifyTraversalState::default();
        if let Some(replacer) = args.get(1) {
            if self.is_callable_value(replacer)? {
                state.replacer = Some(replacer.clone());
            } else if let Value::Array(array) = replacer {
                let length = self.array_length(*array)?;
                let mut keys = Vec::new();
                let mut seen = HashSet::new();
                let mut bytes = 0usize;
                for index in 0..length {
                    self.charge_native_helper_work(1)?;
                    let value = self.json_unbox_primitive(self.array_value_at(*array, index)?)?;
                    if matches!(value, Value::String(_) | Value::Number(_)) {
                        let key = self.to_string(value)?;
                        if seen.insert(key.clone()) {
                            bytes = bytes
                                .checked_add(key.len() + std::mem::size_of::<String>())
                                .ok_or_else(|| limit_error("heap limit exceeded"))?;
                            self.ensure_heap_capacity(bytes)?;
                            keys.push(key);
                        }
                    }
                }
                state.property_list = Some(keys);
            }
        }
        state.gap =
            match self.json_unbox_primitive(args.get(2).cloned().unwrap_or(Value::Undefined))? {
                Value::Number(number) => " ".repeat(if number.is_nan() {
                    0
                } else {
                    number.clamp(0.0, 10.0) as usize
                }),
                // Strings currently use Unicode scalar indexing throughout the runtime.
                Value::String(string) => string.chars().take(10).collect(),
                _ => String::new(),
            };
        Ok(state)
    }

    fn call_json_callback(
        &mut self,
        callback: Value,
        holder: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        self.call_callback(callback, holder, args, CallbackCallOptions {
            non_callable_message: "TypeError: JSON callback is not callable",
            host_suspension_message: "TypeError: JSON callbacks do not support synchronous host suspensions",
            unsettled_message: "synchronous JSON callback did not settle",
            allow_host_suspension: false,
            allow_pending_promise_result: true,
        })
    }

    fn json_stringify_property(
        &mut self,
        holder: Value,
        key: &str,
        state: &mut JsonStringifyTraversalState,
        output: &mut String,
    ) -> MustardResult<bool> {
        self.charge_native_helper_work(1)?;
        let value = self.get_property_static_at_site(holder.clone(), key, false, None)?;
        self.with_temporary_roots(&[holder.clone(), value.clone()], |runtime| {
            let mut value = value;
            if matches!(value, Value::Object(_) | Value::Array(_)) {
                let to_json =
                    runtime.get_property_static_at_site(value.clone(), "toJSON", false, None)?;
                if runtime.is_callable_value(&to_json)? {
                    value = runtime.call_json_callback(
                        to_json,
                        value.clone(),
                        &[Value::String(key.into())],
                    )?;
                }
            }
            runtime.with_temporary_roots(&[value.clone()], |runtime| {
                if let Some(callback) = &state.replacer {
                    value = runtime.call_json_callback(
                        callback.clone(),
                        holder,
                        &[Value::String(key.into()), value.clone()],
                    )?;
                }
                runtime.with_temporary_roots(&[value.clone()], |runtime| {
                    runtime.json_stringify_value(
                        &runtime.json_unbox_primitive(value.clone())?,
                        state,
                        output,
                    )
                })
            })
        })
    }

    pub(crate) fn call_json_parse(&mut self, args: &[Value]) -> MustardResult<Value> {
        self.with_temporary_roots(args, |runtime| {
            let source = runtime.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
            let mut reader = BudgetedJsonReader::new(runtime, source.as_bytes());
            let parsed: serde_json::Value = match serde_json::from_reader(&mut reader) {
                Ok(parsed) => parsed,
                Err(error) => {
                    if let Some(runtime_error) = reader.into_failure() {
                        return Err(runtime_error);
                    }
                    return Err(MustardError::runtime(format!("SyntaxError: {error}")));
                }
            };
            drop(reader);
            let value = runtime.value_from_json(parsed)?;
            let reviver = args.get(1).cloned().unwrap_or(Value::Undefined);
            if !runtime.is_callable_value(&reviver)? {
                return Ok(value);
            }
            runtime.with_temporary_roots(std::slice::from_ref(&value), |runtime| {
                let holder = Value::Object(runtime.insert_object(
                    IndexMap::from([(String::new(), value.clone())]),
                    ObjectKind::Plain,
                )?);
                runtime.json_revive_property(holder, "", reviver, 0)
            })
        })
    }

    fn json_revive_property(
        &mut self,
        holder: Value,
        key: &str,
        reviver: Value,
        depth: usize,
    ) -> MustardResult<Value> {
        if depth > JSON_MAX_DEPTH {
            return Err(limit_error("JSON reviver nesting depth limit exceeded"));
        }
        self.charge_native_helper_work(1)?;
        let value = self.get_property_static_at_site(holder.clone(), key, false, None)?;
        self.with_temporary_roots(
            &[holder.clone(), value.clone(), reviver.clone()],
            |runtime| {
                match &value {
                    Value::Array(array) => {
                        let length = runtime.array_length(*array)?;
                        for index in 0..length {
                            let key = index.to_string();
                            runtime.json_revive_child(
                                value.clone(),
                                &key,
                                reviver.clone(),
                                depth,
                            )?;
                        }
                    }
                    Value::Object(object) => {
                        let keys = runtime
                            .objects
                            .get(*object)
                            .ok_or_else(|| MustardError::runtime("object missing"))?
                            .properties
                            .ordered_keys();
                        for key in keys {
                            runtime.json_revive_child(
                                value.clone(),
                                &key,
                                reviver.clone(),
                                depth,
                            )?;
                        }
                    }
                    _ => {}
                }
                runtime.call_json_callback(reviver, holder, &[Value::String(key.into()), value])
            },
        )
    }

    fn json_revive_child(
        &mut self,
        holder: Value,
        key: &str,
        reviver: Value,
        depth: usize,
    ) -> MustardResult<()> {
        let revived = self.json_revive_property(holder.clone(), key, reviver, depth + 1)?;
        if matches!(revived, Value::Undefined) {
            self.delete_property_by_key(holder, key)
        } else {
            self.set_property_static(holder, key, revived)
        }
    }

    fn json_indent(
        &mut self,
        output: &mut String,
        state: &JsonStringifyTraversalState,
    ) -> MustardResult<()> {
        if !state.gap.is_empty() {
            self.push_json_fragment(output, "\n")?;
            for _ in 0..state.depth {
                self.push_json_fragment(output, &state.gap)?;
            }
        }
        Ok(())
    }

    fn json_stringify_value(
        &mut self,
        value: &Value,
        state: &mut JsonStringifyTraversalState,
        output: &mut String,
    ) -> MustardResult<bool> {
        self.charge_native_helper_work(1)?;
        if self.is_callable_value(value)? {
            return Ok(false);
        }
        match value {
            Value::Undefined => Ok(false),
            Value::Null => {
                self.push_json_fragment(output, "null")?;
                Ok(true)
            }
            Value::Bool(value) => {
                self.push_json_fragment(output, if *value { "true" } else { "false" })?;
                Ok(true)
            }
            Value::Number(value) => {
                self.push_json_fragment(output, &json_number_to_string(*value))?;
                Ok(true)
            }
            Value::BigInt(_) => Err(MustardError::runtime(
                "TypeError: Do not know how to serialize a BigInt",
            )),
            Value::String(value) => {
                self.push_json_string(output, value)?;
                Ok(true)
            }
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => Ok(false),
            Value::Array(array) => {
                if !state.arrays.insert(*array) {
                    return Err(json_stringify_cycle_error());
                }
                if state.depth >= JSON_MAX_DEPTH {
                    return Err(limit_error("JSON.stringify nesting depth limit exceeded"));
                }
                let length = self.array_length(*array)?;
                state.depth += 1;
                let result = (|| {
                    self.push_json_fragment(output, "[")?;
                    for index in 0..length {
                        if index > 0 {
                            self.push_json_fragment(output, ",")?;
                        }
                        self.json_indent(output, state)?;
                        if !self.json_stringify_property(
                            value.clone(),
                            &index.to_string(),
                            state,
                            output,
                        )? {
                            self.push_json_fragment(output, "null")?;
                        }
                    }
                    state.depth -= 1;
                    if length > 0 {
                        self.json_indent(output, state)?;
                    }
                    self.push_json_fragment(output, "]")?;
                    Ok(true)
                })();
                state.arrays.remove(array);
                result
            }
            Value::Object(object) => self.json_stringify_object(*object, state, output),
            Value::Map(_) | Value::Set(_) | Value::Iterator(_) | Value::Promise(_) => {
                self.push_json_fragment(output, "{}")?;
                Ok(true)
            }
        }
    }

    fn json_stringify_object(
        &mut self,
        object: ObjectKey,
        state: &mut JsonStringifyTraversalState,
        output: &mut String,
    ) -> MustardResult<bool> {
        if !state.objects.insert(object) {
            return Err(json_stringify_cycle_error());
        }
        if state.depth >= JSON_MAX_DEPTH {
            return Err(limit_error("JSON.stringify nesting depth limit exceeded"));
        }
        let keys = if let Some(keys) = &state.property_list {
            keys.clone()
        } else {
            let object_ref = self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?;
            match &object_ref.kind {
                ObjectKind::Error(_) => object_ref.properties.ordered_keys_filtered(|key, _| {
                    !matches!(key, "name" | "message" | "stack" | "cause" | "errors")
                }),
                _ => object_ref.properties.ordered_keys(),
            }
        };
        self.charge_native_helper_work(keys.len())?;
        state.depth += 1;
        let result = (|| {
            self.push_json_fragment(output, "{")?;
            let mut wrote_any = false;
            for key in keys {
                let rewind = output.len();
                if wrote_any {
                    self.push_json_fragment(output, ",")?;
                }
                self.json_indent(output, state)?;
                self.push_json_string(output, &key)?;
                self.push_json_fragment(output, if state.gap.is_empty() { ":" } else { ": " })?;
                if !self.json_stringify_property(Value::Object(object), &key, state, output)? {
                    output.truncate(rewind);
                    continue;
                }
                wrote_any = true;
            }
            state.depth -= 1;
            if wrote_any {
                self.json_indent(output, state)?;
            }
            self.push_json_fragment(output, "}")?;
            Ok(true)
        })();
        state.objects.remove(&object);
        result
    }

    fn push_json_fragment(&mut self, output: &mut String, fragment: &str) -> MustardResult<()> {
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

    fn push_json_string(&mut self, output: &mut String, value: &str) -> MustardResult<()> {
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
                    Err(MustardError::runtime(error.to_string()))
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

fn json_stringify_cycle_error() -> MustardError {
    MustardError::runtime("TypeError: Converting circular structure to JSON")
}
