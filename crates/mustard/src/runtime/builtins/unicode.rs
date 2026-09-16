use super::*;
use unicode_normalization::UnicodeNormalization;

impl Runtime {
    fn unicode_string_receiver(&self, value: Value) -> MustardResult<String> {
        if matches!(value, Value::Null | Value::Undefined) {
            return Err(MustardError::runtime(
                "TypeError: String method requires a non-nullish receiver",
            ));
        }
        self.to_string(value)
    }

    pub(crate) fn call_string_is_well_formed(&self, this_value: Value) -> MustardResult<Value> {
        self.unicode_string_receiver(this_value)?;
        // Guest strings are valid Unicode: constructors and boundaries reject lone surrogates.
        Ok(Value::Bool(true))
    }

    pub(crate) fn call_string_code_at(
        &mut self,
        this_value: Value,
        args: &[Value],
        point: bool,
    ) -> MustardResult<Value> {
        let text = self.unicode_string_receiver(this_value)?;
        let index = self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?;
        let missing = if point {
            Value::Undefined
        } else {
            Value::Number(f64::NAN)
        };
        if index < 0 {
            return Ok(missing);
        }
        // These two ECMAScript APIs explicitly expose UTF-16; legacy indexing remains scalar-based.
        let index = index as usize;
        self.charge_native_helper_work(index.saturating_add(2).min(text.len()))?;
        let mut units = text.encode_utf16();
        let Some(first) = units.nth(index) else {
            return Ok(missing);
        };
        let value = if point && (0xD800..=0xDBFF).contains(&first) {
            let second = units
                .next()
                .expect("well-formed high surrogate has a low surrogate");
            0x10000 + ((first as u32 - 0xD800) << 10) + (second as u32 - 0xDC00)
        } else {
            first as u32
        };
        Ok(Value::Number(value as f64))
    }

    pub(crate) fn call_string_from_codes(
        &mut self,
        args: &[Value],
        points: bool,
    ) -> MustardResult<Value> {
        self.with_temporary_roots(args, |runtime| {
            runtime.charge_native_helper_work(args.len())?;
            runtime.ensure_heap_capacity(args.len().saturating_mul(6))?;
            let mut result = String::new();
            if points {
                for value in args {
                    let value = runtime.to_number(value.clone())?;
                    if !value.is_finite() || value.fract() != 0.0 || !(0.0..=0x10FFFF as f64).contains(&value) {
                        return Err(MustardError::runtime("RangeError: invalid Unicode code point"));
                    }
                    let ch = char::from_u32(value as u32).ok_or_else(|| MustardError::runtime("RangeError: lone surrogates are not supported by the Unicode string profile"))?;
                    result.push(ch);
                }
            } else {
                let units = args.iter().map(|value| {
                    let number = runtime.to_number(value.clone())?;
                    Ok(if number.is_finite() { number.trunc().rem_euclid(65536.0) as u16 } else { 0 })
                }).collect::<MustardResult<Vec<_>>>()?;
                result = String::from_utf16(&units).map_err(|_| MustardError::runtime("RangeError: lone surrogates are not supported by the Unicode string profile"))?;
            }
            Ok(Value::String(result))
        })
    }

    pub(crate) fn call_string_normalize(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let mut roots = args.to_vec();
        roots.push(this_value.clone());
        self.with_temporary_roots(&roots, |runtime| {
            let text = runtime.unicode_string_receiver(this_value.clone())?;
            let form = match args.first() {
                None | Some(Value::Undefined) => "NFC".to_string(),
                Some(value) => runtime.to_string(value.clone())?,
            };
            if !matches!(form.as_str(), "NFC" | "NFD" | "NFKC" | "NFKD") {
                return Err(MustardError::runtime(
                    "RangeError: normalization form must be NFC, NFD, NFKC, or NFKD",
                ));
            }
            // Preflight the actual decomposition size. The maintained normalizer may buffer
            // an arbitrarily long combining sequence; do not hide that allocation from limits.
            let mut decomposed = 0usize;
            for ch in text.chars() {
                runtime.charge_native_helper_work(1)?;
                let mut count = |_| {
                    decomposed = decomposed.saturating_add(1);
                };
                if form.starts_with("NFK") {
                    unicode_normalization::char::decompose_compatible(ch, &mut count);
                } else {
                    unicode_normalization::char::decompose_canonical(ch, &mut count);
                }
            }
            runtime.ensure_heap_capacity(decomposed.saturating_mul(24))?;
            runtime.charge_native_helper_work(
                decomposed.saturating_mul((decomposed.max(1).ilog2() + 1) as usize),
            )?;
            let iterator: Box<dyn Iterator<Item = char>> = match form.as_str() {
                "NFC" => Box::new(text.nfc()),
                "NFD" => Box::new(text.nfd()),
                "NFKC" => Box::new(text.nfkc()),
                "NFKD" => Box::new(text.nfkd()),
                _ => unreachable!(),
            };
            Ok(Value::String(iterator.collect()))
        })
    }
}
