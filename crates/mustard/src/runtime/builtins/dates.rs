use super::*;

impl Runtime {
    pub(crate) fn construct_date(&mut self, args: &[Value]) -> MustardResult<Value> {
        let timestamp_ms = if args.is_empty() {
            time_clip(current_time_millis())
        } else if args.len() == 1 {
            self.date_timestamp_ms_from_value(args[0].clone())?
        } else {
            self.date_components_from_args(args)?
        };
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::Date(DateObject { timestamp_ms }),
        )?))
    }

    fn date_components_from_args(&mut self, args: &[Value]) -> MustardResult<f64> {
        let mut parts = [f64::NAN, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        for (index, value) in args.iter().take(7).enumerate() {
            parts[index] = self.coerce_number(value.clone())?;
        }
        Ok(make_utc_timestamp(parts, true))
    }

    pub(in crate::runtime) fn date_receiver(
        &self,
        value: Value,
        method: &str,
    ) -> MustardResult<ObjectKey> {
        match value {
            Value::Object(key) if self.is_date_object(key) => Ok(key),
            _ => Err(MustardError::runtime(format!(
                "TypeError: Date.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn call_date_get_time(&self, this_value: Value) -> MustardResult<Value> {
        let date = self.date_receiver(this_value, "getTime")?;
        Ok(Value::Number(self.date_object(date)?.timestamp_ms))
    }

    pub(crate) fn call_date_value_of(&self, this_value: Value) -> MustardResult<Value> {
        let date = self.date_receiver(this_value, "valueOf")?;
        Ok(Value::Number(self.date_object(date)?.timestamp_ms))
    }

    pub(crate) fn call_date_to_iso_string(&self, this_value: Value) -> MustardResult<Value> {
        let date = self.date_receiver(this_value, "toISOString")?;
        let timestamp_ms = self.date_object(date)?.timestamp_ms;
        let Some(rendered) = format_iso_datetime(timestamp_ms) else {
            return Err(MustardError::runtime("RangeError: Invalid time value"));
        };
        Ok(Value::String(rendered))
    }

    pub(crate) fn call_date_to_json(&self, this_value: Value) -> MustardResult<Value> {
        let date = self.date_receiver(this_value, "toJSON")?;
        let timestamp_ms = self.date_object(date)?.timestamp_ms;
        Ok(match format_iso_datetime(timestamp_ms) {
            Some(rendered) => Value::String(rendered),
            None => Value::Null,
        })
    }

    fn date_utc_fields(
        &self,
        this_value: Value,
        method: &str,
    ) -> MustardResult<Option<DateTimeFields>> {
        let date = self.date_receiver(this_value, method)?;
        let timestamp_ms = self.date_object(date)?.timestamp_ms;
        Ok(date_time_fields_from_timestamp_ms(timestamp_ms))
    }

    pub(crate) fn call_date_get_utc_full_year(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Number(
            self.date_utc_fields(this_value, "getUTCFullYear")?
                .map_or(f64::NAN, |fields| fields.year as f64),
        ))
    }

    pub(crate) fn call_date_get_utc_month(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Number(
            self.date_utc_fields(this_value, "getUTCMonth")?
                .map_or(f64::NAN, |fields| f64::from(fields.month - 1)),
        ))
    }

    pub(crate) fn call_date_get_utc_date(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Number(
            self.date_utc_fields(this_value, "getUTCDate")?
                .map_or(f64::NAN, |fields| f64::from(fields.day)),
        ))
    }

    pub(crate) fn call_date_get_utc_hours(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Number(
            self.date_utc_fields(this_value, "getUTCHours")?
                .map_or(f64::NAN, |fields| f64::from(fields.hour)),
        ))
    }

    pub(crate) fn call_date_get_utc_minutes(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Number(
            self.date_utc_fields(this_value, "getUTCMinutes")?
                .map_or(f64::NAN, |fields| f64::from(fields.minute)),
        ))
    }

    pub(crate) fn call_date_get_utc_seconds(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::Number(
            self.date_utc_fields(this_value, "getUTCSeconds")?
                .map_or(f64::NAN, |fields| f64::from(fields.second)),
        ))
    }

    pub(crate) fn date_timestamp_ms_from_value(&mut self, value: Value) -> MustardResult<f64> {
        let value = match value {
            Value::Object(object) if self.is_date_object(object) => {
                return Ok(self.date_object(object)?.timestamp_ms);
            }
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::NumberObject(value) => Value::Number(*value),
                ObjectKind::StringObject(value) => Value::String(value.clone()),
                ObjectKind::BooleanObject(value) => Value::Bool(*value),
                _ => {
                    return Err(MustardError::runtime(
                        "TypeError: Date does not support custom object coercion",
                    ));
                }
            },
            value => value,
        };
        Ok(time_clip(match value {
            Value::String(value) => {
                self.charge_native_helper_work(value.len())?;
                parse_date_timestamp_ms(&value)
            }
            value => self.coerce_number(value)?,
        }))
    }

    pub(in crate::runtime) fn date_default_string(timestamp: f64) -> String {
        format_date_string(timestamp, DateStringKind::Full)
    }

    pub(crate) fn call_date_completion(
        &mut self,
        method: BuiltinFunction,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        use BuiltinFunction::*;
        if method == DateUTC {
            return Ok(Value::Number(self.date_components_from_args(args)?));
        }
        if method == DateParse {
            let text = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
            self.charge_native_helper_work(text.len())?;
            return Ok(Value::Number(time_clip(parse_date_timestamp_ms(&text))));
        }
        let date = self.date_receiver(this_value.clone(), Self::builtin_function_name(method))?;
        let timestamp = self.date_object(date)?.timestamp_ms;
        if matches!(
            method,
            DateToLocaleString | DateToLocaleDateString | DateToLocaleTimeString
        ) {
            return self.call_date_locale(method, timestamp, args);
        }
        let string_kind = match method {
            DateToString => Some(DateStringKind::Full),
            DateToDateString => Some(DateStringKind::Date),
            DateToTimeString => Some(DateStringKind::Time),
            DateToUTCString => Some(DateStringKind::Utc),
            _ => None,
        };
        if let Some(kind) = string_kind {
            return Ok(Value::String(format_date_string(timestamp, kind)));
        }
        let fields = date_time_fields_from_timestamp_ms(timestamp);
        let field_value = match method {
            DateGetFullYear => Some(fields.map_or(f64::NAN, |f| f.year as f64)),
            DateGetMonth => Some(fields.map_or(f64::NAN, |f| (f.month - 1) as f64)),
            DateGetDate => Some(fields.map_or(f64::NAN, |f| f.day as f64)),
            DateGetDay | DateGetUTCDay => {
                Some(fields.map_or(f64::NAN, |_| weekday(timestamp) as f64))
            }
            DateGetHours => Some(fields.map_or(f64::NAN, |f| f.hour as f64)),
            DateGetMinutes => Some(fields.map_or(f64::NAN, |f| f.minute as f64)),
            DateGetSeconds => Some(fields.map_or(f64::NAN, |f| f.second as f64)),
            DateGetMilliseconds | DateGetUTCMilliseconds => {
                Some(fields.map_or(f64::NAN, |f| f.millisecond as f64))
            }
            DateGetTimezoneOffset => Some(fields.map_or(f64::NAN, |_| 0.0)),
            DateGetYear => Some(fields.map_or(f64::NAN, |f| (f.year - 1900) as f64)),
            _ => None,
        };
        if let Some(value) = field_value {
            return Ok(Value::Number(value));
        }
        let (start, count) = match method {
            DateSetFullYear | DateSetUTCFullYear => (0, 3),
            DateSetMonth | DateSetUTCMonth => (1, 2),
            DateSetDate | DateSetUTCDate => (2, 1),
            DateSetHours | DateSetUTCHours => (3, 4),
            DateSetMinutes | DateSetUTCMinutes => (4, 3),
            DateSetSeconds | DateSetUTCSeconds => (5, 2),
            DateSetMilliseconds | DateSetUTCMilliseconds => (6, 1),
            DateSetTime | DateSetYear => (0, 1),
            _ => return Err(MustardError::runtime("invalid internal Date method")),
        };
        // Convert supplied arguments even when the current date is invalid.
        let mut values = Vec::new();
        for index in 0..args.len().min(count).max(1) {
            values.push(self.coerce_number(args.get(index).cloned().unwrap_or(Value::Undefined))?);
        }
        let timestamp_ms = if method == DateSetTime {
            time_clip(values[0])
        } else {
            let repair = matches!(method, DateSetFullYear | DateSetUTCFullYear | DateSetYear);
            let fields = fields.or_else(|| {
                if repair {
                    date_time_fields_from_timestamp_ms(0.0)
                } else {
                    None
                }
            });
            if let Some(f) = fields {
                let mut parts = [
                    f.year as f64,
                    (f.month - 1) as f64,
                    f.day as f64,
                    f.hour as f64,
                    f.minute as f64,
                    f.second as f64,
                    f.millisecond as f64,
                ];
                for (i, value) in values.into_iter().enumerate() {
                    parts[start + i] = value;
                }
                make_utc_timestamp(parts, method == DateSetYear)
            } else {
                f64::NAN
            }
        };
        let ObjectKind::Date(object) = &mut self
            .objects
            .get_mut(date)
            .ok_or_else(|| MustardError::runtime("date missing"))?
            .kind
        else {
            unreachable!()
        };
        object.timestamp_ms = timestamp_ms;
        Ok(Value::Number(timestamp_ms))
    }
}

#[derive(Clone, Copy)]
enum DateStringKind {
    Full,
    Date,
    Time,
    Utc,
}
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn weekday(timestamp: f64) -> usize {
    ((timestamp as i64).div_euclid(86_400_000) + 4).rem_euclid(7) as usize
}

fn format_date_string(timestamp: f64, kind: DateStringKind) -> String {
    let Some(f) = date_time_fields_from_timestamp_ms(timestamp) else {
        return "Invalid Date".to_string();
    };
    let year = if f.year < 0 {
        format!("-{:04}", f.year.unsigned_abs())
    } else {
        format!("{:04}", f.year)
    };
    let time = format!("{:02}:{:02}:{:02}", f.hour, f.minute, f.second);
    let day = WEEKDAYS[weekday(timestamp)];
    let month = MONTHS[(f.month - 1) as usize];
    match kind {
        DateStringKind::Utc => format!("{day}, {:02} {month} {year} {time} GMT", f.day),
        DateStringKind::Time => format!("{time} GMT+0000 (Coordinated Universal Time)"),
        DateStringKind::Date => format!("{day} {month} {:02} {year}", f.day),
        DateStringKind::Full => format!(
            "{day} {month} {:02} {year} {time} GMT+0000 (Coordinated Universal Time)",
            f.day
        ),
    }
}

// All normalized calendar fields are integral. Checked wide arithmetic prevents
// overflow or saturation before the final ECMAScript TimeClip.
fn make_utc_timestamp(parts: [f64; 7], adjust_year: bool) -> f64 {
    fn calculate(parts: [f64; 7], adjust_year: bool) -> Option<i128> {
        let mut values = [0i128; 7];
        for (i, value) in parts.into_iter().enumerate() {
            if !value.is_finite() || value < i128::MIN as f64 || value >= i128::MAX as f64 {
                return None;
            }
            values[i] = value.trunc() as i128;
        }
        let [mut year, month, date, hour, minute, second, millisecond] = values;
        if adjust_year && (0..=99).contains(&year) {
            year += 1900;
        }
        year = year.checked_add(month.div_euclid(12))?;
        let month = month.rem_euclid(12) + 1;
        let year = year.checked_sub(i128::from(month <= 2))?;
        let era = year.div_euclid(400);
        let yoe = year.rem_euclid(400);
        let shifted_month = month + if month > 2 { -3 } else { 9 };
        let doy = (153 * shifted_month + 2) / 5;
        let days = era
            .checked_mul(146_097)?
            .checked_add(yoe * 365 + yoe / 4 - yoe / 100 + doy)?
            .checked_sub(719_468)?
            .checked_add(date.checked_sub(1)?)?;
        days.checked_mul(86_400_000)?
            .checked_add(hour.checked_mul(3_600_000)?)?
            .checked_add(minute.checked_mul(60_000)?)?
            .checked_add(second.checked_mul(1_000)?)?
            .checked_add(millisecond)
    }
    calculate(parts, adjust_year).map_or(f64::NAN, |value| time_clip(value as f64))
}
