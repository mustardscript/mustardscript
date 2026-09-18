use super::*;
use num_traits::ToPrimitive;
use oxc_syntax::number::ToJsString;

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
            Value::String(value) => parse_string_number(&value),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::NumberObject(value) => *value,
                ObjectKind::StringObject(value) => parse_string_number(value),
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

    pub(in crate::runtime) fn coerce_number(&mut self, value: Value) -> MustardResult<f64> {
        let work = match &value {
            Value::String(text) => text.len(),
            Value::Object(id) => match &self
                .objects
                .get(*id)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::StringObject(text) => text.len(),
                _ => 0,
            },
            _ => 0,
        };
        self.charge_native_helper_work(work)?;
        self.to_number(value)
    }

    pub(in crate::runtime) fn coerce_uint32(&mut self, value: Value) -> MustardResult<u32> {
        let value = self.coerce_number(value)?;
        Ok(if !value.is_finite() || value == 0.0 {
            0
        } else {
            value.trunc().rem_euclid(4294967296.0) as u32
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
        self.to_string_guarded(value, &mut Vec::new(), &mut 0)
    }

    fn to_string_guarded(
        &self,
        value: Value,
        active: &mut Vec<ArrayKey>,
        work: &mut usize,
    ) -> MustardResult<String> {
        Ok(match value {
            Value::Undefined => "undefined".to_string(),
            Value::Null => "null".to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_js_string(),
            Value::BigInt(value) => value.to_string(),
            Value::String(value) => value,
            Value::Array(array) => self.stringify_array(array, ",", active, work)?,
            Value::Map(_) => "[object Map]".to_string(),
            Value::Set(_) => "[object Set]".to_string(),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::NullPrototype => {
                    return Err(MustardError::runtime(
                        "TypeError: cannot convert prototype-less object to a primitive value",
                    ));
                }
                ObjectKind::Date(date) => Self::date_default_string(date.timestamp_ms),
                ObjectKind::RegExp(regex) => format!("/{}/{}", regex.pattern, regex.flags),
                ObjectKind::NumberObject(value) => value.to_js_string(),
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

    pub(in crate::runtime) fn stringify_array(
        &self,
        array: ArrayKey,
        separator: &str,
        active: &mut Vec<ArrayKey>,
        work: &mut usize,
    ) -> MustardResult<String> {
        if active.contains(&array) {
            return Ok(String::new());
        }
        if active.len() >= 128 {
            return Err(MustardError::runtime(
                "RangeError: array string conversion nesting limit exceeded",
            ));
        }
        active.push(array);
        let array = self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?;
        let mut result = String::new();
        for (index, value) in array.elements.iter().enumerate() {
            *work = work.saturating_add(1);
            if *work > self.limits.instruction_budget {
                return Err(limit_error("instruction budget exceeded"));
            }
            let text = match value {
                None | Some(Value::Null | Value::Undefined) => String::new(),
                Some(value) => self.to_string_guarded(value.clone(), active, work)?,
            };
            let separator = if index == 0 { "" } else { separator };
            let length = result
                .len()
                .saturating_add(separator.len())
                .saturating_add(text.len());
            if length > self.limits.heap_limit_bytes {
                return Err(limit_error("heap limit exceeded"));
            }
            result.push_str(separator);
            result.push_str(&text);
        }
        active.pop();
        Ok(result)
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

    pub(in crate::runtime) fn callable_display_string(
        &self,
        value: &Value,
    ) -> MustardResult<String> {
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

// ECMAScript WhiteSpace + LineTerminator, intentionally not Rust's broader trim set.
pub(in crate::runtime) fn is_ecmascript_whitespace(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000D}' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}')
}

fn parse_string_number(value: &str) -> f64 {
    let value = value.trim_matches(is_ecmascript_whitespace);
    if value.is_empty() {
        return 0.0;
    }
    match value {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefixes, radix) in [(["0x", "0X"], 16), (["0o", "0O"], 8), (["0b", "0B"], 2)] {
        if let Some(digits) = prefixes
            .iter()
            .find_map(|prefix| value.strip_prefix(prefix))
        {
            if digits.is_empty() || !digits.chars().all(|ch| ch.is_digit(radix)) {
                return f64::NAN;
            }
            return num_bigint::BigUint::parse_bytes(digits.as_bytes(), radix)
                .map_or(f64::NAN, |value| value.to_f64().unwrap_or(f64::INFINITY));
        }
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || b"+-.eE".contains(&byte))
    {
        return f64::NAN;
    }
    value.parse::<f64>().unwrap_or(f64::NAN)
}
