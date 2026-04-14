use super::*;

impl Runtime {
    fn string_char_len(value: &str) -> usize {
        if value.is_ascii() {
            value.len()
        } else {
            value.chars().count()
        }
    }

    fn string_slice_by_char_range(value: &str, start: usize, end: usize) -> String {
        let start_byte = char_index_to_byte_index(value, start);
        let end_byte = char_index_to_byte_index(value, end);
        value[start_byte..end_byte].to_string()
    }

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

    fn record_ascii_substring_fast_path(&mut self, value: &str, needle: &str) -> bool {
        if !ascii_string_fast_paths_enabled() {
            return false;
        }
        if value.is_ascii() && needle.is_ascii() {
            self.record_ascii_substring_fast_path_hit();
            true
        } else {
            self.record_ascii_substring_fast_path_fallback();
            false
        }
    }

    fn try_ascii_token_regex_matches(
        &mut self,
        value: &str,
        regex: &RegExpObject,
        all: bool,
    ) -> Option<Vec<RegExpMatchData>> {
        if !is_ascii_literal_alternation_regex(&regex.pattern, &regex.flags) {
            return None;
        }
        if let Some(matches) =
            collect_ascii_literal_alternation_matches(value, &regex.pattern, &regex.flags, all)
        {
            self.record_ascii_token_regex_fast_path_hit();
            Some(matches)
        } else {
            self.record_ascii_token_regex_fast_path_fallback();
            None
        }
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
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "includes")?;
        let needle = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.record_literal_string_search();
        let position = self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?;
        if self.record_ascii_substring_fast_path(&value, &needle) {
            let position = clamp_index(position, value.len());
            return Ok(Value::Bool(value[position..].contains(&needle)));
        }
        let position = clamp_index(position, Self::string_char_len(&value));
        let start_byte = char_index_to_byte_index(&value, position);
        Ok(Value::Bool(value[start_byte..].contains(&needle)))
    }

    pub(crate) fn call_string_starts_with(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "startsWith")?;
        let needle = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.record_literal_string_search();
        let position = self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?;
        if self.record_ascii_substring_fast_path(&value, &needle) {
            let position = clamp_index(position, value.len());
            return Ok(Value::Bool(value[position..].starts_with(&needle)));
        }
        let position = clamp_index(position, Self::string_char_len(&value));
        let start_byte = char_index_to_byte_index(&value, position);
        Ok(Value::Bool(value[start_byte..].starts_with(&needle)))
    }

    pub(crate) fn call_string_ends_with(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "endsWith")?;
        let needle = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.record_literal_string_search();
        if self.record_ascii_substring_fast_path(&value, &needle) {
            let end = match args.get(1) {
                Some(position) => clamp_index(self.to_integer(position.clone())?, value.len()),
                None => value.len(),
            };
            return Ok(Value::Bool(value[..end].ends_with(&needle)));
        }
        let length = Self::string_char_len(&value);
        let end = match args.get(1) {
            Some(position) => clamp_index(self.to_integer(position.clone())?, length),
            None => length,
        };
        let end_byte = char_index_to_byte_index(&value, end);
        Ok(Value::Bool(value[..end_byte].ends_with(&needle)))
    }

    pub(crate) fn call_string_index_of(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "indexOf")?;
        let needle = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.record_literal_string_search();
        let position = self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?;
        if self.record_ascii_substring_fast_path(&value, &needle) {
            let position = clamp_index(position, value.len());
            let index = if needle.is_empty() {
                position as f64
            } else {
                value[position..]
                    .find(&needle)
                    .map(|byte_index| (position + byte_index) as f64)
                    .unwrap_or(-1.0)
            };
            return Ok(Value::Number(index));
        }
        let position = clamp_index(position, Self::string_char_len(&value));
        let index = if needle.is_empty() {
            position as f64
        } else {
            find_string_pattern(&value, &needle, position)
                .map(|index| index as f64)
                .unwrap_or(-1.0)
        };
        Ok(Value::Number(index))
    }

    pub(crate) fn call_string_last_index_of(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "lastIndexOf")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        self.record_literal_string_search();
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
        let len = Self::string_char_len(&value);
        let index = clamp_index(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            len,
        );
        if index >= len {
            return Ok(Value::String(String::new()));
        }
        Ok(Value::String(Self::string_slice_by_char_range(
            &value,
            index,
            index + 1,
        )))
    }

    pub(crate) fn call_string_at(&self, this_value: Value, args: &[Value]) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "at")?;
        let len = Self::string_char_len(&value);
        let index = self.to_integer(args.first().cloned().unwrap_or(Value::Undefined))?;
        let index = if index < 0 { len as i64 + index } else { index };
        if index < 0 || index >= len as i64 {
            Ok(Value::Undefined)
        } else {
            let index = index as usize;
            Ok(Value::String(Self::string_slice_by_char_range(
                &value,
                index,
                index + 1,
            )))
        }
    }

    pub(crate) fn call_string_slice(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "slice")?;
        let len = Self::string_char_len(&value);
        let start = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            len,
        );
        let end = normalize_relative_bound(
            match args.get(1) {
                Some(value) => self.to_integer(value.clone())?,
                None => len as i64,
            },
            len,
        );
        let end = end.max(start);
        Ok(Value::String(Self::string_slice_by_char_range(
            &value, start, end,
        )))
    }

    pub(crate) fn call_string_substring(
        &self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "substring")?;
        let len = Self::string_char_len(&value);
        let start = clamp_index(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            len,
        );
        let end = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, len),
            None => len,
        };
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Ok(Value::String(Self::string_slice_by_char_range(
            &value, start, end,
        )))
    }

    pub(crate) fn call_string_to_lower_case(&mut self, this_value: Value) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "toLowerCase")?;
        self.record_string_case_conversion();
        if ascii_string_fast_paths_enabled() {
            if let Some(lowered) = ascii_to_lowercase(&value) {
                self.record_ascii_case_fast_path_hit();
                return Ok(Value::String(lowered));
            }
            self.record_ascii_case_fast_path_fallback();
        }
        Ok(Value::String(value.to_lowercase()))
    }

    pub(crate) fn call_string_to_upper_case(&mut self, this_value: Value) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "toUpperCase")?;
        self.record_string_case_conversion();
        if ascii_string_fast_paths_enabled() {
            if let Some(upper) = ascii_to_uppercase(&value) {
                self.record_ascii_case_fast_path_hit();
                return Ok(Value::String(upper));
            }
            self.record_ascii_case_fast_path_fallback();
        }
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
                self.record_literal_string_search();
                split_string_by_pattern(&value, Some(separator.as_str()), limit)
                    .into_iter()
                    .map(Value::String)
                    .collect()
            }
            Some(StringSearchPattern::RegExp { regex, .. }) => {
                self.record_regex_search_or_replacement();
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
                self.record_literal_string_search();
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
            (StringSearchPattern::Literal(search), replacement) => {
                self.record_literal_string_search();
                Ok(Value::String(replace_first_string_match(
                    &value,
                    &search,
                    &self.to_string(replacement)?,
                )))
            }
            (StringSearchPattern::RegExp { regex, .. }, replacement)
                if is_callable(&replacement) =>
            {
                self.record_regex_search_or_replacement();
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
                self.record_regex_search_or_replacement();
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
                self.record_literal_string_search();
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
            StringSearchPattern::Literal(search) => {
                self.record_literal_string_search();
                Ok(Value::String(replace_all_string_matches(
                    &value,
                    &search,
                    &self.to_string(replacement)?,
                )))
            }
            StringSearchPattern::RegExp { regex, .. } => {
                self.record_regex_search_or_replacement();
                if !regex.flags.contains('g') {
                    return Err(MustardError::runtime(
                        "TypeError: String.prototype.replaceAll requires a global RegExp",
                    ));
                }
                if !is_callable(&replacement) {
                    let replacement = self.to_string(replacement.clone())?;
                    if ascii_string_fast_paths_enabled() {
                        if let Some(cleaned) = try_ascii_cleanup_replace_all(
                            &value,
                            &regex.pattern,
                            &regex.flags,
                            &replacement,
                        ) {
                            self.record_ascii_cleanup_fast_path_hit();
                            return Ok(Value::String(cleaned));
                        }
                        self.record_ascii_cleanup_fast_path_fallback();
                    }
                    let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                    if matches.is_empty() {
                        return Ok(Value::String(value));
                    }
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
                    return Ok(Value::String(result));
                }
                let matches = self.collect_regexp_matches_from_state(&regex, &value, true)?;
                if matches.is_empty() {
                    return Ok(Value::String(value));
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
        }
    }

    pub(crate) fn call_string_search(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = self.string_receiver(this_value, "search")?;
        let needle = self
            .string_search_pattern(args.first().cloned().unwrap_or(Value::Undefined), "search")?;
        Ok(Value::Number(match needle {
            StringSearchPattern::Literal(needle) => {
                self.record_literal_string_search();
                self.record_ascii_substring_fast_path(&value, &needle);
                find_string_pattern(&value, &needle, 0)
                    .map(|index| index as f64)
                    .unwrap_or(-1.0)
            }
            StringSearchPattern::RegExp { regex, .. } => {
                self.record_regex_search_or_replacement();
                if let Some(matches) = self.try_ascii_token_regex_matches(&value, &regex, false) {
                    matches
                        .first()
                        .map(|matched| matched.start_index as f64)
                        .unwrap_or(-1.0)
                } else {
                    self.first_regexp_match_from_state(&regex, &value, 0)?
                        .map(|matched| matched.start_index as f64)
                        .unwrap_or(-1.0)
                }
            }
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
                self.record_literal_string_search();
                self.record_ascii_substring_fast_path(&value, &needle);
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
                self.record_regex_search_or_replacement();
                if regex.flags.contains('g') {
                    self.regexp_object_mut(object)?.last_index = 0;
                    let matches = if let Some(matches) =
                        self.try_ascii_token_regex_matches(&value, &regex, true)
                    {
                        matches
                    } else {
                        self.collect_regexp_matches_from_state(&regex, &value, true)?
                    };
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
            StringSearchPattern::Literal(needle) => {
                self.record_literal_string_search();
                collect_literal_matches(&value, &needle)
            }
            StringSearchPattern::RegExp { object, regex } => {
                self.record_regex_search_or_replacement();
                if !regex.flags.contains('g') {
                    return Err(MustardError::runtime(
                        "TypeError: String.prototype.matchAll requires a global RegExp",
                    ));
                }
                self.regexp_object_mut(object)?.last_index = 0;
                if let Some(matches) = self.try_ascii_token_regex_matches(&value, &regex, true) {
                    matches
                } else {
                    self.collect_regexp_matches_from_state(&regex, &value, true)?
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use std::sync::Arc;

    use crate::{
        ExecutionOptions, ExecutionStep, RuntimeLimits, StructuredValue, compile,
        lower_to_bytecode, start_shared_bytecode_with_metrics,
    };

    fn run_with_metrics(source: &str) -> (StructuredValue, RuntimeDebugMetrics) {
        let program = compile(source).expect("source should compile");
        let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
        let (step, metrics) = start_shared_bytecode_with_metrics(
            Arc::new(bytecode),
            ExecutionOptions {
                inputs: IndexMap::new(),
                capabilities: Vec::new(),
                limits: RuntimeLimits::default(),
                cancellation_token: None,
            },
        )
        .expect("program should execute");
        match step {
            ExecutionStep::Completed(value) => (value, metrics),
            ExecutionStep::Suspended(_) => panic!("program should not suspend"),
        }
    }

    #[test]
    fn ascii_string_fast_paths_record_hits_when_enabled() {
        let _guard = super::support::override_ascii_string_fast_paths_for_tests(true);
        let (value, metrics) = run_with_metrics(
            r#"
            const lower = "FOO".toLowerCase();
            const found = lower.includes("oo");
            const compact = "A\tB\nC".replaceAll(/\s+/g, " ");
            [lower, found, compact];
            "#,
        );

        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::from("foo"),
                StructuredValue::Bool(true),
                StructuredValue::from("A B C"),
            ])
        );
        assert!(metrics.ascii_case_fast_path_hits > 0);
        assert_eq!(metrics.ascii_case_fast_path_fallbacks, 0);
        assert!(metrics.ascii_substring_fast_path_hits > 0);
        assert_eq!(metrics.ascii_substring_fast_path_fallbacks, 0);
        assert!(metrics.ascii_cleanup_fast_path_hits > 0);
        assert_eq!(metrics.ascii_cleanup_fast_path_fallbacks, 0);
    }

    #[test]
    fn ascii_string_fast_paths_record_fallbacks_for_non_ascii_when_enabled() {
        let _guard = super::support::override_ascii_string_fast_paths_for_tests(true);
        let (value, metrics) = run_with_metrics(
            r#"
            const lower = "CAFÉ".toLowerCase();
            const found = lower.includes("fé");
            const compact = "CAFÉ\n".replaceAll(/\s+/g, " ");
            [lower, found, compact];
            "#,
        );

        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::from("café"),
                StructuredValue::Bool(true),
                StructuredValue::from("CAFÉ "),
            ])
        );
        assert_eq!(metrics.ascii_case_fast_path_hits, 0);
        assert!(metrics.ascii_case_fast_path_fallbacks > 0);
        assert_eq!(metrics.ascii_substring_fast_path_hits, 0);
        assert!(metrics.ascii_substring_fast_path_fallbacks > 0);
        assert_eq!(metrics.ascii_cleanup_fast_path_hits, 0);
        assert!(metrics.ascii_cleanup_fast_path_fallbacks > 0);
    }

    #[test]
    fn ascii_token_regex_fast_path_hits_for_global_literal_alternations() {
        let (value, metrics) = run_with_metrics(
            r#"
            const found = "jwks timeout token rate limit dns certificate"
              .match(/jwks|timeout|token|rate limit|dns|certificate/g);
            const first = "jwks timeout".search(/jwks|timeout/g);
            const all = Array.from("throttle timeout".matchAll(/timeout|throttle/g)).length;
            [found.length, found[3], first, all];
            "#,
        );

        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::from(6.0),
                StructuredValue::from("rate limit"),
                StructuredValue::from(0.0),
                StructuredValue::from(2.0),
            ])
        );
        assert!(metrics.ascii_token_regex_fast_path_hits >= 3);
        assert_eq!(metrics.ascii_token_regex_fast_path_fallbacks, 0);
    }

    #[test]
    fn ascii_token_regex_fast_path_preserves_alternative_order() {
        let (value, metrics) = run_with_metrics(
            r#"
            const found = "rate limit".match(/rate|rate limit/g);
            [found.length, found[0]];
            "#,
        );

        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::from(1.0),
                StructuredValue::from("rate"),
            ])
        );
        assert!(metrics.ascii_token_regex_fast_path_hits > 0);
    }

    #[test]
    fn ascii_token_regex_fast_path_records_fallbacks_for_non_ascii_input() {
        let (value, metrics) = run_with_metrics(
            r#"
            const lowered = "CAFÉ timeout".toLowerCase();
            const found = lowered.match(/caf|timeout/g);
            const first = lowered.search(/caf|timeout/g);
            [found.length, found[0], found[1], first];
            "#,
        );

        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::from(2.0),
                StructuredValue::from("caf"),
                StructuredValue::from("timeout"),
                StructuredValue::from(0.0),
            ])
        );
        assert_eq!(metrics.ascii_token_regex_fast_path_hits, 0);
        assert!(metrics.ascii_token_regex_fast_path_fallbacks >= 2);
    }
}
