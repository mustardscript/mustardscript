use super::*;

impl Runtime {
    pub(crate) fn call_builtin(
        &mut self,
        function: BuiltinFunction,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        match function {
            BuiltinFunction::FunctionCtor => Err(JsliteError::runtime(
                "TypeError: Function constructor is unavailable in the supported surface",
            )),
            BuiltinFunction::FunctionCall
            | BuiltinFunction::FunctionApply
            | BuiltinFunction::FunctionBind => Err(JsliteError::runtime(
                "internal function helper should be dispatched through call semantics",
            )),
            BuiltinFunction::ArrayCtor => self.call_array_ctor(args),
            BuiltinFunction::ArrayFrom => self.call_array_from(args),
            BuiltinFunction::ArrayOf => self.call_array_of(args),
            BuiltinFunction::ArrayIsArray => {
                Ok(Value::Bool(matches!(args.first(), Some(Value::Array(_)))))
            }
            BuiltinFunction::ArrayPush => self.call_array_push(this_value, args),
            BuiltinFunction::ArrayPop => self.call_array_pop(this_value),
            BuiltinFunction::ArraySlice => self.call_array_slice(this_value, args),
            BuiltinFunction::ArraySplice => self.call_array_splice(this_value, args),
            BuiltinFunction::ArrayConcat => self.call_array_concat(this_value, args),
            BuiltinFunction::ArrayAt => self.call_array_at(this_value, args),
            BuiltinFunction::ArrayJoin => self.call_array_join(this_value, args),
            BuiltinFunction::ArrayIncludes => self.call_array_includes(this_value, args),
            BuiltinFunction::ArrayIndexOf => self.call_array_index_of(this_value, args),
            BuiltinFunction::ArrayLastIndexOf => self.call_array_last_index_of(this_value, args),
            BuiltinFunction::ArrayReverse => self.call_array_reverse(this_value),
            BuiltinFunction::ArrayFill => self.call_array_fill(this_value, args),
            BuiltinFunction::ArraySort => self.call_array_sort(this_value, args),
            BuiltinFunction::ArrayValues => self.call_array_values(this_value),
            BuiltinFunction::ArrayKeys => self.call_array_keys(this_value),
            BuiltinFunction::ArrayEntries => self.call_array_entries(this_value),
            BuiltinFunction::ArrayForEach => self.call_array_for_each(this_value, args),
            BuiltinFunction::ArrayMap => self.call_array_map(this_value, args),
            BuiltinFunction::ArrayFilter => self.call_array_filter(this_value, args),
            BuiltinFunction::ArrayFind => self.call_array_find(this_value, args),
            BuiltinFunction::ArrayFindIndex => self.call_array_find_index(this_value, args),
            BuiltinFunction::ArraySome => self.call_array_some(this_value, args),
            BuiltinFunction::ArrayEvery => self.call_array_every(this_value, args),
            BuiltinFunction::ArrayFlat => self.call_array_flat(this_value, args),
            BuiltinFunction::ArrayFlatMap => self.call_array_flat_map(this_value, args),
            BuiltinFunction::ArrayReduce => self.call_array_reduce(this_value, args),
            BuiltinFunction::ArrayReduceRight => self.call_array_reduce_right(this_value, args),
            BuiltinFunction::ArrayFindLast => self.call_array_find_last(this_value, args),
            BuiltinFunction::ArrayFindLastIndex => {
                self.call_array_find_last_index(this_value, args)
            }
            BuiltinFunction::ObjectCtor => self.call_object_ctor(args),
            BuiltinFunction::ObjectAssign => self.call_object_assign(args),
            BuiltinFunction::ObjectCreate => self.reject_object_create(),
            BuiltinFunction::ObjectFreeze => self.reject_object_freeze(),
            BuiltinFunction::ObjectSeal => self.reject_object_seal(),
            BuiltinFunction::ObjectFromEntries => self.call_object_from_entries(args),
            BuiltinFunction::ObjectKeys => self.call_object_keys(args),
            BuiltinFunction::ObjectValues => self.call_object_values(args),
            BuiltinFunction::ObjectEntries => self.call_object_entries(args),
            BuiltinFunction::ObjectHasOwn => self.call_object_has_own(args),
            BuiltinFunction::MapCtor => Err(JsliteError::runtime(
                "TypeError: Map constructor must be called with new",
            )),
            BuiltinFunction::MapGet => self.call_map_get(this_value, args),
            BuiltinFunction::MapSet => self.call_map_set(this_value, args),
            BuiltinFunction::MapHas => self.call_map_has(this_value, args),
            BuiltinFunction::MapDelete => self.call_map_delete(this_value, args),
            BuiltinFunction::MapClear => self.call_map_clear(this_value),
            BuiltinFunction::MapEntries => self.call_map_entries(this_value),
            BuiltinFunction::MapKeys => self.call_map_keys(this_value),
            BuiltinFunction::MapValues => self.call_map_values(this_value),
            BuiltinFunction::MapForEach => self.call_map_for_each(this_value, args),
            BuiltinFunction::SetCtor => Err(JsliteError::runtime(
                "TypeError: Set constructor must be called with new",
            )),
            BuiltinFunction::SetAdd => self.call_set_add(this_value, args),
            BuiltinFunction::SetHas => self.call_set_has(this_value, args),
            BuiltinFunction::SetDelete => self.call_set_delete(this_value, args),
            BuiltinFunction::SetClear => self.call_set_clear(this_value),
            BuiltinFunction::SetEntries => self.call_set_entries(this_value),
            BuiltinFunction::SetKeys => self.call_set_keys(this_value),
            BuiltinFunction::SetValues => self.call_set_values(this_value),
            BuiltinFunction::SetForEach => self.call_set_for_each(this_value, args),
            BuiltinFunction::IteratorNext => self.call_iterator_next(this_value),
            BuiltinFunction::PromiseCtor => Err(JsliteError::runtime(
                "TypeError: Promise constructor must be called with new",
            )),
            BuiltinFunction::PromiseResolve => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                Ok(Value::Promise(self.coerce_to_promise(value)?))
            }
            BuiltinFunction::PromiseReject => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                Ok(Value::Promise(self.insert_promise(
                    PromiseState::Rejected(PromiseRejection {
                        value,
                        span: None,
                        traceback: self.traceback_snapshots(),
                    }),
                )?))
            }
            BuiltinFunction::PromiseResolveFunction(target) => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                self.resolve_promise(target, value)?;
                Ok(Value::Undefined)
            }
            BuiltinFunction::PromiseRejectFunction(target) => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                self.reject_promise(
                    target,
                    PromiseRejection {
                        value,
                        span: None,
                        traceback: self.traceback_snapshots(),
                    },
                )?;
                Ok(Value::Undefined)
            }
            BuiltinFunction::PromiseThen => self.call_promise_then(this_value, args),
            BuiltinFunction::PromiseCatch => self.call_promise_catch(this_value, args),
            BuiltinFunction::PromiseFinally => self.call_promise_finally(this_value, args),
            BuiltinFunction::PromiseAll => self.call_promise_all(args),
            BuiltinFunction::PromiseRace => self.call_promise_race(args),
            BuiltinFunction::PromiseAny => self.call_promise_any(args),
            BuiltinFunction::PromiseAllSettled => self.call_promise_all_settled(args),
            BuiltinFunction::RegExpCtor => self.construct_regexp(args),
            BuiltinFunction::RegExpExec => self.call_regexp_exec(this_value, args),
            BuiltinFunction::RegExpTest => self.call_regexp_test(this_value, args),
            BuiltinFunction::ErrorCtor => self.call_error_ctor(args, "Error"),
            BuiltinFunction::TypeErrorCtor => self.call_error_ctor(args, "TypeError"),
            BuiltinFunction::ReferenceErrorCtor => self.call_error_ctor(args, "ReferenceError"),
            BuiltinFunction::RangeErrorCtor => self.call_error_ctor(args, "RangeError"),
            BuiltinFunction::SyntaxErrorCtor => self.call_error_ctor(args, "SyntaxError"),
            BuiltinFunction::NumberCtor => self.call_number_ctor(args),
            BuiltinFunction::NumberParseInt => self.call_number_parse_int(args),
            BuiltinFunction::NumberParseFloat => self.call_number_parse_float(args),
            BuiltinFunction::NumberIsNaN => Ok(self.call_number_is_nan(args)),
            BuiltinFunction::NumberIsFinite => Ok(self.call_number_is_finite(args)),
            BuiltinFunction::NumberIsInteger => Ok(self.call_number_is_integer(args)),
            BuiltinFunction::NumberIsSafeInteger => Ok(self.call_number_is_safe_integer(args)),
            BuiltinFunction::DateCtor => Err(JsliteError::runtime(
                "TypeError: Date constructor must be called with new",
            )),
            BuiltinFunction::DateNow => Ok(Value::Number(current_time_millis())),
            BuiltinFunction::DateGetTime => self.call_date_get_time(this_value),
            BuiltinFunction::DateValueOf => self.call_date_value_of(this_value),
            BuiltinFunction::DateToISOString => self.call_date_to_iso_string(this_value),
            BuiltinFunction::DateToJSON => self.call_date_to_json(this_value),
            BuiltinFunction::DateGetUTCFullYear => self.call_date_get_utc_full_year(this_value),
            BuiltinFunction::DateGetUTCMonth => self.call_date_get_utc_month(this_value),
            BuiltinFunction::DateGetUTCDate => self.call_date_get_utc_date(this_value),
            BuiltinFunction::DateGetUTCHours => self.call_date_get_utc_hours(this_value),
            BuiltinFunction::DateGetUTCMinutes => self.call_date_get_utc_minutes(this_value),
            BuiltinFunction::DateGetUTCSeconds => self.call_date_get_utc_seconds(this_value),
            BuiltinFunction::IntlDateTimeFormatCtor => self.construct_intl_date_time_format(args),
            BuiltinFunction::IntlNumberFormatCtor => self.construct_intl_number_format(args),
            BuiltinFunction::IntlDateTimeFormatFormat => {
                self.call_intl_date_time_format_format(this_value, args)
            }
            BuiltinFunction::IntlDateTimeFormatResolvedOptions => {
                self.call_intl_date_time_format_resolved_options(this_value)
            }
            BuiltinFunction::IntlNumberFormatFormat => {
                self.call_intl_number_format_format(this_value, args)
            }
            BuiltinFunction::IntlNumberFormatResolvedOptions => {
                self.call_intl_number_format_resolved_options(this_value)
            }
            BuiltinFunction::StringCtor => self.call_string_ctor(args),
            BuiltinFunction::StringTrim => self.call_string_trim(this_value),
            BuiltinFunction::StringTrimStart => self.call_string_trim_start(this_value),
            BuiltinFunction::StringTrimEnd => self.call_string_trim_end(this_value),
            BuiltinFunction::StringIncludes => self.call_string_includes(this_value, args),
            BuiltinFunction::StringStartsWith => self.call_string_starts_with(this_value, args),
            BuiltinFunction::StringEndsWith => self.call_string_ends_with(this_value, args),
            BuiltinFunction::StringIndexOf => self.call_string_index_of(this_value, args),
            BuiltinFunction::StringLastIndexOf => self.call_string_last_index_of(this_value, args),
            BuiltinFunction::StringCharAt => self.call_string_char_at(this_value, args),
            BuiltinFunction::StringAt => self.call_string_at(this_value, args),
            BuiltinFunction::StringSlice => self.call_string_slice(this_value, args),
            BuiltinFunction::StringSubstring => self.call_string_substring(this_value, args),
            BuiltinFunction::StringToLowerCase => self.call_string_to_lower_case(this_value),
            BuiltinFunction::StringToUpperCase => self.call_string_to_upper_case(this_value),
            BuiltinFunction::StringRepeat => self.call_string_repeat(this_value, args),
            BuiltinFunction::StringConcat => self.call_string_concat(this_value, args),
            BuiltinFunction::StringPadStart => self.call_string_pad_start(this_value, args),
            BuiltinFunction::StringPadEnd => self.call_string_pad_end(this_value, args),
            BuiltinFunction::StringSplit => self.call_string_split(this_value, args),
            BuiltinFunction::StringReplace => self.call_string_replace(this_value, args),
            BuiltinFunction::StringReplaceAll => self.call_string_replace_all(this_value, args),
            BuiltinFunction::StringSearch => self.call_string_search(this_value, args),
            BuiltinFunction::StringMatch => self.call_string_match(this_value, args),
            BuiltinFunction::StringMatchAll => self.call_string_match_all(this_value, args),
            BuiltinFunction::StringToString => self.call_string_to_string(this_value),
            BuiltinFunction::StringValueOf => self.call_string_value_of(this_value),
            BuiltinFunction::BooleanCtor => self.call_boolean_ctor(args),
            BuiltinFunction::BooleanToString => self.call_boolean_to_string(this_value),
            BuiltinFunction::BooleanValueOf => self.call_boolean_value_of(this_value),
            BuiltinFunction::NumberToString => self.call_number_to_string(this_value),
            BuiltinFunction::NumberValueOf => self.call_number_value_of(this_value),
            BuiltinFunction::MathAbs => self.call_math_abs(args),
            BuiltinFunction::MathMax => self.call_math_max(args),
            BuiltinFunction::MathMin => self.call_math_min(args),
            BuiltinFunction::MathFloor => self.call_math_floor(args),
            BuiltinFunction::MathCeil => self.call_math_ceil(args),
            BuiltinFunction::MathRound => self.call_math_round(args),
            BuiltinFunction::MathPow => self.call_math_pow(args),
            BuiltinFunction::MathSqrt => self.call_math_sqrt(args),
            BuiltinFunction::MathTrunc => self.call_math_trunc(args),
            BuiltinFunction::MathSign => self.call_math_sign(args),
            BuiltinFunction::MathLog => self.call_math_log(args),
            BuiltinFunction::MathExp => self.call_math_exp(args),
            BuiltinFunction::MathLog2 => self.call_math_log2(args),
            BuiltinFunction::MathLog10 => self.call_math_log10(args),
            BuiltinFunction::MathSin => self.call_math_sin(args),
            BuiltinFunction::MathCos => self.call_math_cos(args),
            BuiltinFunction::MathAtan2 => self.call_math_atan2(args),
            BuiltinFunction::MathHypot => self.call_math_hypot(args),
            BuiltinFunction::MathCbrt => self.call_math_cbrt(args),
            BuiltinFunction::MathRandom => Ok(self.call_math_random()),
            BuiltinFunction::JsonStringify => self.call_json_stringify(args),
            BuiltinFunction::JsonParse => self.call_json_parse(args),
        }
    }

    pub(crate) fn install_builtins(&mut self) -> JsliteResult<()> {
        let global_object = self.insert_object(IndexMap::new(), ObjectKind::Global)?;
        for function in [
            BuiltinFunction::FunctionCtor,
            BuiltinFunction::ObjectCtor,
            BuiltinFunction::MapCtor,
            BuiltinFunction::SetCtor,
            BuiltinFunction::ArrayCtor,
            BuiltinFunction::DateCtor,
            BuiltinFunction::PromiseCtor,
            BuiltinFunction::RegExpCtor,
            BuiltinFunction::StringCtor,
            BuiltinFunction::ErrorCtor,
            BuiltinFunction::TypeErrorCtor,
            BuiltinFunction::ReferenceErrorCtor,
            BuiltinFunction::RangeErrorCtor,
            BuiltinFunction::SyntaxErrorCtor,
            BuiltinFunction::NumberCtor,
            BuiltinFunction::BooleanCtor,
            BuiltinFunction::IntlDateTimeFormatCtor,
            BuiltinFunction::IntlNumberFormatCtor,
        ] {
            self.register_builtin_prototype(function)?;
        }
        self.define_global(
            "globalThis".to_string(),
            Value::Object(global_object),
            false,
        )?;
        self.define_global(
            "Object".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ObjectCtor),
            false,
        )?;
        self.define_global(
            "Map".to_string(),
            Value::BuiltinFunction(BuiltinFunction::MapCtor),
            false,
        )?;
        self.define_global(
            "Set".to_string(),
            Value::BuiltinFunction(BuiltinFunction::SetCtor),
            false,
        )?;
        self.define_global(
            "Array".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor),
            false,
        )?;
        self.define_global(
            "Date".to_string(),
            Value::BuiltinFunction(BuiltinFunction::DateCtor),
            false,
        )?;
        self.define_global(
            "Promise".to_string(),
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor),
            false,
        )?;
        self.define_global(
            "RegExp".to_string(),
            Value::BuiltinFunction(BuiltinFunction::RegExpCtor),
            false,
        )?;
        self.define_global(
            "String".to_string(),
            Value::BuiltinFunction(BuiltinFunction::StringCtor),
            false,
        )?;
        self.define_global(
            "Error".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ErrorCtor),
            false,
        )?;
        self.define_global(
            "TypeError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::TypeErrorCtor),
            false,
        )?;
        self.define_global(
            "ReferenceError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ReferenceErrorCtor),
            false,
        )?;
        self.define_global(
            "RangeError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::RangeErrorCtor),
            false,
        )?;
        self.define_global(
            "SyntaxError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::SyntaxErrorCtor),
            false,
        )?;
        self.define_global(
            "Number".to_string(),
            Value::BuiltinFunction(BuiltinFunction::NumberCtor),
            false,
        )?;
        self.define_global("NaN".to_string(), Value::Number(f64::NAN), false)?;
        self.define_global("Infinity".to_string(), Value::Number(f64::INFINITY), false)?;
        self.define_global(
            "parseInt".to_string(),
            Value::BuiltinFunction(BuiltinFunction::NumberParseInt),
            false,
        )?;
        self.define_global(
            "parseFloat".to_string(),
            Value::BuiltinFunction(BuiltinFunction::NumberParseFloat),
            false,
        )?;
        self.define_global(
            "isNaN".to_string(),
            Value::BuiltinFunction(BuiltinFunction::NumberIsNaN),
            false,
        )?;
        self.define_global(
            "isFinite".to_string(),
            Value::BuiltinFunction(BuiltinFunction::NumberIsFinite),
            false,
        )?;
        self.define_global(
            "Boolean".to_string(),
            Value::BuiltinFunction(BuiltinFunction::BooleanCtor),
            false,
        )?;
        let intl = self.insert_object(
            IndexMap::from([
                (
                    "DateTimeFormat".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::IntlDateTimeFormatCtor),
                ),
                (
                    "NumberFormat".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::IntlNumberFormatCtor),
                ),
            ]),
            ObjectKind::Intl,
        )?;
        self.define_global("Intl".to_string(), Value::Object(intl), false)?;

        let math = self.insert_object(
            IndexMap::from([
                ("E".to_string(), Value::Number(std::f64::consts::E)),
                ("LN2".to_string(), Value::Number(std::f64::consts::LN_2)),
                ("LN10".to_string(), Value::Number(std::f64::consts::LN_10)),
                ("LOG2E".to_string(), Value::Number(std::f64::consts::LOG2_E)),
                (
                    "LOG10E".to_string(),
                    Value::Number(std::f64::consts::LOG10_E),
                ),
                ("PI".to_string(), Value::Number(std::f64::consts::PI)),
                ("SQRT2".to_string(), Value::Number(std::f64::consts::SQRT_2)),
                (
                    "SQRT1_2".to_string(),
                    Value::Number(std::f64::consts::FRAC_1_SQRT_2),
                ),
                (
                    "abs".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathAbs),
                ),
                (
                    "max".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathMax),
                ),
                (
                    "min".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathMin),
                ),
                (
                    "floor".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathFloor),
                ),
                (
                    "ceil".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathCeil),
                ),
                (
                    "round".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathRound),
                ),
                (
                    "pow".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathPow),
                ),
                (
                    "sqrt".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathSqrt),
                ),
                (
                    "trunc".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathTrunc),
                ),
                (
                    "sign".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathSign),
                ),
                (
                    "log".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathLog),
                ),
                (
                    "exp".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathExp),
                ),
                (
                    "log2".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathLog2),
                ),
                (
                    "log10".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathLog10),
                ),
                (
                    "sin".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathSin),
                ),
                (
                    "cos".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathCos),
                ),
                (
                    "atan2".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathAtan2),
                ),
                (
                    "hypot".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathHypot),
                ),
                (
                    "cbrt".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathCbrt),
                ),
                (
                    "random".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathRandom),
                ),
            ]),
            ObjectKind::Math,
        )?;
        self.define_global("Math".to_string(), Value::Object(math), false)?;

        let json = self.insert_object(
            IndexMap::from([
                (
                    "stringify".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonStringify),
                ),
                (
                    "parse".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonParse),
                ),
            ]),
            ObjectKind::Json,
        )?;
        self.define_global("JSON".to_string(), Value::Object(json), false)?;

        let console = self.insert_object(IndexMap::new(), ObjectKind::Console)?;
        self.define_global("console".to_string(), Value::Object(console), false)?;
        Ok(())
    }

    fn register_builtin_prototype(&mut self, function: BuiltinFunction) -> JsliteResult<()> {
        let prototype = self.insert_object(
            IndexMap::new(),
            ObjectKind::FunctionPrototype(Value::BuiltinFunction(function)),
        )?;
        self.builtin_prototypes.insert(function, prototype);
        Ok(())
    }
}
