use super::*;

impl Runtime {
    pub(crate) fn construct_regexp(&mut self, args: &[Value]) -> JsliteResult<Value> {
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

    fn regexp_receiver(&self, value: Value, method: &str) -> JsliteResult<ObjectKey> {
        match value {
            Value::Object(key) if self.is_regexp_object(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: RegExp.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn regexp_match_array_value(
        &mut self,
        input: &str,
        matched: &RegExpMatchData,
    ) -> JsliteResult<Value> {
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
    ) -> JsliteResult<Value> {
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
    ) -> JsliteResult<Value> {
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
    ) -> JsliteResult<Value> {
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

    fn validate_regexp_flags(&self, flags: &str) -> JsliteResult<RegExpFlagsState> {
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
                return Err(JsliteError::runtime(format!(
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
                    return Err(JsliteError::runtime(format!(
                        "SyntaxError: unsupported regular expression flag `{flag}`",
                    )));
                }
            }
        }
        Ok(state)
    }

    fn compile_regexp(&self, pattern: &str, flags: &str) -> JsliteResult<Regex> {
        let flags = self.validate_regexp_flags(flags)?;
        let mut engine_flags = String::new();
        if flags.ignore_case {
            engine_flags.push('i');
        }
        if flags.multiline {
            engine_flags.push('m');
        }
        if flags.dot_all {
            engine_flags.push('s');
        }
        if flags.unicode {
            engine_flags.push('u');
        }
        Regex::with_flags(pattern, engine_flags.as_str()).map_err(|error| {
            JsliteError::runtime(format!("SyntaxError: invalid regular expression: {error}"))
        })
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

    pub(crate) fn date_object(&self, key: ObjectKey) -> JsliteResult<&DateObject> {
        match &self
            .objects
            .get(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .kind
        {
            ObjectKind::Date(date) => Ok(date),
            _ => Err(JsliteError::runtime("date missing")),
        }
    }

    pub(crate) fn regexp_object(&self, key: ObjectKey) -> JsliteResult<&RegExpObject> {
        match &self
            .objects
            .get(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .kind
        {
            ObjectKind::RegExp(regex) => Ok(regex),
            _ => Err(JsliteError::runtime("regexp missing")),
        }
    }

    pub(crate) fn regexp_object_mut(&mut self, key: ObjectKey) -> JsliteResult<&mut RegExpObject> {
        match &mut self
            .objects
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .kind
        {
            ObjectKind::RegExp(regex) => Ok(regex),
            _ => Err(JsliteError::runtime("regexp missing")),
        }
    }

    pub(crate) fn first_regexp_match_from_state(
        &self,
        regex: &RegExpObject,
        text: &str,
        start_index: usize,
    ) -> JsliteResult<Option<RegExpMatchData>> {
        let flags = self.validate_regexp_flags(&regex.flags)?;
        let compiled = self.compile_regexp(&regex.pattern, &regex.flags)?;
        let start_byte = char_index_to_byte_index(text, start_index);
        let matched = compiled.find_from(text, start_byte).next();
        let Some(matched) = matched else {
            return Ok(None);
        };
        if flags.sticky && matched.start() != start_byte {
            return Ok(None);
        }
        let named_groups = matched
            .named_groups()
            .map(|(name, range)| {
                (
                    name.to_string(),
                    range.map(|range| text[range.start..range.end].to_string()),
                )
            })
            .collect::<IndexMap<_, _>>();
        Ok(Some(RegExpMatchData {
            start_byte: matched.start(),
            end_byte: matched.end(),
            start_index: byte_index_to_char_index(text, matched.start()),
            end_index: byte_index_to_char_index(text, matched.end()),
            captures: matched
                .captures
                .iter()
                .map(|range| {
                    range
                        .clone()
                        .map(|range| text[range.start..range.end].to_string())
                })
                .collect(),
            named_groups,
        }))
    }

    fn first_regexp_match(
        &mut self,
        regex_key: ObjectKey,
        text: &str,
    ) -> JsliteResult<Option<RegExpMatchData>> {
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
            self.refresh_object_accounting(regex_key)?;
        }
        Ok(matched)
    }

    pub(crate) fn collect_regexp_matches_from_state(
        &self,
        regex: &RegExpObject,
        text: &str,
        all: bool,
    ) -> JsliteResult<Vec<RegExpMatchData>> {
        let mut matches = Vec::new();
        let mut start_index = 0usize;
        loop {
            let Some(matched) = self.first_regexp_match_from_state(regex, text, start_index)?
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
