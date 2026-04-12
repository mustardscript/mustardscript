use super::*;

impl Runtime {
    pub(in crate::runtime) fn make_error_object(
        &mut self,
        name: &str,
        args: &[Value],
        code: Option<String>,
        details: Option<Value>,
        cause: Option<Option<Value>>,
    ) -> JsliteResult<Value> {
        let message = match args.first() {
            Some(Value::Undefined) | None => String::new(),
            Some(value) => self.to_string(value.clone())?,
        };
        let mut properties = IndexMap::from([
            ("name".to_string(), Value::String(name.to_string())),
            ("message".to_string(), Value::String(message)),
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
    ) -> JsliteResult<Value> {
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
    ) -> JsliteResult<Value> {
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

    pub(in crate::runtime) fn render_exception(&self, value: &Value) -> JsliteResult<String> {
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
    ) -> JsliteResult<Option<String>> {
        let object = self
            .objects
            .get(object)
            .ok_or_else(|| JsliteError::runtime("object missing"))?;
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
