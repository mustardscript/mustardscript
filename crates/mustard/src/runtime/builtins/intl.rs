use super::*;

impl Runtime {
    fn normalize_intl_locale(&self, value: Option<Value>) -> MustardResult<String> {
        let Some(value) = value else {
            return Ok("en-US".to_string());
        };
        match value {
            Value::Undefined => Ok("en-US".to_string()),
            Value::String(locale) if locale == "en-US" => Ok(locale),
            Value::Array(array) => {
                let first = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .iter()
                    .flatten()
                    .next()
                    .cloned()
                    .unwrap_or(Value::String("en-US".to_string()));
                self.normalize_intl_locale(Some(first))
            }
            _ => Err(MustardError::runtime(
                "TypeError: Intl currently supports only the `en-US` locale",
            )),
        }
    }

    fn intl_options_object(&self, value: Option<Value>) -> MustardResult<Option<ObjectKey>> {
        match value.unwrap_or(Value::Undefined) {
            Value::Undefined | Value::Null => Ok(None),
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
        &self,
        object: Option<ObjectKey>,
        key: &str,
    ) -> MustardResult<Option<usize>> {
        match self.intl_option_value(object, key)? {
            Value::Undefined => Ok(None),
            value => {
                let digits = self.to_integer(value)?;
                if !(0..=20).contains(&digits) {
                    return Err(MustardError::runtime(format!(
                        "RangeError: Intl `{key}` must be between 0 and 20",
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
        if style == IntlNumberStyle::Currency && currency.as_deref() != Some("USD") {
            return Err(MustardError::runtime(
                "TypeError: Intl.NumberFormat currency style currently supports only `USD`",
            ));
        }
        let minimum_fraction_digits = self
            .intl_option_digits(options, "minimumFractionDigits")?
            .unwrap_or(match style {
                IntlNumberStyle::Currency => 2,
                _ => 0,
            });
        let maximum_fraction_digits = self
            .intl_option_digits(options, "maximumFractionDigits")?
            .unwrap_or(match style {
                IntlNumberStyle::Currency => 2,
                IntlNumberStyle::Percent => 0,
                IntlNumberStyle::Decimal => 3,
            });
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
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let formatter = self.intl_date_time_format_receiver(this_value, "format")?;
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
                IntlFieldStyle::Numeric => datetime.year.to_string(),
                IntlFieldStyle::TwoDigit => format!("{:02}", datetime.year.rem_euclid(100)),
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
            if let Some(minute) = formatter.minute {
                time_parts.push(Self::format_intl_field(datetime.minute, minute));
            }
            if let Some(second) = formatter.second {
                time_parts.push(Self::format_intl_field(datetime.second, second));
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

    fn format_intl_number(&self, formatter: &IntlNumberFormatObject, number: f64) -> String {
        let value = match formatter.style {
            IntlNumberStyle::Percent => number * 100.0,
            _ => number,
        };
        if !value.is_finite() {
            return value.to_string();
        }
        let rounded = format!("{:.*}", formatter.maximum_fraction_digits, value.abs());
        let mut parts = rounded.split('.').collect::<Vec<_>>();
        let mut integer = parts.remove(0).to_string();
        let mut fraction = parts.first().copied().unwrap_or("").to_string();
        while fraction.len() > formatter.minimum_fraction_digits && fraction.ends_with('0') {
            fraction.pop();
        }
        if formatter.use_grouping {
            integer = format_en_us_number_grouped(&integer);
        }
        let rendered = if fraction.is_empty() {
            integer
        } else {
            format!("{integer}.{fraction}")
        };
        match formatter.style {
            IntlNumberStyle::Decimal => {
                if value.is_sign_negative() {
                    format!("-{rendered}")
                } else {
                    rendered
                }
            }
            IntlNumberStyle::Percent => {
                let mut rendered = if value.is_sign_negative() {
                    format!("-{rendered}")
                } else {
                    rendered
                };
                rendered.push('%');
                rendered
            }
            IntlNumberStyle::Currency => {
                if value.is_sign_negative() {
                    format!("-${rendered}")
                } else {
                    format!("${rendered}")
                }
            }
        }
    }

    pub(crate) fn call_intl_number_format_format(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let formatter = self.intl_number_format_receiver(this_value, "format")?;
        let number = self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?;
        Ok(Value::String(self.format_intl_number(formatter, number)))
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
