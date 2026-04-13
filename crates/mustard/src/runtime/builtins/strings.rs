use super::*;

impl Runtime {
    fn string_value_receiver(&self, value: Value, method: &str) -> MustardResult<String> {
        match value {
            Value::String(value) => Ok(value),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::StringObject(value) => Ok(value.clone()),
                _ => Err(MustardError::runtime(format!(
                    "TypeError: String.prototype.{method} called on incompatible receiver",
                ))),
            },
            _ => Err(MustardError::runtime(format!(
                "TypeError: String.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn string_receiver(&self, value: Value, method: &str) -> MustardResult<String> {
        match value {
            Value::String(value) => Ok(value),
            Value::Object(object) => match &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::StringObject(value) => Ok(value.clone()),
                ObjectKind::NumberObject(value) => self.to_string(Value::Number(*value)),
                ObjectKind::BooleanObject(value) => self.to_string(Value::Bool(*value)),
                _ => Err(MustardError::runtime(format!(
                    "TypeError: String.prototype.{method} called on incompatible receiver",
                ))),
            },
            _ => Err(MustardError::runtime(format!(
                "TypeError: String.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn string_search_pattern(
        &self,
        value: Value,
        method: &str,
    ) -> MustardResult<StringSearchPattern> {
        match value {
            Value::Object(object) if self.is_regexp_object(object) => {
                Ok(StringSearchPattern::RegExp {
                    object,
                    regex: self.regexp_object(object)?.clone(),
                })
            }
            value => {
                if is_callable(&value) {
                    return Err(MustardError::runtime(format!(
                        "TypeError: String.prototype.{method} does not support callback patterns",
                    )));
                }
                Ok(StringSearchPattern::Literal(self.to_string(value)?))
            }
        }
    }

    fn string_callback_replacement(
        &mut self,
        method: &str,
        callback: Value,
        input: &str,
        matched: &RegExpMatchData,
    ) -> MustardResult<String> {
        let mut args = vec![Value::String(
            input[matched.start_byte..matched.end_byte].to_string(),
        )];
        args.extend(
            matched
                .captures
                .iter()
                .map(|value| value.clone().map_or(Value::Undefined, Value::String)),
        );
        args.push(Value::Number(matched.start_index as f64));
        args.push(Value::String(input.to_string()));
        if !matched.named_groups.is_empty() {
            let groups = matched
                .named_groups
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        value.clone().map_or(Value::Undefined, Value::String),
                    )
                })
                .collect::<IndexMap<_, _>>();
            let object = self.insert_object(groups, ObjectKind::Plain)?;
            args.push(Value::Object(object));
        }
        let mut roots = vec![callback.clone()];
        roots.extend(args.iter().cloned());
        let value = self.with_temporary_roots(&roots, |runtime| {
            runtime.call_callback(
                callback.clone(),
                Value::Undefined,
                &args,
                CallbackCallOptions {
                    non_callable_message: &format!(
                        "TypeError: String.prototype.{method} replacement callback is not callable"
                    ),
                    host_suspension_message: &format!(
                        "TypeError: String.prototype.{method} callback replacements do not support host suspensions"
                    ),
                    unsettled_message: &format!(
                        "synchronous String.prototype.{method} callback did not settle"
                    ),
                    allow_host_suspension: false,
                    allow_pending_promise_result: false,
                },
            )
        })?;
        self.to_string(value)
    }

    pub(crate) fn call_string_trim(&self, this_value: Value) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "trim")?;
        Ok(Value::String(value.trim().to_string()))
    }

    pub(crate) fn call_string_trim_start(&self, this_value: Value) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "trimStart")?;
        Ok(Value::String(value.trim_start().to_string()))
    }

    pub(crate) fn call_string_trim_end(&self, this_value: Value) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "trimEnd")?;
        Ok(Value::String(value.trim_end().to_string()))
    }

    pub(crate) fn call_string_to_string(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::String(
            self.string_value_receiver(this_value, "toString")?,
        ))
    }

    pub(crate) fn call_string_value_of(&self, this_value: Value) -> MustardResult<Value> {
        Ok(Value::String(
            self.string_value_receiver(this_value, "valueOf")?,
        ))
    }

    pub(crate) fn call_string_includes(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "includes")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = clamp_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let haystack = chars[position..].iter().collect::<String>();
        Ok(Value::Bool(
            haystack.contains(&needle.iter().collect::<String>()),
        ))
    }

    pub(crate) fn call_string_starts_with(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "startsWith")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = clamp_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        Ok(Value::Bool(chars[position..].starts_with(&needle)))
    }

    pub(crate) fn call_string_ends_with(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "endsWith")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let end = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, chars.len()),
            None => chars.len(),
        };
        Ok(Value::Bool(chars[..end].ends_with(&needle)))
    }

    pub(crate) fn call_string_index_of(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "indexOf")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = clamp_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let index = if needle.is_empty() {
            position as f64
        } else {
            chars[position..]
                .windows(needle.len())
                .position(|window| window == needle.as_slice())
                .map(|offset| (position + offset) as f64)
                .unwrap_or(-1.0)
        };
        Ok(Value::Number(index))
    }

    pub(crate) fn call_string_last_index_of(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "lastIndexOf")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, chars.len()),
            None => chars.len(),
        };
        let index = if needle.is_empty() {
            position as f64
        } else if needle.len() > chars.len() {
            -1.0
        } else {
            let max_start = position.min(chars.len().saturating_sub(needle.len()));
            (0..=max_start)
                .rev()
                .find(|start| chars[*start..*start + needle.len()] == needle[..])
                .map(|index| index as f64)
                .unwrap_or(-1.0)
        };
        Ok(Value::Number(index))
    }

    pub(crate) fn call_string_char_at(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "charAt")?;
        let chars = value.chars().collect::<Vec<_>>();
        let index = clamp_index(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        Ok(Value::String(
            chars
                .get(index)
                .map(|ch| ch.to_string())
                .unwrap_or_default(),
        ))
    }

    pub(crate) fn call_string_at(&self, this_value: Value, args: &[Value]) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "at")?;
        let chars = value.chars().collect::<Vec<_>>();
        let index = self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?;
        let index = if index < 0 {
            chars.len() as i64 + index
        } else {
            index
        };
        if index < 0 || index >= chars.len() as i64 {
            Ok(Value::Undefined)
        } else {
            Ok(Value::String(chars[index as usize].to_string()))
        }
    }

    pub(crate) fn call_string_slice(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "slice")?;
        let chars = value.chars().collect::<Vec<_>>();
        let start = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let end = normalize_relative_bound(
            match args.get(1) {
                Some(value) => self.to_integer(value.clone())?,
                None => chars.len() as i64,
            },
            chars.len(),
        );
        let end = end.max(start);
        Ok(Value::String(chars[start..end].iter().collect()))
    }

    pub(crate) fn call_string_substring(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "substring")?;
        let chars = value.chars().collect::<Vec<_>>();
        let start = clamp_index(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let end = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, chars.len()),
            None => chars.len(),
        };
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Ok(Value::String(chars[start..end].iter().collect()))
    }

    pub(crate) fn call_string_to_lower_case(&self, this_value: Value) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "toLowerCase")?;
        Ok(Value::String(value.to_lowercase()))
    }

    pub(crate) fn call_string_to_upper_case(&self, this_value: Value) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "toUpperCase")?;
        Ok(Value::String(value.to_uppercase()))
    }

    pub(crate) fn call_string_repeat(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "repeat")?;
        let count = self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?;
        if !count.is_finite() || count < 0.0 {
            return Err(MustardError::runtime("RangeError: Invalid count value"));
        }
        let count = self.to_integer(Value::Number(count))? as usize;
        Ok(Value::String(value.repeat(count)))
    }

    pub(crate) fn call_string_concat(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let mut value = self.string_receiver(this_value, "concat")?;
        for arg in args {
            value.push_str(&self.to_string(arg.clone())?);
        }
        Ok(Value::String(value))
    }

    pub(crate) fn call_string_pad_start(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        self.call_string_pad(this_value, args, true)
    }

    pub(crate) fn call_string_pad_end(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        self.call_string_pad(this_value, args, false)
    }

    fn call_string_pad(
        &self,
        this_value: Value,
        args: &[Value],
        at_start: bool,
    ) -> MustardResult<Value> {
        let method = if at_start { "padStart" } else { "padEnd" };
        let value = self.string_receiver(this_value, method)?;
        let target_len = self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?;
        let target_len = usize::try_from(target_len.max(0)).unwrap_or(usize::MAX);
        let value_len = value.chars().count();
        if target_len <= value_len {
            return Ok(Value::String(value));
        }
        let fill = self.to_string(args.get(1).cloned().unwrap_or(Value::String(" ".into())))?;
        if fill.is_empty() {
            return Ok(Value::String(value));
        }
        let fill_chars = fill.chars().collect::<Vec<_>>();
        let pad_len = target_len - value_len;
        let padding = fill_chars
            .iter()
            .copied()
            .cycle()
            .take(pad_len)
            .collect::<String>();
        Ok(Value::String(if at_start {
            format!("{padding}{value}")
        } else {
            format!("{value}{padding}")
        }))
    }

    pub(crate) fn call_string_split(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "split")?;
        let limit = match args.get(1) {
            Some(value) => {
                let limit = self.to_integer(value.clone())?;
                if limit <= 0 {
                    0
                } else {
                    usize::try_from(limit).unwrap_or(usize::MAX)
                }
            }
            None => usize::MAX,
        };
        if limit == 0 {
            return Ok(Value::Array(
                self.insert_array(Vec::new(), IndexMap::new())?,
            ));
        }
        let pattern = match args.first() {
            None | Some(Value::Undefined) => None,
            Some(value) => Some(self.string_search_pattern(value.clone(), "split")?),
        };
        let elements = match pattern {
            None => vec![Value::String(value)],
            Some(StringSearchPattern::Literal(separator)) => {
                split_string_by_pattern(&value, Some(separator.as_str()), limit)
                    .into_iter()
                    .map(Value::String)
                    .collect()
            }
            Some(StringSearchPattern::RegExp { regex, .. }) => {
                let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                let mut elements = Vec::new();
                let mut last_end = 0usize;
                for matched in matches {
                    if elements.len() >= limit {
                        break;
                    }
                    elements.push(Value::String(
                        value[last_end..matched.start_byte].to_string(),
                    ));
                    if elements.len() >= limit {
                        break;
                    }
                    for capture in matched.captures {
                        elements.push(capture.map_or(Value::Undefined, Value::String));
                        if elements.len() >= limit {
                            break;
                        }
                    }
                    last_end = matched.end_byte;
                }
                if elements.len() < limit {
                    elements.push(Value::String(value[last_end..].to_string()));
                }
                elements
            }
        };
        Ok(Value::Array(self.insert_array(
            elements.into_iter().take(limit).collect(),
            IndexMap::new(),
        )?))
    }

    pub(crate) fn call_string_replace(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "replace")?;
        let search = self
            .string_search_pattern(args.first().cloned().unwrap_or(Value::Undefined), "replace")?;
        let replacement = args.get(1).cloned().unwrap_or(Value::Undefined);
        match (search, replacement.clone()) {
            (StringSearchPattern::Literal(search), replacement) if is_callable(&replacement) => {
                let matched = if search.is_empty() {
                    Some(RegExpMatchData {
                        start_byte: 0,
                        end_byte: 0,
                        start_index: 0,
                        end_index: 0,
                        captures: Vec::new(),
                        named_groups: IndexMap::new(),
                    })
                } else {
                    self.literal_match_data(&value, &search, 0)
                };
                if let Some(matched) = matched {
                    let replacement =
                        self.string_callback_replacement("replace", replacement, &value, &matched)?;
                    let mut result = String::new();
                    result.push_str(&value[..matched.start_byte]);
                    result.push_str(&replacement);
                    result.push_str(&value[matched.end_byte..]);
                    Ok(Value::String(result))
                } else {
                    Ok(Value::String(value))
                }
            }
            (StringSearchPattern::Literal(search), replacement) => Ok(Value::String(
                replace_first_string_match(&value, &search, &self.to_string(replacement)?),
            )),
            (StringSearchPattern::RegExp { regex, .. }, replacement)
                if is_callable(&replacement) =>
            {
                let all = regex.flags.contains('g');
                let matches = self.collect_regexp_matches_from_state(&regex, &value, all)?;
                if matches.is_empty() {
                    return Ok(Value::String(value));
                }
                let mut result = String::new();
                let mut last_end = 0usize;
                for matched in &matches {
                    result.push_str(&value[last_end..matched.start_byte]);
                    result.push_str(&self.string_callback_replacement(
                        "replace",
                        replacement.clone(),
                        &value,
                        matched,
                    )?);
                    last_end = matched.end_byte;
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
            (StringSearchPattern::RegExp { regex, .. }, replacement) => {
                let all = regex.flags.contains('g');
                let matches = self.collect_regexp_matches_from_state(&regex, &value, all)?;
                if matches.is_empty() {
                    return Ok(Value::String(value));
                }
                let replacement = self.to_string(replacement)?;
                let mut result = String::new();
                let mut last_end = 0usize;
                for matched in &matches {
                    result.push_str(&value[last_end..matched.start_byte]);
                    result.push_str(&expand_regexp_replacement_template(
                        &replacement,
                        &value,
                        matched,
                    ));
                    last_end = matched.end_byte;
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
        }
    }

    pub(crate) fn call_string_replace_all(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "replaceAll")?;
        let search = self.string_search_pattern(
            args.first().cloned().unwrap_or(Value::Undefined),
            "replaceAll",
        )?;
        let replacement = args.get(1).cloned().unwrap_or(Value::Undefined);
        match search {
            StringSearchPattern::Literal(search) if is_callable(&replacement) => {
                let mut matches = Vec::new();
                if search.is_empty() {
                    let total = value.chars().count();
                    for index in 0..=total {
                        let byte = char_index_to_byte_index(&value, index);
                        matches.push(RegExpMatchData {
                            start_byte: byte,
                            end_byte: byte,
                            start_index: index,
                            end_index: index,
                            captures: Vec::new(),
                            named_groups: IndexMap::new(),
                        });
                    }
                } else {
                    let mut start_index = 0usize;
                    while let Some(matched) = self.literal_match_data(&value, &search, start_index)
                    {
                        start_index = matched.end_index;
                        matches.push(matched);
                    }
                }
                let mut result = String::new();
                let mut last_end = 0usize;
                for matched in &matches {
                    result.push_str(&value[last_end..matched.start_byte]);
                    result.push_str(&self.string_callback_replacement(
                        "replaceAll",
                        replacement.clone(),
                        &value,
                        matched,
                    )?);
                    last_end = matched.end_byte;
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
            StringSearchPattern::Literal(search) => Ok(Value::String(replace_all_string_matches(
                &value,
                &search,
                &self.to_string(replacement)?,
            ))),
            StringSearchPattern::RegExp { regex, .. } => {
                if !regex.flags.contains('g') {
                    return Err(MustardError::runtime(
                        "TypeError: String.prototype.replaceAll requires a global RegExp",
                    ));
                }
                let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                if matches.is_empty() {
                    return Ok(Value::String(value));
                }
                let mut result = String::new();
                let mut last_end = 0usize;
                if is_callable(&replacement) {
                    for matched in &matches {
                        result.push_str(&value[last_end..matched.start_byte]);
                        result.push_str(&self.string_callback_replacement(
                            "replaceAll",
                            replacement.clone(),
                            &value,
                            matched,
                        )?);
                        last_end = matched.end_byte;
                    }
                } else {
                    let replacement = self.to_string(replacement)?;
                    for matched in &matches {
                        result.push_str(&value[last_end..matched.start_byte]);
                        result.push_str(&expand_regexp_replacement_template(
                            &replacement,
                            &value,
                            matched,
                        ));
                        last_end = matched.end_byte;
                    }
                }
                result.push_str(&value[last_end..]);
                Ok(Value::String(result))
            }
        }
    }

    pub(crate) fn call_string_search(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "search")?;
        let needle = self
            .string_search_pattern(args.first().cloned().unwrap_or(Value::Undefined), "search")?;
        Ok(Value::Number(match needle {
            StringSearchPattern::Literal(needle) => find_string_pattern(&value, &needle, 0)
                .map(|index| index as f64)
                .unwrap_or(-1.0),
            StringSearchPattern::RegExp { regex, .. } => self
                .first_regexp_match_from_state(&regex, &value, 0)?
                .map(|matched| matched.start_index as f64)
                .unwrap_or(-1.0),
        }))
    }

    pub(crate) fn call_string_match(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "match")?;
        let needle =
            self.string_search_pattern(args.first().cloned().unwrap_or(Value::Undefined), "match")?;
        match needle {
            StringSearchPattern::Literal(needle) => {
                let Some(index) = find_string_pattern(&value, &needle, 0) else {
                    return Ok(Value::Null);
                };
                let match_array = self.insert_array(
                    vec![Value::String(needle.clone())],
                    IndexMap::from([
                        ("index".to_string(), Value::Number(index as f64)),
                        ("input".to_string(), Value::String(value)),
                    ]),
                )?;
                Ok(Value::Array(match_array))
            }
            StringSearchPattern::RegExp { object, regex } => {
                if regex.flags.contains('g') {
                    self.regexp_object_mut(object)?.last_index = 0;
                    self.refresh_object_accounting(object)?;
                    let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                    if matches.is_empty() {
                        return Ok(Value::Null);
                    }
                    let array = self.insert_array(
                        matches
                            .into_iter()
                            .map(|matched| {
                                Value::String(
                                    value[matched.start_byte..matched.end_byte].to_string(),
                                )
                            })
                            .collect(),
                        IndexMap::new(),
                    )?;
                    Ok(Value::Array(array))
                } else {
                    let Some(matched) = self.first_regexp_match_from_state(&regex, &value, 0)?
                    else {
                        return Ok(Value::Null);
                    };
                    self.regexp_match_array_value(&value, &matched)
                }
            }
        }
    }

    pub(crate) fn call_string_match_all(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "matchAll")?;
        let needle = self.string_search_pattern(
            args.first().cloned().unwrap_or(Value::Undefined),
            "matchAll",
        )?;
        let matches = match needle {
            StringSearchPattern::Literal(needle) => collect_literal_matches(&value, &needle),
            StringSearchPattern::RegExp { object, regex } => {
                if !regex.flags.contains('g') {
                    return Err(MustardError::runtime(
                        "TypeError: String.prototype.matchAll requires a global RegExp",
                    ));
                }
                self.regexp_object_mut(object)?.last_index = 0;
                self.refresh_object_accounting(object)?;
                self.collect_regexp_matches_from_state(&regex, &value, true)?
            }
        };
        let mut values = Vec::with_capacity(matches.len());
        for matched in matches {
            values.push(self.regexp_match_array_value(&value, &matched)?);
        }
        let array = self.insert_array(values, IndexMap::new())?;
        self.call_array_values(Value::Array(array))
    }

    fn literal_match_data(
        &self,
        value: &str,
        needle: &str,
        start_index: usize,
    ) -> Option<RegExpMatchData> {
        let start_index = find_string_pattern(value, needle, start_index)?;
        let start_byte = char_index_to_byte_index(value, start_index);
        let end_index = start_index + needle.chars().count();
        let end_byte = char_index_to_byte_index(value, end_index);
        Some(RegExpMatchData {
            start_byte,
            end_byte,
            start_index,
            end_index,
            captures: Vec::new(),
            named_groups: IndexMap::new(),
        })
    }
}
