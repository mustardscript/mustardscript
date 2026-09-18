use super::currencies::{currency_digits, currency_symbol};
use super::*;
use oxc_syntax::number::ToJsString;

impl Runtime {
    fn normalize_intl_locale(&mut self, value: Option<Value>) -> MustardResult<String> {
        let locales = match value.unwrap_or(Value::Undefined) {
            Value::Undefined => return Ok("en-US".to_string()),
            Value::String(locale) => vec![Some(Value::String(locale))],
            Value::Array(key) => {
                let length = self
                    .arrays
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .len();
                self.charge_native_helper_work(length)?;
                self.ensure_heap_capacity(
                    length.saturating_mul(std::mem::size_of::<Option<Value>>()),
                )?;
                self.arrays.get(key).unwrap().elements.clone()
            }
            _ => {
                return Err(MustardError::runtime(
                    "TypeError: Intl currently supports only the `en-US` locale",
                ));
            }
        };
        // Validate every present entry without recursively following guest arrays/objects.
        for locale in locales.into_iter().flatten() {
            let Value::String(locale) = locale else {
                return Err(MustardError::runtime(
                    "TypeError: Intl locale list entries must be strings",
                ));
            };
            self.charge_native_helper_work(locale.len())?;
            if !locale.eq_ignore_ascii_case("en-US") {
                return Err(MustardError::runtime(
                    "TypeError: Intl currently supports only the `en-US` locale",
                ));
            }
        }
        Ok("en-US".to_string())
    }

    fn intl_options_object(&self, value: Option<Value>) -> MustardResult<Option<ObjectKey>> {
        match value.unwrap_or(Value::Undefined) {
            Value::Undefined => Ok(None),
            Value::Object(object) => Ok(Some(object)),
            _ => Err(MustardError::runtime(
                "TypeError: Intl options must be a plain object in the supported surface",
            )),
        }
    }

    fn intl_option_value(&self, object: Option<ObjectKey>, key: &str) -> MustardResult<Value> {
        let Some(object) = object else {
            return Ok(Value::Undefined);
        };
        Ok(self
            .objects
            .get(object)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .properties
            .get(key)
            .cloned()
            .unwrap_or(Value::Undefined))
    }

    fn intl_option_field_style(
        &self,
        object: Option<ObjectKey>,
        key: &str,
    ) -> MustardResult<Option<IntlFieldStyle>> {
        Ok(match self.intl_option_value(object, key)? {
            Value::Undefined => None,
            Value::String(value) if value == "numeric" => Some(IntlFieldStyle::Numeric),
            Value::String(value) if value == "2-digit" => Some(IntlFieldStyle::TwoDigit),
            _ => {
                return Err(MustardError::runtime(format!(
                    "TypeError: Intl.{key} only supports `numeric` or `2-digit`",
                )));
            }
        })
    }

    fn intl_option_string(
        &self,
        object: Option<ObjectKey>,
        key: &str,
    ) -> MustardResult<Option<String>> {
        Ok(match self.intl_option_value(object, key)? {
            Value::Undefined => None,
            value => Some(self.to_string(value)?),
        })
    }

    fn intl_option_bool(
        &self,
        object: Option<ObjectKey>,
        key: &str,
    ) -> MustardResult<Option<bool>> {
        Ok(match self.intl_option_value(object, key)? {
            Value::Undefined => None,
            Value::Bool(value) => Some(value),
            _ => {
                return Err(MustardError::runtime(format!(
                    "TypeError: Intl `{key}` must be a boolean in the supported surface",
                )));
            }
        })
    }

    fn intl_option_digits(
        &mut self,
        object: Option<ObjectKey>,
        key: &str,
    ) -> MustardResult<Option<usize>> {
        match self.intl_option_value(object, key)? {
            Value::Undefined => Ok(None),
            value => {
                let digits = self.coerce_number(value)?;
                if !digits.is_finite() || !(0.0..=100.0).contains(&digits) {
                    return Err(MustardError::runtime(format!(
                        "RangeError: Intl `{key}` must be between 0 and 100",
                    )));
                }
                Ok(Some(digits as usize))
            }
        }
    }

    fn intl_assert_supported_option_keys(
        &self,
        object: Option<ObjectKey>,
        ctor: &str,
        allowed: &[&str],
    ) -> MustardResult<()> {
        let Some(object) = object else {
            return Ok(());
        };
        for key in self
            .objects
            .get(object)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .properties
            .keys()
        {
            if !allowed.contains(&key.as_str()) {
                return Err(MustardError::runtime(format!(
                    "TypeError: Intl.{ctor} does not support the `{key}` option",
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn construct_intl_date_time_format(
        &mut self,
        args: &[Value],
    ) -> MustardResult<Value> {
        let locale = self.normalize_intl_locale(args.first().cloned())?;
        let options = self.intl_options_object(args.get(1).cloned())?;
        self.intl_assert_supported_option_keys(
            options,
            "DateTimeFormat",
            &[
                "timeZone", "year", "month", "day", "hour", "minute", "second",
            ],
        )?;
        let time_zone = match self.intl_option_string(options, "timeZone")? {
            None => "UTC".to_string(),
            Some(value) if value == "UTC" => value,
            Some(_) => {
                return Err(MustardError::runtime(
                    "TypeError: Intl.DateTimeFormat currently supports only the `UTC` timeZone",
                ));
            }
        };
        let mut year = self.intl_option_field_style(options, "year")?;
        let mut month = self.intl_option_field_style(options, "month")?;
        let mut day = self.intl_option_field_style(options, "day")?;
        let hour = self.intl_option_field_style(options, "hour")?;
        let minute = self.intl_option_field_style(options, "minute")?;
        let second = self.intl_option_field_style(options, "second")?;
        if year.is_none()
            && month.is_none()
            && day.is_none()
            && hour.is_none()
            && minute.is_none()
            && second.is_none()
        {
            year = Some(IntlFieldStyle::Numeric);
            month = Some(IntlFieldStyle::Numeric);
            day = Some(IntlFieldStyle::Numeric);
        }
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::IntlDateTimeFormat(IntlDateTimeFormatObject {
                locale,
                time_zone,
                year,
                month,
                day,
                hour,
                minute,
                second,
            }),
        )?))
    }

    pub(crate) fn construct_intl_number_format(&mut self, args: &[Value]) -> MustardResult<Value> {
        let locale = self.normalize_intl_locale(args.first().cloned())?;
        let options = self.intl_options_object(args.get(1).cloned())?;
        self.intl_assert_supported_option_keys(
            options,
            "NumberFormat",
            &[
                "style",
                "currency",
                "minimumFractionDigits",
                "maximumFractionDigits",
                "useGrouping",
            ],
        )?;
        let style = match self.intl_option_string(options, "style")? {
            None => IntlNumberStyle::Decimal,
            Some(value) if value == "decimal" => IntlNumberStyle::Decimal,
            Some(value) if value == "percent" => IntlNumberStyle::Percent,
            Some(value) if value == "currency" => IntlNumberStyle::Currency,
            Some(_) => {
                return Err(MustardError::runtime(
                    "TypeError: Intl.NumberFormat currently supports `decimal`, `percent`, or `currency` styles",
                ));
            }
        };
        let currency = self.intl_option_string(options, "currency")?;
        if let Some(currency) = &currency {
            self.charge_native_helper_work(currency.len())?;
            if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_alphabetic()) {
                return Err(MustardError::runtime(
                    "RangeError: Intl.NumberFormat currency must be a three-letter code",
                ));
            }
        }
        if style == IntlNumberStyle::Currency && currency.is_none() {
            return Err(MustardError::runtime(
                "TypeError: Intl.NumberFormat currency style requires currency",
            ));
        }
        let currency = if style == IntlNumberStyle::Currency {
            currency.map(|c| c.to_ascii_uppercase())
        } else {
            None
        };
        let default_min = currency.as_deref().map(currency_digits).unwrap_or(0);
        let default_max = match style {
            IntlNumberStyle::Currency => default_min,
            IntlNumberStyle::Percent => 0,
            IntlNumberStyle::Decimal => 3,
        };
        let minimum = self.intl_option_digits(options, "minimumFractionDigits")?;
        let maximum = self.intl_option_digits(options, "maximumFractionDigits")?;
        let minimum_fraction_digits = minimum.unwrap_or(default_min.min(maximum.unwrap_or(100)));
        let maximum_fraction_digits = maximum.unwrap_or(default_max.max(minimum_fraction_digits));
        if minimum_fraction_digits > maximum_fraction_digits {
            return Err(MustardError::runtime(
                "RangeError: Intl.NumberFormat minimumFractionDigits cannot exceed maximumFractionDigits",
            ));
        }
        let use_grouping = self
            .intl_option_bool(options, "useGrouping")?
            .unwrap_or(true);
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::IntlNumberFormat(IntlNumberFormatObject {
                locale,
                style,
                currency,
                minimum_fraction_digits,
                maximum_fraction_digits,
                use_grouping,
            }),
        )?))
    }

    fn intl_date_time_format_receiver(
        &self,
        value: Value,
        method: &str,
    ) -> MustardResult<&IntlDateTimeFormatObject> {
        match value {
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::IntlDateTimeFormat(formatter) => Ok(formatter),
                _ => Err(MustardError::runtime(format!(
                    "TypeError: Intl.DateTimeFormat.prototype.{method} called on incompatible receiver",
                ))),
            },
            _ => Err(MustardError::runtime(format!(
                "TypeError: Intl.DateTimeFormat.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn intl_number_format_receiver(
        &self,
        value: Value,
        method: &str,
    ) -> MustardResult<&IntlNumberFormatObject> {
        match value {
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::IntlNumberFormat(formatter) => Ok(formatter),
                _ => Err(MustardError::runtime(format!(
                    "TypeError: Intl.NumberFormat.prototype.{method} called on incompatible receiver",
                ))),
            },
            _ => Err(MustardError::runtime(format!(
                "TypeError: Intl.NumberFormat.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn format_intl_field(value: u8, style: IntlFieldStyle) -> String {
        match style {
            IntlFieldStyle::Numeric => value.to_string(),
            IntlFieldStyle::TwoDigit => format!("{value:02}"),
        }
    }

    pub(crate) fn call_intl_date_time_format_format(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let formatter = self
            .intl_date_time_format_receiver(this_value, "format")?
            .clone();
        let timestamp_ms = match args.first().cloned().unwrap_or(Value::Undefined) {
            Value::Undefined => current_time_millis(),
            value => self.date_timestamp_ms_from_value(value)?,
        };
        let Some(datetime) = date_time_fields_from_timestamp_ms(timestamp_ms) else {
            return Err(MustardError::runtime("RangeError: Invalid time value"));
        };
        let mut date_parts = Vec::new();
        if let Some(month) = formatter.month {
            date_parts.push(Self::format_intl_field(datetime.month, month));
        }
        if let Some(day) = formatter.day {
            date_parts.push(Self::format_intl_field(datetime.day, day));
        }
        if let Some(year) = formatter.year {
            date_parts.push(match year {
                IntlFieldStyle::Numeric => if datetime.year <= 0 {
                    1 - datetime.year
                } else {
                    datetime.year
                }
                .to_string(),
                IntlFieldStyle::TwoDigit => format!(
                    "{:02}",
                    (if datetime.year <= 0 {
                        1 - datetime.year
                    } else {
                        datetime.year
                    })
                    .rem_euclid(100)
                ),
            });
        }
        let mut rendered = if date_parts.is_empty() {
            String::new()
        } else {
            date_parts.join("/")
        };
        let mut rendered_time = None;
        if let Some(hour_style) = formatter.hour {
            let hour_24 = datetime.hour;
            let meridiem = if hour_24 < 12 { "AM" } else { "PM" };
            let hour_12 = match hour_24 % 12 {
                0 => 12,
                value => value,
            };
            let mut time_parts = vec![match hour_style {
                IntlFieldStyle::Numeric => hour_12.to_string(),
                IntlFieldStyle::TwoDigit => format!("{hour_12:02}"),
            }];
            if formatter.minute.is_some() {
                time_parts.push(format!("{:02}", datetime.minute));
            }
            if formatter.second.is_some() {
                time_parts.push(format!("{:02}", datetime.second));
            }
            rendered_time = Some(format!("{} {meridiem}", time_parts.join(":")));
        } else {
            let mut time_parts = Vec::new();
            if let Some(minute) = formatter.minute {
                time_parts.push(Self::format_intl_field(datetime.minute, minute));
            }
            if let Some(second) = formatter.second {
                time_parts.push(Self::format_intl_field(datetime.second, second));
            }
            if !time_parts.is_empty() {
                rendered_time = Some(time_parts.join(":"));
            }
        }
        if let Some(time) = rendered_time {
            if !rendered.is_empty() {
                rendered.push_str(", ");
            }
            rendered.push_str(&time);
        }
        Ok(Value::String(rendered))
    }

    pub(crate) fn call_intl_date_time_format_resolved_options(
        &mut self,
        this_value: Value,
    ) -> MustardResult<Value> {
        let formatter = self
            .intl_date_time_format_receiver(this_value, "resolvedOptions")?
            .clone();
        let mut properties = IndexMap::new();
        properties.insert("locale".to_string(), Value::String(formatter.locale));
        properties.insert("timeZone".to_string(), Value::String(formatter.time_zone));
        if let Some(year) = formatter.year {
            properties.insert(
                "year".to_string(),
                Value::String(match year {
                    IntlFieldStyle::Numeric => "numeric".to_string(),
                    IntlFieldStyle::TwoDigit => "2-digit".to_string(),
                }),
            );
        }
        if let Some(month) = formatter.month {
            properties.insert(
                "month".to_string(),
                Value::String(match month {
                    IntlFieldStyle::Numeric => "numeric".to_string(),
                    IntlFieldStyle::TwoDigit => "2-digit".to_string(),
                }),
            );
        }
        if let Some(day) = formatter.day {
            properties.insert(
                "day".to_string(),
                Value::String(match day {
                    IntlFieldStyle::Numeric => "numeric".to_string(),
                    IntlFieldStyle::TwoDigit => "2-digit".to_string(),
                }),
            );
        }
        if let Some(hour) = formatter.hour {
            properties.insert(
                "hour".to_string(),
                Value::String(match hour {
                    IntlFieldStyle::Numeric => "numeric".to_string(),
                    IntlFieldStyle::TwoDigit => "2-digit".to_string(),
                }),
            );
        }
        if let Some(minute) = formatter.minute {
            properties.insert(
                "minute".to_string(),
                Value::String(match minute {
                    IntlFieldStyle::Numeric => "numeric".to_string(),
                    IntlFieldStyle::TwoDigit => "2-digit".to_string(),
                }),
            );
        }
        if let Some(second) = formatter.second {
            properties.insert(
                "second".to_string(),
                Value::String(match second {
                    IntlFieldStyle::Numeric => "numeric".to_string(),
                    IntlFieldStyle::TwoDigit => "2-digit".to_string(),
                }),
            );
        }
        Ok(Value::Object(
            self.insert_object(properties, ObjectKind::Plain)?,
        ))
    }

    fn format_intl_number(
        &mut self,
        formatter: &IntlNumberFormatObject,
        number: f64,
    ) -> MustardResult<String> {
        if formatter.maximum_fraction_digits > 100
            || formatter.minimum_fraction_digits > formatter.maximum_fraction_digits
        {
            return Err(MustardError::runtime(
                "RangeError: invalid Intl.NumberFormat fraction digits",
            ));
        }
        // A finite binary64 needs at most 309 integer digits (+2 for percent),
        // plus 100 fraction digits. Account for all temporary decimal buffers.
        self.charge_native_helper_work(512)?;
        self.ensure_heap_capacity(8192)?;
        let rendered = if number.is_nan() {
            "NaN".to_string()
        } else if number.is_infinite() {
            "∞".to_string()
        } else {
            format_decimal_half_expand(number.abs(), formatter)
        };
        let sign = if !number.is_nan() && number.is_sign_negative() {
            "-"
        } else {
            ""
        };
        Ok(match formatter.style {
            IntlNumberStyle::Decimal => format!("{sign}{rendered}"),
            IntlNumberStyle::Percent => format!("{sign}{rendered}%"),
            IntlNumberStyle::Currency => {
                let (symbol, spacing) =
                    currency_symbol(formatter.currency.as_deref().unwrap_or("USD"));
                let space = if spacing && number.is_finite() {
                    "\u{a0}"
                } else {
                    ""
                };
                format!("{sign}{symbol}{space}{rendered}")
            }
        })
    }

    pub(crate) fn call_intl_number_format_format(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let formatter = self
            .intl_number_format_receiver(this_value, "format")?
            .clone();
        let number = self.coerce_number(args.first().cloned().unwrap_or(Value::Undefined))?;
        Ok(Value::String(self.format_intl_number(&formatter, number)?))
    }

    pub(crate) fn call_number_to_locale_string(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let number = self.number_receiver(this_value, "toLocaleString")?;
        let formatter = self.construct_intl_number_format(args)?;
        self.with_temporary_roots(std::slice::from_ref(&formatter), |runtime| {
            runtime.call_intl_number_format_format(formatter.clone(), &[Value::Number(number)])
        })
    }

    pub(crate) fn call_intl_number_format_resolved_options(
        &mut self,
        this_value: Value,
    ) -> MustardResult<Value> {
        let formatter = self
            .intl_number_format_receiver(this_value, "resolvedOptions")?
            .clone();
        let mut properties = IndexMap::new();
        properties.insert("locale".to_string(), Value::String(formatter.locale));
        properties.insert(
            "style".to_string(),
            Value::String(match formatter.style {
                IntlNumberStyle::Decimal => "decimal".to_string(),
                IntlNumberStyle::Percent => "percent".to_string(),
                IntlNumberStyle::Currency => "currency".to_string(),
            }),
        );
        if let Some(currency) = formatter.currency {
            properties.insert("currency".to_string(), Value::String(currency));
        }
        properties.insert(
            "minimumFractionDigits".to_string(),
            Value::Number(formatter.minimum_fraction_digits as f64),
        );
        properties.insert(
            "maximumFractionDigits".to_string(),
            Value::Number(formatter.maximum_fraction_digits as f64),
        );
        properties.insert(
            "useGrouping".to_string(),
            Value::Bool(formatter.use_grouping),
        );
        Ok(Value::Object(
            self.insert_object(properties, ObjectKind::Plain)?,
        ))
    }
}

impl Runtime {
    pub(crate) fn call_date_locale(
        &mut self,
        method: BuiltinFunction,
        timestamp: f64,
        args: &[Value],
    ) -> MustardResult<Value> {
        if !timestamp.is_finite() {
            return Ok(Value::String("Invalid Date".to_string()));
        }
        if matches!(args.get(1), Some(Value::Null)) {
            return Err(MustardError::runtime(
                "TypeError: Date locale options must not be null",
            ));
        }
        let options = self.intl_options_object(args.get(1).cloned())?;
        let mut properties = if let Some(object) = options {
            self.objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("options missing"))?
                .properties
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<IndexMap<_, _>>()
        } else {
            IndexMap::new()
        };
        let has_date = ["year", "month", "day"].iter().any(|key| {
            properties
                .get(*key)
                .is_some_and(|v| !matches!(v, Value::Undefined))
        });
        let has_time = ["hour", "minute", "second"].iter().any(|key| {
            properties
                .get(*key)
                .is_some_and(|v| !matches!(v, Value::Undefined))
        });
        let need_defaults = match method {
            BuiltinFunction::DateToLocaleDateString => !has_date,
            BuiltinFunction::DateToLocaleTimeString => !has_time,
            _ => !has_date && !has_time,
        };
        if need_defaults {
            if method != BuiltinFunction::DateToLocaleTimeString {
                for key in ["year", "month", "day"] {
                    properties.insert(key.to_string(), Value::String("numeric".to_string()));
                }
            }
            if method != BuiltinFunction::DateToLocaleDateString {
                properties.insert("hour".to_string(), Value::String("numeric".to_string()));
                for key in ["minute", "second"] {
                    properties.insert(key.to_string(), Value::String("2-digit".to_string()));
                }
            }
        }
        let options = self.insert_object(properties, ObjectKind::Plain)?;
        self.with_temporary_roots(&[Value::Object(options)], |runtime| {
            let formatter = runtime.construct_intl_date_time_format(&[
                args.first().cloned().unwrap_or(Value::Undefined),
                Value::Object(options),
            ])?;
            runtime.with_temporary_roots(std::slice::from_ref(&formatter), |runtime| {
                runtime.call_intl_date_time_format_format(
                    formatter.clone(),
                    &[Value::Number(timestamp)],
                )
            })
        })
    }
}

impl Runtime {
    pub(crate) fn call_string_locale_compare(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        use icu_collator::options::{
            AlternateHandling, CaseLevel, CollatorOptions, MaxVariable, Strength,
        };
        use icu_collator::preferences::{CollationCaseFirst, CollationNumericOrdering};
        use icu_collator::{Collator, CollatorPreferences};

        let left = self.unicode_string_receiver(this_value)?;
        let right = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.normalize_intl_locale(args.get(1).cloned())?;
        let options = self.intl_options_object(args.get(2).cloned())?;
        self.intl_assert_supported_option_keys(
            options,
            "Collator",
            &[
                "usage",
                "localeMatcher",
                "collation",
                "numeric",
                "caseFirst",
                "sensitivity",
                "ignorePunctuation",
            ],
        )?;
        for (key, allowed) in [
            ("usage", &["sort"][..]),
            ("localeMatcher", &["lookup", "best fit"][..]),
            ("collation", &["default"][..]),
        ] {
            if let Some(value) = self.intl_option_string(options, key)?
                && !allowed.contains(&value.as_str())
            {
                return Err(MustardError::runtime(format!(
                    "TypeError: Intl.Collator does not support `{key}: {value}`"
                )));
            }
        }
        let mut prefs: CollatorPreferences = icu_locale_core::locale!("en-US").into();
        prefs.numeric_ordering = Some(
            if self.intl_option_bool(options, "numeric")?.unwrap_or(false) {
                CollationNumericOrdering::True
            } else {
                CollationNumericOrdering::False
            },
        );
        prefs.case_first = Some(
            match self
                .intl_option_string(options, "caseFirst")?
                .as_deref()
                .unwrap_or("false")
            {
                "false" => CollationCaseFirst::False,
                "upper" => CollationCaseFirst::Upper,
                "lower" => CollationCaseFirst::Lower,
                _ => {
                    return Err(MustardError::runtime(
                        "RangeError: invalid collation caseFirst",
                    ));
                }
            },
        );
        let mut config = CollatorOptions::default();
        let (strength, case_level) = match self
            .intl_option_string(options, "sensitivity")?
            .as_deref()
            .unwrap_or("variant")
        {
            "base" => (Strength::Primary, CaseLevel::Off),
            "accent" => (Strength::Secondary, CaseLevel::Off),
            "case" => (Strength::Primary, CaseLevel::On),
            "variant" => (Strength::Tertiary, CaseLevel::Off),
            _ => {
                return Err(MustardError::runtime(
                    "RangeError: invalid collation sensitivity",
                ));
            }
        };
        config.strength = Some(strength);
        config.case_level = Some(case_level);
        config.alternate_handling = Some(
            if self
                .intl_option_bool(options, "ignorePunctuation")?
                .unwrap_or(false)
            {
                AlternateHandling::Shifted
            } else {
                AlternateHandling::NonIgnorable
            },
        );
        config.max_variable = Some(MaxVariable::Punctuation);

        // ICU buffers collation elements/decompositions, and can reorder a combining
        // run while handling contractions. Preflight both space and worst-case work.
        let bytes = left.len().saturating_add(right.len());
        self.charge_native_helper_work(bytes.saturating_mul(8))?;
        self.ensure_heap_capacity(bytes.saturating_mul(128).saturating_add(1024))?;
        let mut combining_run = 0usize;
        for ch in left
            .chars()
            .chain(std::iter::once('\0'))
            .chain(right.chars())
        {
            if unicode_normalization::char::canonical_combining_class(ch) == 0 {
                combining_run = 0;
            } else {
                combining_run = combining_run.saturating_add(1);
                self.charge_native_helper_work(combining_run.saturating_mul(16))?;
            }
        }
        // The supported profile has one locale and four semantic option axes.
        // Revalidate mutable guest options above on every call, then cache only
        // the immutable ICU configuration. Work/space checks remain per-call.
        let cache_key = (
            prefs.numeric_ordering == Some(CollationNumericOrdering::True),
            match prefs.case_first {
                Some(CollationCaseFirst::Upper) => 1,
                Some(CollationCaseFirst::Lower) => 2,
                _ => 0,
            },
            match (strength, case_level) {
                (Strength::Primary, CaseLevel::Off) => 0,
                (Strength::Secondary, CaseLevel::Off) => 1,
                (Strength::Primary, CaseLevel::On) => 2,
                _ => 3,
            },
            config.alternate_handling == Some(AlternateHandling::Shifted),
        );
        let collator = if let Some(collator) = self.collator_cache.shift_remove(&cache_key) {
            collator
        } else {
            Arc::new(Collator::try_new(prefs, config).map_err(|_| {
                MustardError::runtime("TypeError: pinned en-US collation data unavailable")
            })?)
        };
        if self.collator_cache.len() >= 8 {
            self.collator_cache.shift_remove_index(0);
        }
        self.collator_cache.insert(cache_key, Arc::clone(&collator));
        Ok(Value::Number(match collator.compare(&left, &right) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        }))
    }
}

// ECMA-402 rounds the shortest decimal representation, not the binary fraction
// used by Number.toFixed. Shift percent in decimal to avoid overflow/double rounding.
fn format_decimal_half_expand(number: f64, formatter: &IntlNumberFormatObject) -> String {
    let decimal = number.to_js_string();
    let (mantissa, exponent) = decimal
        .split_once('e')
        .map_or((decimal.as_str(), 0), |(m, e)| {
            (m, e.parse::<i32>().expect("binary64 decimal exponent"))
        });
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32
        + exponent
        + if formatter.style == IntlNumberStyle::Percent {
            2
        } else {
            0
        };
    let digits: Vec<u8> = mantissa.bytes().filter(|b| *b != b'.').collect();
    let precision = formatter.maximum_fraction_digits;
    let keep = point + precision as i32;
    let mut rounded = if keep <= 0 {
        vec![b'0']
    } else {
        let mut result = digits[..digits.len().min(keep as usize)].to_vec();
        result.resize(keep as usize, b'0');
        result
    };
    if keep >= 0 && digits.get(keep as usize).is_some_and(|b| *b >= b'5') {
        let mut carry = true;
        for digit in rounded.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            rounded.insert(0, b'1');
        }
    }
    if rounded.len() <= precision {
        let mut padded = vec![b'0'; precision + 1 - rounded.len()];
        padded.extend(rounded);
        rounded = padded;
    }
    let split = rounded.len() - precision;
    let integer = std::str::from_utf8(&rounded[..split])
        .expect("decimal digits")
        .trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let mut output = if formatter.use_grouping {
        format_en_us_number_grouped(integer)
    } else {
        integer.to_string()
    };
    let mut fraction_end = rounded.len();
    while fraction_end > split + formatter.minimum_fraction_digits
        && rounded[fraction_end - 1] == b'0'
    {
        fraction_end -= 1;
    }
    if fraction_end > split {
        output.push('.');
        output
            .push_str(std::str::from_utf8(&rounded[split..fraction_end]).expect("decimal digits"));
    }
    output
}
