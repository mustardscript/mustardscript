use oxc_syntax::number::ToJsString;
#[cfg(not(target_arch = "wasm32"))]
use rand::random;

use super::*;

const NUMBER_PARSE_HELPER_CHUNK_CHARS: usize = 256;

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn mustard_random_f64() -> f64;
}

#[cfg(target_arch = "wasm32")]
fn math_random_f64() -> f64 {
    unsafe { mustard_random_f64() }
}

#[cfg(not(target_arch = "wasm32"))]
fn math_random_f64() -> f64 {
    random::<f64>()
}

impl Runtime {
    pub(in crate::runtime) fn number_receiver(
        &self,
        value: Value,
        method: &str,
    ) -> MustardResult<f64> {
        match value {
            Value::Number(value) => Ok(value),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::NumberObject(value) => Ok(*value),
                ObjectKind::FunctionPrototype(Value::BuiltinFunction(
                    BuiltinFunction::NumberCtor,
                )) => Ok(0.0),
                _ => Err(MustardError::runtime(format!(
                    "TypeError: Number.prototype.{method} called on incompatible receiver",
                ))),
            },
            _ => Err(MustardError::runtime(format!(
                "TypeError: Number.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn boolean_receiver(&self, value: Value, method: &str) -> MustardResult<bool> {
        match value {
            Value::Bool(value) => Ok(value),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::BooleanObject(value) => Ok(*value),
                _ => Err(MustardError::runtime(format!(
                    "TypeError: Boolean.prototype.{method} called on incompatible receiver",
                ))),
            },
            _ => Err(MustardError::runtime(format!(
                "TypeError: Boolean.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn call_error_ctor(&mut self, args: &[Value], name: &str) -> MustardResult<Value> {
        let options = args.get(1).cloned().unwrap_or(Value::Undefined);
        let cause = self.error_options_cause(options)?;
        self.make_error_object(name, args, None, None, cause)
    }

    pub(crate) fn call_number_ctor(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(self.to_number(
            args.first().cloned().unwrap_or(Value::Undefined),
        )?))
    }

    pub(crate) fn call_number_parse_int(&mut self, args: &[Value]) -> MustardResult<Value> {
        let input = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        let trimmed = input.trim_start();
        let radix_value = args.get(1).cloned().unwrap_or(Value::Undefined);
        let radix = if matches!(radix_value, Value::Undefined) {
            None
        } else {
            let parsed = self.to_integer(radix_value)?;
            if !(2..=36).contains(&parsed) {
                return Ok(Value::Number(f64::NAN));
            }
            Some(parsed as u32)
        };

        let (sign, remainder) = if let Some(stripped) = trimmed.strip_prefix('+') {
            (1.0, stripped)
        } else if let Some(stripped) = trimmed.strip_prefix('-') {
            (-1.0, stripped)
        } else {
            (1.0, trimmed)
        };
        let (radix, digits) =
            if radix.is_none() && (remainder.starts_with("0x") || remainder.starts_with("0X")) {
                (16u32, &remainder[2..])
            } else {
                (radix.unwrap_or(10), remainder)
            };
        let mut end = 0usize;
        let mut saw_digit = false;
        for (index, ch) in digits.char_indices() {
            if index % NUMBER_PARSE_HELPER_CHUNK_CHARS == 0 {
                self.charge_native_helper_work(1)?;
            }
            if ch.to_digit(radix).is_none() {
                break;
            }
            saw_digit = true;
            end = index + ch.len_utf8();
        }
        if !saw_digit {
            return Ok(Value::Number(f64::NAN));
        }
        let parsed = i128::from_str_radix(&digits[..end], radix).unwrap_or(0) as f64 * sign;
        Ok(Value::Number(parsed))
    }

    pub(crate) fn call_number_parse_float(&mut self, args: &[Value]) -> MustardResult<Value> {
        let input = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        let trimmed = input.trim_start();
        if trimmed.starts_with("Infinity") || trimmed.starts_with("+Infinity") {
            return Ok(Value::Number(f64::INFINITY));
        }
        if trimmed.starts_with("-Infinity") {
            return Ok(Value::Number(f64::NEG_INFINITY));
        }
        let mut end = 0usize;
        let mut seen_digit = false;
        let mut seen_dot = false;
        let mut seen_exp = false;
        let mut allow_sign = true;
        for (index, ch) in trimmed.char_indices() {
            if index % NUMBER_PARSE_HELPER_CHUNK_CHARS == 0 {
                self.charge_native_helper_work(1)?;
            }
            let accepted = if allow_sign && matches!(ch, '+' | '-') {
                allow_sign = false;
                true
            } else if ch.is_ascii_digit() {
                seen_digit = true;
                allow_sign = false;
                true
            } else if ch == '.' && !seen_dot && !seen_exp {
                seen_dot = true;
                allow_sign = false;
                true
            } else if matches!(ch, 'e' | 'E') && seen_digit && !seen_exp {
                seen_exp = true;
                allow_sign = true;
                true
            } else {
                false
            };
            if !accepted {
                break;
            }
            end = index + ch.len_utf8();
        }
        let parsed = trimmed[..end].parse::<f64>().unwrap_or(f64::NAN);
        Ok(Value::Number(parsed))
    }

    pub(crate) fn call_number_is_nan(&self, args: &[Value]) -> Value {
        Value::Bool(matches!(args.first(), Some(Value::Number(value)) if value.is_nan()))
    }

    pub(crate) fn call_number_is_finite(&self, args: &[Value]) -> Value {
        Value::Bool(matches!(args.first(), Some(Value::Number(value)) if value.is_finite()))
    }

    pub(crate) fn call_number_is_integer(&self, args: &[Value]) -> Value {
        Value::Bool(matches!(
            args.first(),
            Some(Value::Number(value)) if value.is_finite() && value.fract() == 0.0
        ))
    }

    pub(crate) fn call_number_is_safe_integer(&self, args: &[Value]) -> Value {
        Value::Bool(matches!(
            args.first(),
            Some(Value::Number(value))
                if value.is_finite()
                    && value.fract() == 0.0
                    && value.abs() <= 9_007_199_254_740_991.0
        ))
    }

    pub(crate) fn call_string_ctor(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::String(self.to_string(
            args.first().cloned().unwrap_or(Value::Undefined),
        )?))
    }

    pub(crate) fn call_boolean_ctor(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Bool(is_truthy(
            args.first().unwrap_or(&Value::Undefined),
        )))
    }

    pub(crate) fn call_number_to_string(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let number = self.number_receiver(this_value, "toString")?;
        let radix = match args.first() {
            None | Some(Value::Undefined) => 10,
            Some(value) => self.to_integer(value.clone())?,
        };
        if !(2..=36).contains(&radix) {
            return Err(MustardError::runtime(
                "RangeError: Number.prototype.toString radix must be between 2 and 36",
            ));
        }
        Ok(Value::String(number_to_radix_string(number, radix as u32)))
    }

    pub(crate) fn call_number_value_of(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Number(self.number_receiver(this_value, "valueOf")?))
    }

    pub(crate) fn call_number_to_fixed(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let number = self.number_receiver(this_value, "toFixed")?;
        let digits = self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?;
        if !(0..=100).contains(&digits) {
            return Err(MustardError::runtime(
                "RangeError: Number.prototype.toFixed digits must be between 0 and 100",
            ));
        }

        if number.is_nan() {
            return Ok(Value::String("NaN".to_string()));
        }
        if number.is_infinite() {
            return Ok(Value::String(if number.is_sign_negative() {
                "-Infinity".to_string()
            } else {
                "Infinity".to_string()
            }));
        }
        if number.abs() >= 1e21 {
            return Ok(Value::String(number.to_js_string()));
        }

        let digits = digits as usize;
        let rendered = format!("{:.*}", digits, number.abs());
        if number.is_sign_negative() && number != 0.0 {
            Ok(Value::String(format!("-{rendered}")))
        } else {
            Ok(Value::String(rendered))
        }
    }

    pub(crate) fn call_number_to_exponential(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let number = self.number_receiver(this_value, "toExponential")?;
        let digits = match args.first() {
            None | Some(Value::Undefined) => None,
            Some(value) => {
                let digits = self.to_integer(value.clone())?;
                if !(0..=100).contains(&digits) {
                    return Err(MustardError::runtime(
                        "RangeError: Number.prototype.toExponential digits must be between 0 and 100",
                    ));
                }
                Some(digits as usize)
            }
        };
        Ok(Value::String(number_to_exponential_string(number, digits)))
    }

    pub(crate) fn call_number_to_precision(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let number = self.number_receiver(this_value, "toPrecision")?;
        let precision = match args.first() {
            None | Some(Value::Undefined) => return Ok(Value::String(number.to_js_string())),
            Some(value) => self.to_integer(value.clone())?,
        };
        if !(1..=100).contains(&precision) {
            return Err(MustardError::runtime(
                "RangeError: Number.prototype.toPrecision precision must be between 1 and 100",
            ));
        }
        Ok(Value::String(number_to_precision_string(
            number,
            precision as usize,
        )))
    }

    pub(crate) fn call_boolean_to_string(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::String(self.to_string(Value::Bool(
            self.boolean_receiver(this_value, "toString")?,
        ))?))
    }

    pub(crate) fn call_boolean_value_of(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Bool(self.boolean_receiver(this_value, "valueOf")?))
    }

    pub(crate) fn construct_number(&mut self, args: &[Value]) -> MustardResult<Value> {
        let value = self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?;
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::NumberObject(value),
        )?))
    }

    pub(crate) fn construct_string(&mut self, args: &[Value]) -> MustardResult<Value> {
        let value = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::StringObject(value),
        )?))
    }

    pub(crate) fn construct_boolean(&mut self, args: &[Value]) -> MustardResult<Value> {
        let value = is_truthy(args.first().unwrap_or(&Value::Undefined));
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::BooleanObject(value),
        )?))
    }

    pub(crate) fn call_math_abs(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .abs(),
        ))
    }

    pub(crate) fn call_math_max(&self, args: &[Value]) -> MustardResult<Value> {
        let mut value = f64::NEG_INFINITY;
        for arg in args {
            value = value.max(self.to_number(arg.clone())?);
        }
        Ok(Value::Number(value))
    }

    pub(crate) fn call_math_min(&self, args: &[Value]) -> MustardResult<Value> {
        let mut value = f64::INFINITY;
        for arg in args {
            value = value.min(self.to_number(arg.clone())?);
        }
        Ok(Value::Number(value))
    }

    pub(crate) fn call_math_floor(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .floor(),
        ))
    }

    pub(crate) fn call_math_ceil(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .ceil(),
        ))
    }

    pub(crate) fn call_math_round(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .round(),
        ))
    }

    pub(crate) fn call_math_pow(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .powf(self.to_number(args.get(1).cloned().unwrap_or(Value::Undefined))?),
        ))
    }

    pub(crate) fn call_math_sqrt(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .sqrt(),
        ))
    }

    pub(crate) fn call_math_trunc(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .trunc(),
        ))
    }

    pub(crate) fn call_math_sign(&self, args: &[Value]) -> MustardResult<Value> {
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

    pub(crate) fn call_math_log(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .ln(),
        ))
    }

    pub(crate) fn call_math_exp(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .exp(),
        ))
    }

    pub(crate) fn call_math_log2(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .log2(),
        ))
    }

    pub(crate) fn call_math_log10(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .log10(),
        ))
    }

    pub(crate) fn call_math_completion(
        &mut self,
        function: BuiltinFunction,
        args: &[Value],
    ) -> MustardResult<Value> {
        let first = args.first().cloned().unwrap_or(Value::Undefined);
        if function == BuiltinFunction::MathClz32 {
            return Ok(Value::Number(
                self.coerce_uint32(first)?.leading_zeros() as f64
            ));
        }
        if function == BuiltinFunction::MathImul {
            let left = self.coerce_uint32(first)?;
            let right = self.coerce_uint32(args.get(1).cloned().unwrap_or(Value::Undefined))?;
            return Ok(Value::Number(left.wrapping_mul(right) as i32 as f64));
        }
        let value = self.coerce_number(first)?;
        Ok(Value::Number(match function {
            BuiltinFunction::MathTan => value.tan(),
            BuiltinFunction::MathAsin => value.asin(),
            BuiltinFunction::MathAcos => value.acos(),
            BuiltinFunction::MathAtan => value.atan(),
            BuiltinFunction::MathSinh => value.sinh(),
            BuiltinFunction::MathCosh => value.cosh(),
            BuiltinFunction::MathTanh => value.tanh(),
            // Some platform libm implementations overflow their intermediate
            // 2*x for finite inputs near MAX_VALUE. Above 2^28 the correction
            // to ln(2*x) is below one ulp; this form cannot overflow.
            BuiltinFunction::MathAsinh if value.abs() > 268_435_456.0 => {
                (value.abs().ln() + std::f64::consts::LN_2).copysign(value)
            }
            BuiltinFunction::MathAcosh if value > 268_435_456.0 => {
                value.ln() + std::f64::consts::LN_2
            }
            BuiltinFunction::MathAcosh if (1.0..=2.0).contains(&value) => {
                let delta = value - 1.0;
                (delta + (delta * delta + 2.0 * delta).sqrt()).ln_1p()
            }
            BuiltinFunction::MathAsinh => value.asinh(),
            BuiltinFunction::MathAcosh => value.acosh(),
            BuiltinFunction::MathAtanh => value.atanh(),
            BuiltinFunction::MathFround => (value as f32) as f64,
            BuiltinFunction::MathLog1p => value.ln_1p(),
            BuiltinFunction::MathExpm1 => value.exp_m1(),
            _ => return Err(MustardError::runtime("invalid internal Math helper")),
        }))
    }

    pub(crate) fn call_math_sin(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .sin(),
        ))
    }

    pub(crate) fn call_math_cos(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .cos(),
        ))
    }

    pub(crate) fn call_math_atan2(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .atan2(self.to_number(args.get(1).cloned().unwrap_or(Value::Undefined))?),
        ))
    }

    pub(crate) fn call_math_hypot(&self, args: &[Value]) -> MustardResult<Value> {
        let mut value: f64 = 0.0;
        for arg in args {
            value = value.hypot(self.to_number(arg.clone())?);
        }
        Ok(Value::Number(value))
    }

    pub(crate) fn call_math_cbrt(&self, args: &[Value]) -> MustardResult<Value> {
        Ok(Value::Number(
            self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                .cbrt(),
        ))
    }

    pub(crate) fn call_math_random(&self) -> Value {
        Value::Number(math_random_f64())
    }
}

impl Runtime {
    fn error_options_cause(&self, options: Value) -> MustardResult<Option<Option<Value>>> {
        match options {
            Value::Undefined | Value::Null => Ok(None),
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| MustardError::runtime("object missing"))?;
                let cause = object.properties.get("cause").cloned();
                Ok(Some(cause))
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?;
                Ok(Some(array.properties.get("cause").cloned()))
            }
            _ => Err(MustardError::runtime(
                "TypeError: Error options must be an object in the supported surface",
            )),
        }
    }
}

fn number_to_radix_string(value: f64, radix: u32) -> String {
    if radix == 10 || !value.is_finite() {
        return value.to_js_string();
    }

    let negative = value.is_sign_negative() && value != 0.0;
    let mut integer = value.abs().trunc();
    let mut integer_digits = Vec::new();
    if integer == 0.0 {
        integer_digits.push('0');
    } else {
        while integer >= 1.0 && integer_digits.len() < 4096 {
            let digit = (integer % radix as f64).floor() as u32;
            integer_digits.push(radix_digit(digit));
            let next = (integer / radix as f64).floor();
            if next == integer {
                break;
            }
            integer = next;
        }
        integer_digits.reverse();
    }

    let mut rendered = integer_digits.into_iter().collect::<String>();
    let mut fraction = value.abs().fract();
    if fraction != 0.0 {
        rendered.push('.');
        let mut digits = 0;
        while fraction != 0.0 && digits < 64 {
            fraction *= radix as f64;
            let digit = fraction.floor();
            rendered.push(radix_digit(digit as u32));
            fraction -= digit;
            digits += 1;
        }
        while rendered.ends_with('0') {
            rendered.pop();
        }
        if rendered.ends_with('.') {
            rendered.pop();
        }
    }

    if negative {
        format!("-{rendered}")
    } else {
        rendered
    }
}

fn number_to_exponential_string(value: f64, fraction_digits: Option<usize>) -> String {
    if !value.is_finite() {
        return value.to_js_string();
    }
    let negative = value.is_sign_negative() && value != 0.0;
    let magnitude = value.abs();
    let rendered = match fraction_digits {
        Some(digits) => format!("{magnitude:.*e}", digits),
        None => format!("{magnitude:e}"),
    };
    let rendered = normalize_exponent(&rendered);
    if negative {
        format!("-{rendered}")
    } else {
        rendered
    }
}

fn number_to_precision_string(value: f64, precision: usize) -> String {
    if !value.is_finite() {
        return value.to_js_string();
    }

    let negative = value.is_sign_negative() && value != 0.0;
    let magnitude = value.abs();
    let exponent = if magnitude == 0.0 {
        0
    } else {
        magnitude.log10().floor() as i32
    };
    let mut rendered = if exponent >= -6 && exponent < precision as i32 {
        let fraction_digits = precision as i32 - exponent - 1;
        format!("{magnitude:.*}", fraction_digits.max(0) as usize)
    } else {
        normalize_exponent(&format!("{magnitude:.*e}", precision - 1))
    };
    if negative {
        rendered.insert(0, '-');
    }
    rendered
}

fn normalize_exponent(value: &str) -> String {
    let Some((mantissa, exponent)) = value.split_once('e') else {
        return value.to_string();
    };
    let exponent = exponent.parse::<i32>().unwrap_or(0);
    if exponent >= 0 {
        format!("{mantissa}e+{exponent}")
    } else {
        format!("{mantissa}e{exponent}")
    }
}

fn radix_digit(value: u32) -> char {
    match value {
        0..=9 => (b'0' + value as u8) as char,
        10..=35 => (b'a' + (value as u8 - 10)) as char,
        _ => '?',
    }
}
