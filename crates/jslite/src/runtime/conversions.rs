use std::collections::HashSet;

use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};

use super::*;

#[derive(Default)]
struct StructuredTraversalState {
    arrays: HashSet<ArrayKey>,
    objects: HashSet<ObjectKey>,
}

impl Runtime {
    pub(super) fn apply_unary(&mut self, operator: UnaryOp, value: Value) -> JsliteResult<Value> {
        match operator {
            UnaryOp::Plus => match value {
                Value::BigInt(_) => Err(JsliteError::runtime(
                    "TypeError: unary plus is not supported for BigInt values",
                )),
                other => Ok(Value::Number(self.to_number(other)?)),
            },
            UnaryOp::Minus => match value {
                Value::BigInt(value) => Ok(Value::BigInt(-value)),
                other => Ok(Value::Number(-self.to_number(other)?)),
            },
            UnaryOp::Not => Ok(Value::Bool(!is_truthy(&value))),
            UnaryOp::Typeof => Ok(Value::String(
                match value {
                    Value::Undefined => "undefined",
                    Value::Null => "object",
                    Value::Bool(_) => "boolean",
                    Value::Number(_) => "number",
                    Value::BigInt(_) => "bigint",
                    Value::String(_) => "string",
                    Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => {
                        "function"
                    }
                    Value::Object(_)
                    | Value::Array(_)
                    | Value::Map(_)
                    | Value::Set(_)
                    | Value::Iterator(_)
                    | Value::Promise(_) => "object",
                }
                .to_string(),
            )),
            UnaryOp::Void => Ok(Value::Undefined),
        }
    }

    pub(super) fn apply_binary(
        &mut self,
        operator: BinaryOp,
        left: Value,
        right: Value,
    ) -> JsliteResult<Value> {
        match operator {
            BinaryOp::Add => {
                if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                    Ok(Value::String(format!(
                        "{}{}",
                        self.to_string(left)?,
                        self.to_string(right)?
                    )))
                } else if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) {
                    self.apply_bigint_binary(BinaryOp::Add, left, right)
                } else {
                    Ok(Value::Number(
                        self.to_number(left)? + self.to_number(right)?,
                    ))
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem | BinaryOp::Pow
                if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) =>
            {
                self.apply_bigint_binary(operator, left, right)
            }
            BinaryOp::Sub => Ok(Value::Number(
                self.to_number(left)? - self.to_number(right)?,
            )),
            BinaryOp::Mul => Ok(Value::Number(
                self.to_number(left)? * self.to_number(right)?,
            )),
            BinaryOp::Div => Ok(Value::Number(
                self.to_number(left)? / self.to_number(right)?,
            )),
            BinaryOp::Rem => Ok(Value::Number(
                self.to_number(left)? % self.to_number(right)?,
            )),
            BinaryOp::Pow => Ok(Value::Number(
                self.to_number(left)?.powf(self.to_number(right)?),
            )),
            BinaryOp::In => Ok(Value::Bool(
                self.has_property_in_supported_surface(right, left)?,
            )),
            BinaryOp::Eq | BinaryOp::StrictEq => Ok(Value::Bool(strict_equal(&left, &right))),
            BinaryOp::NotEq | BinaryOp::StrictNotEq => {
                Ok(Value::Bool(!strict_equal(&left, &right)))
            }
            BinaryOp::LessThan
            | BinaryOp::LessThanEq
            | BinaryOp::GreaterThan
            | BinaryOp::GreaterThanEq
                if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) =>
            {
                let (left, right) = bigint_operands(left, right, mixed_bigint_comparison_error)?;
                Ok(Value::Bool(match operator {
                    BinaryOp::LessThan => left < right,
                    BinaryOp::LessThanEq => left <= right,
                    BinaryOp::GreaterThan => left > right,
                    BinaryOp::GreaterThanEq => left >= right,
                    _ => unreachable!(),
                }))
            }
            BinaryOp::LessThan => Ok(Value::Bool(self.to_number(left)? < self.to_number(right)?)),
            BinaryOp::LessThanEq => {
                Ok(Value::Bool(self.to_number(left)? <= self.to_number(right)?))
            }
            BinaryOp::GreaterThan => {
                Ok(Value::Bool(self.to_number(left)? > self.to_number(right)?))
            }
            BinaryOp::GreaterThanEq => {
                Ok(Value::Bool(self.to_number(left)? >= self.to_number(right)?))
            }
        }
    }

    pub(super) fn to_number(&self, value: Value) -> JsliteResult<f64> {
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
            Value::Array(_)
            | Value::Map(_)
            | Value::Set(_)
            | Value::Iterator(_)
            | Value::Object(_)
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

    pub(super) fn to_integer(&self, value: Value) -> JsliteResult<i64> {
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

    pub(super) fn to_string(&self, value: Value) -> JsliteResult<String> {
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
                    parts.push(self.to_string(value.clone())?);
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

    pub(super) fn make_error_object(
        &mut self,
        name: &str,
        args: &[Value],
        code: Option<String>,
        details: Option<Value>,
    ) -> JsliteResult<Value> {
        let message = if let Some(value) = args.first() {
            self.to_string(value.clone())?
        } else {
            String::new()
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
        let object = self.insert_object(properties, ObjectKind::Error(name.to_string()))?;
        Ok(Value::Object(object))
    }

    pub(super) fn value_from_runtime_message(&mut self, message: &str) -> JsliteResult<Value> {
        let (name, detail) = match message.split_once(": ") {
            Some((name, detail)) if name == "Error" || name.ends_with("Error") => {
                (name.to_string(), detail.to_string())
            }
            _ => ("Error".to_string(), message.to_string()),
        };
        self.make_error_object(&name, &[Value::String(detail)], None, None)
    }

    pub(super) fn value_from_host_error(&mut self, error: HostError) -> JsliteResult<Value> {
        let details = match error.details {
            Some(details) => Some(self.value_from_structured(details)?),
            None => None,
        };
        self.make_error_object(
            &error.name,
            &[Value::String(error.message)],
            error.code,
            details,
        )
    }

    pub(super) fn render_exception(&self, value: &Value) -> JsliteResult<String> {
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

    pub(super) fn error_summary(&self, object: ObjectKey) -> JsliteResult<Option<String>> {
        let object = self
            .objects
            .get(object)
            .ok_or_else(|| JsliteError::runtime("object missing"))?;
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
        if let Some(details) = object.properties.get("details") {
            match self.value_to_structured(details.clone()) {
                Ok(details) => summary.push_str(&format!(" [details={details:?}]")),
                Err(_) => {
                    summary.push_str(&format!(" [details={}]", self.to_string(details.clone())?))
                }
            }
        }

        Ok(Some(summary))
    }

    pub(super) fn to_property_key(&self, value: Value) -> JsliteResult<String> {
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

    pub(super) fn to_array_items(&self, value: Value) -> JsliteResult<Vec<Value>> {
        match value {
            Value::Array(array) => self
                .arrays
                .get(array)
                .map(|array| array.elements.clone())
                .ok_or_else(|| JsliteError::runtime("array missing")),
            Value::Undefined | Value::Null => Ok(Vec::new()),
            _ => Err(JsliteError::runtime(
                "value is not destructurable as an array",
            )),
        }
    }

    pub(super) fn value_from_structured(&mut self, value: StructuredValue) -> JsliteResult<Value> {
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

    pub(super) fn value_to_structured(&self, value: Value) -> JsliteResult<StructuredValue> {
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

    pub(super) fn value_from_json(&mut self, value: serde_json::Value) -> JsliteResult<Value> {
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

impl Runtime {
    fn apply_bigint_binary(
        &self,
        operator: BinaryOp,
        left: Value,
        right: Value,
    ) -> JsliteResult<Value> {
        let (left, right) = bigint_operands(left, right, mixed_bigint_number_error)?;
        match operator {
            BinaryOp::Add => Ok(Value::BigInt(left + right)),
            BinaryOp::Sub => Ok(Value::BigInt(left - right)),
            BinaryOp::Mul => Ok(Value::BigInt(left * right)),
            BinaryOp::Div => {
                if right.is_zero() {
                    Err(JsliteError::runtime("RangeError: BigInt division by zero"))
                } else {
                    Ok(Value::BigInt(left / right))
                }
            }
            BinaryOp::Rem => {
                if right.is_zero() {
                    Err(JsliteError::runtime("RangeError: BigInt division by zero"))
                } else {
                    Ok(Value::BigInt(left % right))
                }
            }
            BinaryOp::Pow => {
                if right < BigInt::zero() {
                    return Err(JsliteError::runtime(
                        "RangeError: BigInt exponent must be non-negative",
                    ));
                }
                let exponent = right.to_u32().ok_or_else(|| {
                    JsliteError::runtime("RangeError: BigInt exponent is too large")
                })?;
                Ok(Value::BigInt(left.pow(exponent)))
            }
            _ => unreachable!(),
        }
    }
}

fn structured_boundary_cycle_error() -> JsliteError {
    JsliteError::runtime("cyclic values cannot cross the structured host boundary")
}

fn bigint_operands(
    left: Value,
    right: Value,
    mixed_error: fn() -> JsliteError,
) -> JsliteResult<(BigInt, BigInt)> {
    match (left, right) {
        (Value::BigInt(left), Value::BigInt(right)) => Ok((left, right)),
        (Value::BigInt(_), _) | (_, Value::BigInt(_)) => Err(mixed_error()),
        _ => unreachable!(),
    }
}

fn mixed_bigint_number_error() -> JsliteError {
    JsliteError::runtime("TypeError: cannot mix BigInt and Number values in arithmetic")
}

fn mixed_bigint_comparison_error() -> JsliteError {
    JsliteError::runtime("TypeError: cannot compare BigInt and Number values")
}
