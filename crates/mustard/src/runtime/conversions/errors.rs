use super::*;

impl Runtime {
    pub(in crate::runtime) fn make_error_object(
        &mut self,
        name: &str,
        args: &[Value],
        code: Option<String>,
        details: Option<Value>,
        cause: Option<Option<Value>>,
    ) -> MustardResult<Value> {
        let message = match args.first() {
            Some(Value::Undefined) | None => String::new(),
            Some(value) => self.to_string(value.clone())?,
        };
        let mut stack = if message.is_empty() {
            name.to_string()
        } else {
            format!("{name}: {message}")
        };
        for frame in self.traceback_frames() {
            self.charge_native_helper_work(1)?;
            stack.push_str(&format!(
                "\n    at {} (guest:{}..{})",
                frame.function_name.as_deref().unwrap_or("anonymous"),
                frame.span.start,
                frame.span.end
            ));
            self.ensure_heap_capacity(stack.len())?;
        }
        let mut properties = IndexMap::from([
            ("name".to_string(), Value::String(name.to_string())),
            ("message".to_string(), Value::String(message)),
            ("stack".to_string(), Value::String(stack)),
        ]);
        if let Some(code) = code {
            properties.insert("code".to_string(), Value::String(code));
        }
        if let Some(details) = details {
            properties.insert("details".to_string(), details);
        }
        if let Some(cause) = cause {
            properties.insert("cause".to_string(), cause.unwrap_or(Value::Undefined));
        }
        let object = self.insert_object(properties, ObjectKind::Error(name.to_string()))?;
        Ok(Value::Object(object))
    }

    pub(in crate::runtime) fn value_from_runtime_message(
        &mut self,
        message: &str,
    ) -> MustardResult<Value> {
        let (name, detail) = match message.split_once(": ") {
            Some((name, detail)) if name == "Error" || name.ends_with("Error") => {
                (name.to_string(), detail.to_string())
            }
            _ => ("Error".to_string(), message.to_string()),
        };
        self.make_error_object(&name, &[Value::String(detail)], None, None, None)
    }

    pub(in crate::runtime) fn value_from_host_error(
        &mut self,
        error: HostError,
    ) -> MustardResult<Value> {
        let details = match error.details {
            Some(details) => Some(self.value_from_structured(details)?),
            None => None,
        };
        self.make_error_object(
            &error.name,
            &[Value::String(error.message)],
            error.code,
            details,
            None,
        )
    }

    pub(in crate::runtime) fn render_exception(&self, value: &Value) -> MustardResult<String> {
        match value {
            Value::Object(object) => {
                if let Some(summary) = self.error_summary(*object)? {
                    Ok(summary)
                } else {
                    self.to_string(value.clone())
                }
            }
            _ => self.to_string(value.clone()),
        }
    }

    pub(in crate::runtime) fn error_summary(
        &self,
        object: ObjectKey,
    ) -> MustardResult<Option<String>> {
        let object = self
            .objects
            .get(object)
            .ok_or_else(|| MustardError::runtime("object missing"))?;
        let details = object.properties.get("details").cloned();
        let name = object.properties.get("name").and_then(|value| match value {
            Value::String(value) => Some(value.as_str()),
            _ => None,
        });
        let message = object
            .properties
            .get("message")
            .and_then(|value| match value {
                Value::String(value) => Some(value.as_str()),
                _ => None,
            });

        if !matches!(object.kind, ObjectKind::Error(_)) && name.is_none() && message.is_none() {
            return Ok(None);
        }

        let mut summary = match (name, message) {
            (Some(name), Some("")) => name.to_string(),
            (Some(name), Some(message)) => format!("{name}: {message}"),
            (Some(name), None) => name.to_string(),
            (None, Some(message)) => message.to_string(),
            (None, None) => "Error".to_string(),
        };

        if let Some(Value::String(code)) = object.properties.get("code") {
            summary.push_str(&format!(" [code={code}]"));
        }
        if let Some(details) = details {
            summary.push_str(&format!(" [details={}]", self.to_string(details)?));
        }

        Ok(Some(summary))
    }
}

impl Runtime {
    pub(in crate::runtime) fn builtin_error_name(
        function: BuiltinFunction,
    ) -> Option<&'static str> {
        Some(match function {
            BuiltinFunction::ErrorCtor => "Error",
            BuiltinFunction::TypeErrorCtor => "TypeError",
            BuiltinFunction::ReferenceErrorCtor => "ReferenceError",
            BuiltinFunction::RangeErrorCtor => "RangeError",
            BuiltinFunction::SyntaxErrorCtor => "SyntaxError",
            BuiltinFunction::EvalErrorCtor => "EvalError",
            BuiltinFunction::URIErrorCtor => "URIError",
            BuiltinFunction::AggregateErrorCtor => "AggregateError",
            _ => return None,
        })
    }

    pub(in crate::runtime) fn call_error_to_string(&self, receiver: Value) -> MustardResult<Value> {
        if matches!(
            receiver,
            Value::Undefined
                | Value::Null
                | Value::Number(_)
                | Value::String(_)
                | Value::Bool(_)
                | Value::BigInt(_)
        ) {
            return Err(MustardError::runtime(
                "TypeError: Error.prototype.toString called on incompatible receiver",
            ));
        }
        let name = self.get_property_by_key(receiver.clone(), "name", false)?;
        let name = if matches!(name, Value::Undefined) {
            "Error".into()
        } else {
            self.to_string(name)?
        };
        let message = self.get_property_by_key(receiver, "message", false)?;
        let message = if matches!(message, Value::Undefined) {
            String::new()
        } else {
            self.to_string(message)?
        };
        Ok(Value::String(if name.is_empty() {
            message
        } else if message.is_empty() {
            name
        } else {
            format!("{name}: {message}")
        }))
    }

    pub(in crate::runtime) fn call_aggregate_error_ctor(
        &mut self,
        args: &[Value],
    ) -> MustardResult<Value> {
        self.with_temporary_roots(args, |runtime| {
            let error = runtime.call_error_ctor(
                &[
                    args.get(1).cloned().unwrap_or(Value::Undefined),
                    args.get(2).cloned().unwrap_or(Value::Undefined),
                ],
                "AggregateError",
            )?;
            runtime.with_temporary_roots(std::slice::from_ref(&error), |runtime| {
                let iterator =
                    runtime.create_iterator(args.first().cloned().unwrap_or(Value::Undefined))?;
                runtime.with_temporary_roots(std::slice::from_ref(&iterator), |runtime| {
                    let errors = Value::Array(runtime.insert_array(Vec::new(), IndexMap::new())?);
                    runtime.set_property_static(error.clone(), "errors", errors.clone())?;
                    let Value::Array(array) = errors else {
                        unreachable!()
                    };
                    loop {
                        runtime.charge_native_helper_work(1)?;
                        let (value, done) = runtime.iterator_next(iterator.clone())?;
                        if done {
                            break;
                        }
                        runtime.push_array_element(array, Some(value))?;
                    }
                    Ok(error.clone())
                })
            })
        })
    }
}
