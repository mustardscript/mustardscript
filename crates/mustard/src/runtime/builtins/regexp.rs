use super::*;
use regex::{Captures, Regex, RegexBuilder};

struct CompiledRegExp {
    flags: RegExpFlagsState,
    regex: Regex,
}

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
        let mut groups = IndexMap::new();
        for (name, value) in &matched.named_groups {
            groups.insert(
                name.clone(),
                value.clone().map_or(Value::Undefined, Value::String),
            );
        }
        let mut properties = IndexMap::from([
            (
                "index".to_string(),
                Value::Number(matched.start_index as f64),
            ),
            ("input".to_string(), Value::String(input.to_string())),
        ]);
        if !groups.is_empty() {
            properties.insert(
                "groups".to_string(),
                Value::Object(self.insert_object(groups, ObjectKind::Plain)?),
            );
        }
        let mut elements = Vec::with_capacity(matched.captures.len() + 1);
        elements.push(Value::String(
            input[matched.start_byte..matched.end_byte].to_string(),
        ));
        elements.extend(
            matched
                .captures
                .iter()
                .map(|value| value.clone().map_or(Value::Undefined, Value::String)),
        );
        Ok(Value::Array(self.insert_array(elements, properties)?))
    }

    pub(crate) fn call_regexp_exec(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let regex = self.regexp_receiver(this_value, "exec")?;
        let input = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
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
        Ok(Value::Bool(
            self.first_regexp_match(regex, &input)?.is_some(),
        ))
    }

    pub(crate) fn make_regexp_value(
        &mut self,
        pattern: String,
        flags: String,
    ) -> MustardResult<Value> {
        self.validate_regexp_flags(&flags)?;
        self.compile_regexp(&pattern, &flags)?;
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
        };
        let mut seen = HashSet::new();
        for flag in flags.chars() {
            if !seen.insert(flag) {
                return Err(MustardError::runtime(format!(
                    "SyntaxError: duplicate regular expression flag `{flag}`",
                )));
            }
            match flag {
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

    fn compile_regexp(&self, pattern: &str, flags: &str) -> MustardResult<CompiledRegExp> {
        let flags = self.validate_regexp_flags(flags)?;
        let mut builder = RegexBuilder::new(pattern);
        builder.case_insensitive(flags.ignore_case);
        builder.multi_line(flags.multiline);
        builder.dot_matches_new_line(flags.dot_all);
        // The Rust engine operates over UTF-8 strings, so keep Unicode mode
        // enabled even without the JS `u` flag. This preserves the supported
        // text-regexp subset while avoiding non-UTF-8 byte classes.
        builder.unicode(true);
        let regex = builder.build().map_err(|error| {
            MustardError::runtime(format!("SyntaxError: invalid regular expression: {error}"))
        })?;
        Ok(CompiledRegExp { flags, regex })
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
        &self,
        compiled: &CompiledRegExp,
        text: &str,
        captures: &Captures<'_>,
    ) -> MustardResult<RegExpMatchData> {
        let matched = captures
            .get(0)
            .ok_or_else(|| MustardError::runtime("regex match missing full capture"))?;
        let named_groups = compiled
            .regex
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
            start_index: byte_index_to_char_index(text, matched.start()),
            end_index: byte_index_to_char_index(text, matched.end()),
            captures: (1..captures.len())
                .map(|index| {
                    captures
                        .get(index)
                        .map(|capture| capture.as_str().to_string())
                })
                .collect(),
            named_groups,
        })
    }

    fn first_regexp_match_with_compiled(
        &self,
        compiled: &CompiledRegExp,
        text: &str,
        start_index: usize,
    ) -> MustardResult<Option<RegExpMatchData>> {
        self.check_cancellation()?;
        let start_byte = char_index_to_byte_index(text, start_index);
        let Some(captures) = compiled.regex.captures_at(text, start_byte) else {
            return Ok(None);
        };
        let matched = captures
            .get(0)
            .ok_or_else(|| MustardError::runtime("regex match missing full capture"))?;
        if compiled.flags.sticky && matched.start() != start_byte {
            return Ok(None);
        }
        self.regexp_match_data_from_captures(compiled, text, &captures)
            .map(Some)
    }

    pub(crate) fn first_regexp_match_from_state(
        &self,
        regex: &RegExpObject,
        text: &str,
        start_index: usize,
    ) -> MustardResult<Option<RegExpMatchData>> {
        let compiled = self.compile_regexp(&regex.pattern, &regex.flags)?;
        self.first_regexp_match_with_compiled(&compiled, text, start_index)
    }

    fn first_regexp_match(
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
            let next_index = matched.as_ref().map_or(0, |matched| {
                if matched.start_byte == matched.end_byte {
                    advance_char_index(text, matched.start_index)
                } else {
                    matched.end_index
                }
            });
            self.regexp_object_mut(regex_key)?.last_index = next_index;
        }
        Ok(matched)
    }

    pub(crate) fn collect_regexp_matches_from_state(
        &self,
        regex: &RegExpObject,
        text: &str,
        all: bool,
    ) -> MustardResult<Vec<RegExpMatchData>> {
        let compiled = self.compile_regexp(&regex.pattern, &regex.flags)?;
        let mut matches = Vec::new();
        let mut start_index = 0usize;
        loop {
            let Some(matched) =
                self.first_regexp_match_with_compiled(&compiled, text, start_index)?
            else {
                break;
            };
            let next_index = if matched.start_byte == matched.end_byte {
                advance_char_index(text, matched.start_index)
            } else {
                matched.end_index
            };
            matches.push(matched);
            if !all {
                break;
            }
            if next_index < start_index {
                break;
            }
            start_index = next_index;
        }
        Ok(matches)
    }
}
