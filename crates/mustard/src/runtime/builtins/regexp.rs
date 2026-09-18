use super::*;
use regex::{Captures, Regex, RegexBuilder};

impl Runtime {
    pub(crate) fn construct_regexp(&mut self, args: &[Value]) -> MustardResult<Value> {
        let pattern_arg = args.first().cloned().unwrap_or(Value::Undefined);
        let flags_arg = args.get(1).cloned().unwrap_or(Value::Undefined);
        let (pattern, flags) = match pattern_arg {
            Value::Object(object) if self.is_regexp_object(object) => {
                let regex = self.regexp_object(object)?.clone();
                if matches!(flags_arg, Value::Undefined) {
                    (regex.pattern, regex.flags)
                } else {
                    (regex.pattern, self.to_string(flags_arg)?)
                }
            }
            value => {
                let pattern = if matches!(value, Value::Undefined) {
                    String::new()
                } else {
                    self.to_string(value)?
                };
                let flags = if matches!(flags_arg, Value::Undefined) {
                    String::new()
                } else {
                    self.to_string(flags_arg)?
                };
                (pattern, flags)
            }
        };
        self.make_regexp_value(pattern, flags)
    }

    fn regexp_receiver(&self, value: Value, method: &str) -> MustardResult<ObjectKey> {
        match value {
            Value::Object(key) if self.is_regexp_object(key) => Ok(key),
            _ => Err(MustardError::runtime(format!(
                "TypeError: RegExp.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn regexp_match_array_value(
        &mut self,
        input: &str,
        matched: &RegExpMatchData,
    ) -> MustardResult<Value> {
        let mut elements = vec![Value::String(
            input[matched.start_byte..matched.end_byte].to_string(),
        )];
        elements.extend(
            matched
                .captures
                .iter()
                .map(|value| value.clone().map_or(Value::Undefined, Value::String)),
        );
        let properties = IndexMap::from([
            ("index".into(), Value::Number(matched.start_index as f64)),
            ("input".into(), Value::String(input.to_string())),
            ("groups".into(), Value::Undefined),
        ]);
        let result = Value::Array(self.insert_array(elements, properties)?);
        self.with_temporary_roots(std::slice::from_ref(&result), |runtime| {
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
                    .collect();
                let groups =
                    Value::Object(runtime.insert_object(groups, ObjectKind::NullPrototype)?);
                runtime.with_temporary_roots(std::slice::from_ref(&groups), |runtime| {
                    runtime.set_property_static(result.clone(), "groups", groups.clone())
                })?;
            }
            if let Some(indices) = &matched.indices {
                let indices_value = Value::Array(runtime.insert_array(
                    Vec::new(),
                    IndexMap::from([("groups".into(), Value::Undefined)]),
                )?);
                runtime.with_temporary_roots(std::slice::from_ref(&indices_value), |runtime| {
                    let Value::Array(indices_array) = indices_value else {
                        unreachable!();
                    };
                    for pair in indices {
                        runtime.charge_native_helper_work(1)?;
                        let value = match pair {
                            Some((start, end)) => Value::Array(runtime.insert_array(
                                vec![Value::Number(*start as f64), Value::Number(*end as f64)],
                                IndexMap::new(),
                            )?),
                            None => Value::Undefined,
                        };
                        runtime.with_temporary_roots(std::slice::from_ref(&value), |runtime| {
                            runtime.push_array_element(indices_array, Some(value.clone()))
                        })?;
                    }
                    if !matched.named_indices.is_empty() {
                        let mut groups = IndexMap::new();
                        for (name, index) in &matched.named_indices {
                            groups.insert(
                                name.clone(),
                                runtime.get_property_by_key(
                                    indices_value.clone(),
                                    &index.to_string(),
                                    false,
                                )?,
                            );
                        }
                        let groups = Value::Object(
                            runtime.insert_object(groups, ObjectKind::NullPrototype)?,
                        );
                        runtime.with_temporary_roots(std::slice::from_ref(&groups), |runtime| {
                            runtime.set_property_static(
                                indices_value.clone(),
                                "groups",
                                groups.clone(),
                            )
                        })?;
                    }
                    runtime.set_property_static(result.clone(), "indices", indices_value.clone())
                })?;
            }
            Ok(result.clone())
        })
    }

    pub(crate) fn call_regexp_exec(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let regex = self.regexp_receiver(this_value, "exec")?;
        let input = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.record_regex_search_or_replacement();
        let Some(matched) = self.first_regexp_match(regex, &input)? else {
            return Ok(Value::Null);
        };
        self.regexp_match_array_value(&input, &matched)
    }

    pub(crate) fn call_regexp_test(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let regex = self.regexp_receiver(this_value, "test")?;
        let input = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
        self.record_regex_search_or_replacement();
        Ok(Value::Bool(
            self.first_regexp_match(regex, &input)?.is_some(),
        ))
    }

    pub(crate) fn make_regexp_value(
        &mut self,
        pattern: String,
        flags: String,
    ) -> MustardResult<Value> {
        self.compiled_regexp(&pattern, &flags)?;
        let flags = "dgimsuy"
            .chars()
            .filter(|flag| flags.contains(*flag))
            .collect();
        let object = self.insert_object(
            IndexMap::new(),
            ObjectKind::RegExp(RegExpObject {
                pattern,
                flags,
                last_index: 0,
            }),
        )?;
        Ok(Value::Object(object))
    }

    fn validate_regexp_flags(&self, flags: &str) -> MustardResult<RegExpFlagsState> {
        let mut state = RegExpFlagsState {
            global: false,
            ignore_case: false,
            multiline: false,
            dot_all: false,
            unicode: false,
            sticky: false,
            has_indices: false,
        };
        let mut seen = HashSet::new();
        for flag in flags.chars() {
            if !seen.insert(flag) {
                return Err(MustardError::runtime(format!(
                    "SyntaxError: duplicate regular expression flag `{flag}`",
                )));
            }
            match flag {
                'd' => state.has_indices = true,
                'g' => state.global = true,
                'i' => state.ignore_case = true,
                'm' => state.multiline = true,
                's' => state.dot_all = true,
                'u' => state.unicode = true,
                'y' => state.sticky = true,
                _ => {
                    return Err(MustardError::runtime(format!(
                        "SyntaxError: unsupported regular expression flag `{flag}`",
                    )));
                }
            }
        }
        Ok(state)
    }

    fn compiled_regexp(
        &mut self,
        pattern: &str,
        flags: &str,
    ) -> MustardResult<(RegExpFlagsState, Regex)> {
        let flags_state = self.validate_regexp_flags(flags)?;
        let cache_key = (pattern.to_string(), flags.to_string());
        if let Some(regex) = self.regex_cache.shift_remove(&cache_key) {
            self.regex_cache.insert(cache_key, regex.clone());
            return Ok((flags_state, regex));
        }
        let normalized = self.normalize_regexp_pattern(pattern, flags_state)?;
        let mut builder = RegexBuilder::new(&normalized);
        builder.case_insensitive(false);
        builder.multi_line(flags_state.multiline);
        builder.dot_matches_new_line(flags_state.dot_all);
        // The Rust engine operates over UTF-8 strings, so keep Unicode mode
        // enabled even without the JS `u` flag. This preserves the supported
        // text-regexp subset while avoiding non-UTF-8 byte classes.
        builder.unicode(true);
        // The engine's own compiled-program and lazy-DFA allocations are bounded,
        // independently of guest object allocation and the parser's temporary work.
        let engine_budget = (self.limits.heap_limit_bytes / 16).clamp(1024, 10 * 1024 * 1024);
        builder
            .size_limit(engine_budget)
            .dfa_size_limit(engine_budget);
        let regex = builder.build().map_err(|error| {
            MustardError::runtime(format!("SyntaxError: invalid regular expression: {error}"))
        })?;
        if self.regex_cache.len() >= 8 {
            self.regex_cache.shift_remove_index(0);
        }
        self.regex_cache.insert(cache_key, regex.clone());
        Ok((flags_state, regex))
    }

    pub(crate) fn is_regexp_object(&self, key: ObjectKey) -> bool {
        self.objects
            .get(key)
            .is_some_and(|object| matches!(object.kind, ObjectKind::RegExp(_)))
    }

    pub(crate) fn is_date_object(&self, key: ObjectKey) -> bool {
        self.objects
            .get(key)
            .is_some_and(|object| matches!(object.kind, ObjectKind::Date(_)))
    }

    pub(crate) fn date_object(&self, key: ObjectKey) -> MustardResult<&DateObject> {
        match &self
            .objects
            .get(key)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .kind
        {
            ObjectKind::Date(date) => Ok(date),
            _ => Err(MustardError::runtime("date missing")),
        }
    }

    pub(crate) fn regexp_object(&self, key: ObjectKey) -> MustardResult<&RegExpObject> {
        match &self
            .objects
            .get(key)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .kind
        {
            ObjectKind::RegExp(regex) => Ok(regex),
            _ => Err(MustardError::runtime("regexp missing")),
        }
    }

    pub(crate) fn regexp_object_mut(&mut self, key: ObjectKey) -> MustardResult<&mut RegExpObject> {
        match &mut self
            .objects
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .kind
        {
            ObjectKind::RegExp(regex) => Ok(regex),
            _ => Err(MustardError::runtime("regexp missing")),
        }
    }

    fn regexp_match_data_from_captures(
        &mut self,
        compiled: &Regex,
        text: &str,
        captures: &Captures<'_>,
        has_indices: bool,
        start_byte: usize,
        start_index: usize,
    ) -> MustardResult<RegExpMatchData> {
        let capture_bytes = captures
            .iter()
            .flatten()
            .map(|capture| capture.len())
            .sum::<usize>();
        self.charge_native_helper_work(
            capture_bytes
                .saturating_mul(2)
                .saturating_add(captures.len()),
        )?;
        self.ensure_heap_capacity(
            capture_bytes
                .saturating_mul(2)
                .saturating_add(captures.len().saturating_mul(96)),
        )?;
        let matched = captures
            .get(0)
            .ok_or_else(|| MustardError::runtime("regex match missing full capture"))?;
        let match_start_index = start_index + text[start_byte..matched.start()].chars().count();
        let match_end_index = match_start_index + matched.as_str().chars().count();
        let indices = if has_indices {
            // Capture ranges can overlap or be nested. Sort their byte endpoints
            // and count each character in the match only once, never from the
            // beginning of the full input for each capture/match.
            let mut endpoints = captures
                .iter()
                .flatten()
                .flat_map(|capture| [capture.start(), capture.end()])
                .collect::<Vec<_>>();
            self.charge_native_helper_work(
                matched.len().saturating_add(
                    endpoints
                        .len()
                        .saturating_mul(endpoints.len().max(1).ilog2() as usize + 1),
                ),
            )?;
            endpoints.sort_unstable();
            endpoints.dedup();
            let mut byte = matched.start();
            let mut index = match_start_index;
            let positions = endpoints
                .iter()
                .map(|&end| {
                    index += text[byte..end].chars().count();
                    byte = end;
                    index
                })
                .collect::<Vec<_>>();
            Some(
                captures
                    .iter()
                    .map(|capture| {
                        capture.map(|capture| {
                            (
                                positions[endpoints
                                    .binary_search(&capture.start())
                                    .expect("capture endpoint")],
                                positions[endpoints
                                    .binary_search(&capture.end())
                                    .expect("capture endpoint")],
                            )
                        })
                    })
                    .collect(),
            )
        } else {
            None
        };
        let named_groups = compiled
            .capture_names()
            .enumerate()
            .skip(1)
            .filter_map(|(index, name)| {
                name.map(|name| {
                    (
                        name.to_string(),
                        captures
                            .get(index)
                            .map(|capture| capture.as_str().to_string()),
                    )
                })
            })
            .collect::<IndexMap<_, _>>();
        Ok(RegExpMatchData {
            start_byte: matched.start(),
            end_byte: matched.end(),
            start_index: match_start_index,
            end_index: match_end_index,
            captures: (1..captures.len())
                .map(|index| {
                    captures
                        .get(index)
                        .map(|capture| capture.as_str().to_string())
                })
                .collect(),
            named_groups,
            indices,
            named_indices: compiled
                .capture_names()
                .enumerate()
                .filter_map(|(index, name)| name.map(|name| (name.to_string(), index)))
                .collect(),
        })
    }

    fn validate_regexp_input(
        &mut self,
        compiled: &Regex,
        flags: RegExpFlagsState,
        text: &str,
    ) -> MustardResult<()> {
        if flags.unicode
            && flags.ignore_case
            && (compiled.as_str().contains(r"(?-u:\b)") || compiled.as_str().contains(r"(?-u:\B)"))
        {
            self.charge_native_helper_work(text.len())?;
            if text.contains(['ſ', 'K']) {
                return Err(MustardError::runtime(
                    "TypeError: iu word boundaries on long-s or Kelvin-sign input are not supported by the linear regexp profile",
                ));
            }
        }
        Ok(())
    }

    fn regexp_start_byte(&mut self, text: &str, start_index: usize) -> MustardResult<usize> {
        if start_index > text.len() {
            return Ok(text.len().saturating_add(1));
        }
        let mut index = 0;
        for (byte, _) in text.char_indices() {
            if index == start_index {
                self.charge_native_helper_work(byte)?;
                return Ok(byte);
            }
            index += 1;
        }
        self.charge_native_helper_work(text.len())?;
        Ok(if index == start_index {
            text.len()
        } else {
            text.len().saturating_add(1)
        })
    }

    fn first_regexp_match_with_compiled(
        &mut self,
        compiled: &Regex,
        flags: RegExpFlagsState,
        text: &str,
        start_byte: usize,
        start_index: usize,
    ) -> MustardResult<Option<RegExpMatchData>> {
        self.charge_native_helper_work(1)?;
        if start_byte > text.len() {
            return Ok(None);
        }
        if compiled.captures_len() == 1 {
            let matched = compiled.find_at(text, start_byte);
            self.charge_native_helper_work(
                matched.as_ref().map_or(text.len(), |m| m.end()) - start_byte,
            )?;
            let Some(matched) = matched else {
                return Ok(None);
            };
            if flags.sticky && matched.start() != start_byte {
                return Ok(None);
            }
            let match_start_index = start_index + text[start_byte..matched.start()].chars().count();
            let match_end_index = match_start_index + matched.as_str().chars().count();
            return Ok(Some(RegExpMatchData {
                start_byte: matched.start(),
                end_byte: matched.end(),
                start_index: match_start_index,
                end_index: match_end_index,
                captures: Vec::new(),
                named_groups: IndexMap::new(),
                indices: flags
                    .has_indices
                    .then(|| vec![Some((match_start_index, match_end_index))]),
                named_indices: IndexMap::new(),
            }));
        }
        let captures = compiled.captures_at(text, start_byte);
        let matched = captures.as_ref().and_then(|captures| captures.get(0));
        self.charge_native_helper_work(
            matched.as_ref().map_or(text.len(), |m| m.end()) - start_byte,
        )?;
        let Some(captures) = captures else {
            return Ok(None);
        };
        let matched =
            matched.ok_or_else(|| MustardError::runtime("regex match missing full capture"))?;
        if flags.sticky && matched.start() != start_byte {
            return Ok(None);
        }
        self.regexp_match_data_from_captures(
            compiled,
            text,
            &captures,
            flags.has_indices,
            start_byte,
            start_index,
        )
        .map(Some)
    }

    pub(crate) fn first_regexp_match_from_state(
        &mut self,
        regex: &RegExpObject,
        text: &str,
        start_index: usize,
    ) -> MustardResult<Option<RegExpMatchData>> {
        let (flags, compiled) = self.compiled_regexp(&regex.pattern, &regex.flags)?;
        self.validate_regexp_input(&compiled, flags, text)?;
        let start_byte = self.regexp_start_byte(text, start_index)?;
        self.first_regexp_match_with_compiled(&compiled, flags, text, start_byte, start_index)
    }

    pub(super) fn first_regexp_match(
        &mut self,
        regex_key: ObjectKey,
        text: &str,
    ) -> MustardResult<Option<RegExpMatchData>> {
        let regex = self.regexp_object(regex_key)?.clone();
        let flags = self.validate_regexp_flags(&regex.flags)?;
        let start_index = if flags.global || flags.sticky {
            regex.last_index
        } else {
            0
        };
        let matched = self.first_regexp_match_from_state(&regex, text, start_index)?;
        if flags.global || flags.sticky {
            let next_index = matched.as_ref().map_or(0, |matched| matched.end_index);
            self.regexp_object_mut(regex_key)?.last_index = next_index;
        }
        Ok(matched)
    }

    pub(crate) fn collect_regexp_matches_from_state(
        &mut self,
        regex: &RegExpObject,
        text: &str,
        all: bool,
    ) -> MustardResult<Vec<RegExpMatchData>> {
        self.collect_regexp_matches_starting(regex, text, all, 0)
    }

    pub(super) fn collect_regexp_matches_starting(
        &mut self,
        regex: &RegExpObject,
        text: &str,
        all: bool,
        mut start_index: usize,
    ) -> MustardResult<Vec<RegExpMatchData>> {
        let (flags, compiled) = self.compiled_regexp(&regex.pattern, &regex.flags)?;
        self.validate_regexp_input(&compiled, flags, text)?;
        let mut start_byte = self.regexp_start_byte(text, start_index)?;
        let mut matches = Vec::new();
        loop {
            let Some(matched) = self.first_regexp_match_with_compiled(
                &compiled,
                flags,
                text,
                start_byte,
                start_index,
            )?
            else {
                break;
            };
            let empty = matched.start_byte == matched.end_byte;
            let at_end = matched.end_byte == text.len();
            start_index = matched.end_index;
            start_byte = matched.end_byte;
            if empty && !at_end {
                start_byte += text[start_byte..]
                    .chars()
                    .next()
                    .expect("remaining character")
                    .len_utf8();
                start_index += 1;
                self.charge_native_helper_work(1)?;
            }
            self.ensure_heap_capacity(
                matches
                    .len()
                    .saturating_add(1)
                    .saturating_mul(std::mem::size_of::<RegExpMatchData>()),
            )?;
            matches.push(matched);
            if !all || (empty && at_end) {
                break;
            }
        }
        Ok(matches)
    }
}
