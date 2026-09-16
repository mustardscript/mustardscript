use super::*;
use crate::runtime::conversions::is_ecmascript_whitespace;
use num_bigint::BigInt;
use num_traits::FromPrimitive;

impl Runtime {
    pub(crate) fn call_bigint_conversion(&mut self, args: &[Value]) -> MustardResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let value = match value {
            Value::Object(id) => {
                let object = self
                    .objects
                    .get(id)
                    .ok_or_else(|| MustardError::runtime("object missing"))?;
                if object.properties.contains_key("valueOf")
                    || object.properties.contains_key("toString")
                {
                    return Err(MustardError::runtime(
                        "TypeError: BigInt conversion does not support object coercion hooks",
                    ));
                }
                match &object.kind {
                    ObjectKind::NumberObject(value) => Value::Number(*value),
                    ObjectKind::BooleanObject(value) => Value::Bool(*value),
                    ObjectKind::StringObject(value) => Value::String(value.clone()),
                    _ => {
                        return Err(MustardError::runtime(
                            "TypeError: BigInt conversion does not support object coercion hooks",
                        ));
                    }
                }
            }
            value => value,
        };
        let value = match value {
            Value::BigInt(value) => value,
            Value::Bool(value) => BigInt::from(u8::from(value)),
            Value::Number(value) => {
                if !value.is_finite() || value.fract() != 0.0 {
                    return Err(MustardError::runtime(
                        "RangeError: BigInt conversion requires a finite integer Number",
                    ));
                }
                BigInt::from_f64(value).ok_or_else(|| {
                    MustardError::runtime("RangeError: invalid BigInt Number input")
                })?
            }
            Value::String(value) => {
                self.charge_native_helper_work(value.len())?;
                let text = value.trim_matches(is_ecmascript_whitespace);
                if text.is_empty() {
                    return Ok(Value::BigInt(BigInt::from(0)));
                }
                let (digits, radix, negative) = if let Some(digits) =
                    text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
                {
                    (digits, 16, false)
                } else if let Some(digits) =
                    text.strip_prefix("0o").or_else(|| text.strip_prefix("0O"))
                {
                    (digits, 8, false)
                } else if let Some(digits) =
                    text.strip_prefix("0b").or_else(|| text.strip_prefix("0B"))
                {
                    (digits, 2, false)
                } else {
                    (
                        text.strip_prefix(['+', '-']).unwrap_or(text),
                        10,
                        text.starts_with('-'),
                    )
                };
                if digits.is_empty() || !digits.chars().all(|ch| ch.is_digit(radix)) {
                    return Err(MustardError::runtime("SyntaxError: invalid BigInt string"));
                }
                // Decimal conversion performs growing multiplications; charge its quadratic
                // component before parsing. Power-of-two radix conversion is linear.
                if radix == 10 {
                    self.charge_native_helper_work(digits.len().saturating_mul(digits.len()) / 64)?;
                }
                self.ensure_heap_capacity(digits.len().saturating_mul(3).saturating_add(128))?;
                let value = BigInt::parse_bytes(digits.as_bytes(), radix)
                    .ok_or_else(|| MustardError::runtime("SyntaxError: invalid BigInt string"))?;
                if negative { -value } else { value }
            }
            _ => {
                return Err(MustardError::runtime(
                    "TypeError: value cannot be converted to BigInt",
                ));
            }
        };
        Ok(Value::BigInt(value))
    }

    fn bigint_receiver(&self, value: Value) -> MustardResult<BigInt> {
        match value {
            Value::BigInt(value) => Ok(value),
            _ => Err(MustardError::runtime(
                "TypeError: BigInt method called on incompatible receiver",
            )),
        }
    }

    pub(crate) fn call_bigint_value_of(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::BigInt(self.bigint_receiver(this_value)?))
    }

    pub(crate) fn call_bigint_to_string(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.bigint_receiver(this_value)?;
        let radix = match args.first() {
            None | Some(Value::Undefined) => 10,
            Some(value) => self.to_integer(value.clone())?,
        };
        if !(2..=36).contains(&radix) {
            return Err(MustardError::runtime(
                "RangeError: BigInt.toString radix must be between 2 and 36",
            ));
        }
        let bits = usize::try_from(value.bits()).unwrap_or(usize::MAX);
        self.charge_native_helper_work(bits.saturating_add(bits.saturating_mul(bits) / 64))?;
        self.ensure_heap_capacity(bits.saturating_add(1))?;
        Ok(Value::String(value.to_str_radix(radix as u32)))
    }
}
