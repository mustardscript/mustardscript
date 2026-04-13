use super::*;

impl Runtime {
    fn function_helper_method(key: &str) -> Option<Value> {
        match key {
            "call" => Some(Value::BuiltinFunction(BuiltinFunction::FunctionCall)),
            "apply" => Some(Value::BuiltinFunction(BuiltinFunction::FunctionApply)),
            "bind" => Some(Value::BuiltinFunction(BuiltinFunction::FunctionBind)),
            _ => None,
        }
    }

    fn callable_constructor() -> Value {
        Value::BuiltinFunction(BuiltinFunction::FunctionCtor)
    }

    pub(super) fn array_length(&self, array: ArrayKey) -> MustardResult<usize> {
        Ok(self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .len())
    }

    pub(super) fn array_has_index(&self, array: ArrayKey, index: usize) -> MustardResult<bool> {
        Ok(self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .get(index)
            .is_some_and(Option::is_some))
    }

    pub(super) fn array_value_at(&self, array: ArrayKey, index: usize) -> MustardResult<Value> {
        Ok(self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .get(index)
            .cloned()
            .flatten()
            .unwrap_or(Value::Undefined))
    }

    pub(super) fn closure_own_property(
        &self,
        closure: ClosureKey,
        key: &str,
    ) -> MustardResult<Option<Value>> {
        let closure_ref = self
            .closures
            .get(closure)
            .ok_or_else(|| MustardError::runtime("closure missing"))?;
        let function = self
            .program
            .functions
            .get(closure_ref.function_id)
            .ok_or_else(|| MustardError::runtime("function not found"))?;
        Ok(match key {
            "name" => Some(Value::String(
                closure_ref
                    .name
                    .clone()
                    .or_else(|| function.name.clone())
                    .unwrap_or_default(),
            )),
            "length" => Some(Value::Number(function.length as f64)),
            "prototype" => closure_ref.prototype.map(Value::Object),
            _ => closure_ref.properties.get(key).cloned(),
        })
    }

    pub(super) fn closure_has_own_property(
        &self,
        closure: ClosureKey,
        key: &str,
    ) -> MustardResult<bool> {
        Ok(self.closure_own_property(closure, key)?.is_some())
    }

    fn set_closure_property(
        &mut self,
        closure: ClosureKey,
        key: String,
        value: Value,
    ) -> MustardResult<()> {
        if matches!(key.as_str(), "name" | "length" | "prototype") {
            return Err(MustardError::runtime(
                "TypeError: cannot assign to read-only function metadata",
            ));
        }
        self.closures
            .get_mut(closure)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .properties
            .insert(key, value);
        self.refresh_closure_accounting(closure)?;
        Ok(())
    }

    pub(super) fn builtin_function_custom_property(
        &self,
        function: BuiltinFunction,
        key: &str,
    ) -> MustardResult<Option<Value>> {
        let Some(object) = self.builtin_function_objects.get(&function).copied() else {
            return Ok(None);
        };
        Ok(self
            .objects
            .get(object)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .properties
            .get(key)
            .cloned())
    }

    pub(super) fn host_function_custom_property(
        &self,
        capability: &str,
        key: &str,
    ) -> MustardResult<Option<Value>> {
        let Some(object) = self.host_function_objects.get(capability).copied() else {
            return Ok(None);
        };
        Ok(self
            .objects
            .get(object)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .properties
            .get(key)
            .cloned())
    }

    pub(super) fn builtin_function_custom_keys(
        &self,
        function: BuiltinFunction,
    ) -> MustardResult<Vec<String>> {
        let Some(object) = self.builtin_function_objects.get(&function).copied() else {
            return Ok(Vec::new());
        };
        Ok(ordered_own_property_keys(
            &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .properties,
        ))
    }

    pub(super) fn host_function_custom_keys(&self, capability: &str) -> MustardResult<Vec<String>> {
        let Some(object) = self.host_function_objects.get(capability).copied() else {
            return Ok(Vec::new());
        };
        Ok(ordered_own_property_keys(
            &self
                .objects
                .get(object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .properties,
        ))
    }

    fn set_builtin_function_property(
        &mut self,
        function: BuiltinFunction,
        key: String,
        value: Value,
    ) -> MustardResult<()> {
        if matches!(key.as_str(), "name" | "length" | "prototype") {
            return Err(MustardError::runtime(
                "TypeError: cannot assign to read-only function metadata",
            ));
        }
        let object = match self.builtin_function_objects.get(&function).copied() {
            Some(object) => object,
            None => {
                let object = self.insert_object(IndexMap::new(), ObjectKind::Plain)?;
                self.builtin_function_objects.insert(function, object);
                object
            }
        };
        self.objects
            .get_mut(object)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .properties
            .insert(key, value);
        self.refresh_object_accounting(object)?;
        Ok(())
    }

    fn set_host_function_property(
        &mut self,
        capability: String,
        key: String,
        value: Value,
    ) -> MustardResult<()> {
        if matches!(key.as_str(), "name" | "length") {
            return Err(MustardError::runtime(
                "TypeError: cannot assign to read-only function metadata",
            ));
        }
        let object = match self.host_function_objects.get(&capability).copied() {
            Some(object) => object,
            None => {
                let object = self.insert_object(IndexMap::new(), ObjectKind::Plain)?;
                self.host_function_objects.insert(capability, object);
                object
            }
        };
        self.objects
            .get_mut(object)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .properties
            .insert(key, value);
        self.refresh_object_accounting(object)?;
        Ok(())
    }

    fn builtin_function_name(function: BuiltinFunction) -> &'static str {
        match function {
            BuiltinFunction::FunctionCtor => "Function",
            BuiltinFunction::FunctionCall => "call",
            BuiltinFunction::FunctionApply => "apply",
            BuiltinFunction::FunctionBind => "bind",
            BuiltinFunction::ArrayCtor => "Array",
            BuiltinFunction::ArrayFrom => "from",
            BuiltinFunction::ArrayOf => "of",
            BuiltinFunction::ArrayIsArray => "isArray",
            BuiltinFunction::ArrayPush => "push",
            BuiltinFunction::ArrayPop => "pop",
            BuiltinFunction::ArraySlice => "slice",
            BuiltinFunction::ArraySplice => "splice",
            BuiltinFunction::ArrayConcat => "concat",
            BuiltinFunction::ArrayAt => "at",
            BuiltinFunction::ArrayJoin => "join",
            BuiltinFunction::ArrayIncludes => "includes",
            BuiltinFunction::ArrayIndexOf => "indexOf",
            BuiltinFunction::ArrayLastIndexOf => "lastIndexOf",
            BuiltinFunction::ArrayReverse => "reverse",
            BuiltinFunction::ArrayFill => "fill",
            BuiltinFunction::ArraySort => "sort",
            BuiltinFunction::ArrayValues => "values",
            BuiltinFunction::ArrayKeys => "keys",
            BuiltinFunction::ArrayEntries => "entries",
            BuiltinFunction::ArrayForEach => "forEach",
            BuiltinFunction::ArrayMap => "map",
            BuiltinFunction::ArrayFilter => "filter",
            BuiltinFunction::ArrayFind => "find",
            BuiltinFunction::ArrayFindIndex => "findIndex",
            BuiltinFunction::ArraySome => "some",
            BuiltinFunction::ArrayEvery => "every",
            BuiltinFunction::ArrayFlat => "flat",
            BuiltinFunction::ArrayFlatMap => "flatMap",
            BuiltinFunction::ArrayReduce => "reduce",
            BuiltinFunction::ArrayReduceRight => "reduceRight",
            BuiltinFunction::ArrayFindLast => "findLast",
            BuiltinFunction::ArrayFindLastIndex => "findLastIndex",
            BuiltinFunction::ObjectCtor => "Object",
            BuiltinFunction::ObjectAssign => "assign",
            BuiltinFunction::ObjectCreate => "create",
            BuiltinFunction::ObjectFreeze => "freeze",
            BuiltinFunction::ObjectSeal => "seal",
            BuiltinFunction::ObjectFromEntries => "fromEntries",
            BuiltinFunction::ObjectKeys => "keys",
            BuiltinFunction::ObjectValues => "values",
            BuiltinFunction::ObjectEntries => "entries",
            BuiltinFunction::ObjectHasOwn => "hasOwn",
            BuiltinFunction::MapCtor => "Map",
            BuiltinFunction::MapGet => "get",
            BuiltinFunction::MapSet => "set",
            BuiltinFunction::MapHas => "has",
            BuiltinFunction::MapDelete => "delete",
            BuiltinFunction::MapClear => "clear",
            BuiltinFunction::MapEntries => "entries",
            BuiltinFunction::MapKeys => "keys",
            BuiltinFunction::MapValues => "values",
            BuiltinFunction::MapForEach => "forEach",
            BuiltinFunction::SetCtor => "Set",
            BuiltinFunction::SetAdd => "add",
            BuiltinFunction::SetHas => "has",
            BuiltinFunction::SetDelete => "delete",
            BuiltinFunction::SetClear => "clear",
            BuiltinFunction::SetEntries => "entries",
            BuiltinFunction::SetKeys => "keys",
            BuiltinFunction::SetValues => "values",
            BuiltinFunction::SetForEach => "forEach",
            BuiltinFunction::IteratorNext => "next",
            BuiltinFunction::PromiseCtor => "Promise",
            BuiltinFunction::PromiseResolve => "resolve",
            BuiltinFunction::PromiseReject => "reject",
            BuiltinFunction::PromiseResolveFunction(_) => "",
            BuiltinFunction::PromiseRejectFunction(_) => "",
            BuiltinFunction::PromiseThen => "then",
            BuiltinFunction::PromiseCatch => "catch",
            BuiltinFunction::PromiseFinally => "finally",
            BuiltinFunction::PromiseAll => "all",
            BuiltinFunction::PromiseRace => "race",
            BuiltinFunction::PromiseAny => "any",
            BuiltinFunction::PromiseAllSettled => "allSettled",
            BuiltinFunction::RegExpCtor => "RegExp",
            BuiltinFunction::RegExpExec => "exec",
            BuiltinFunction::RegExpTest => "test",
            BuiltinFunction::ErrorCtor => "Error",
            BuiltinFunction::TypeErrorCtor => "TypeError",
            BuiltinFunction::ReferenceErrorCtor => "ReferenceError",
            BuiltinFunction::RangeErrorCtor => "RangeError",
            BuiltinFunction::SyntaxErrorCtor => "SyntaxError",
            BuiltinFunction::NumberCtor => "Number",
            BuiltinFunction::NumberParseInt => "parseInt",
            BuiltinFunction::NumberParseFloat => "parseFloat",
            BuiltinFunction::NumberIsNaN => "isNaN",
            BuiltinFunction::NumberIsFinite => "isFinite",
            BuiltinFunction::NumberIsInteger => "isInteger",
            BuiltinFunction::NumberIsSafeInteger => "isSafeInteger",
            BuiltinFunction::DateCtor => "Date",
            BuiltinFunction::DateNow => "now",
            BuiltinFunction::DateGetTime => "getTime",
            BuiltinFunction::DateValueOf => "valueOf",
            BuiltinFunction::DateToISOString => "toISOString",
            BuiltinFunction::DateToJSON => "toJSON",
            BuiltinFunction::DateGetUTCFullYear => "getUTCFullYear",
            BuiltinFunction::DateGetUTCMonth => "getUTCMonth",
            BuiltinFunction::DateGetUTCDate => "getUTCDate",
            BuiltinFunction::DateGetUTCHours => "getUTCHours",
            BuiltinFunction::DateGetUTCMinutes => "getUTCMinutes",
            BuiltinFunction::DateGetUTCSeconds => "getUTCSeconds",
            BuiltinFunction::IntlDateTimeFormatCtor => "DateTimeFormat",
            BuiltinFunction::IntlNumberFormatCtor => "NumberFormat",
            BuiltinFunction::IntlDateTimeFormatFormat => "format",
            BuiltinFunction::IntlDateTimeFormatResolvedOptions => "resolvedOptions",
            BuiltinFunction::IntlNumberFormatFormat => "format",
            BuiltinFunction::IntlNumberFormatResolvedOptions => "resolvedOptions",
            BuiltinFunction::StringCtor => "String",
            BuiltinFunction::StringTrim => "trim",
            BuiltinFunction::StringTrimStart => "trimStart",
            BuiltinFunction::StringTrimEnd => "trimEnd",
            BuiltinFunction::StringIncludes => "includes",
            BuiltinFunction::StringStartsWith => "startsWith",
            BuiltinFunction::StringEndsWith => "endsWith",
            BuiltinFunction::StringIndexOf => "indexOf",
            BuiltinFunction::StringLastIndexOf => "lastIndexOf",
            BuiltinFunction::StringCharAt => "charAt",
            BuiltinFunction::StringAt => "at",
            BuiltinFunction::StringSlice => "slice",
            BuiltinFunction::StringSubstring => "substring",
            BuiltinFunction::StringToLowerCase => "toLowerCase",
            BuiltinFunction::StringToUpperCase => "toUpperCase",
            BuiltinFunction::StringRepeat => "repeat",
            BuiltinFunction::StringConcat => "concat",
            BuiltinFunction::StringPadStart => "padStart",
            BuiltinFunction::StringPadEnd => "padEnd",
            BuiltinFunction::StringSplit => "split",
            BuiltinFunction::StringReplace => "replace",
            BuiltinFunction::StringReplaceAll => "replaceAll",
            BuiltinFunction::StringSearch => "search",
            BuiltinFunction::StringMatch => "match",
            BuiltinFunction::StringMatchAll => "matchAll",
            BuiltinFunction::StringToString => "toString",
            BuiltinFunction::StringValueOf => "valueOf",
            BuiltinFunction::BooleanCtor => "Boolean",
            BuiltinFunction::BooleanToString => "toString",
            BuiltinFunction::BooleanValueOf => "valueOf",
            BuiltinFunction::NumberToString => "toString",
            BuiltinFunction::NumberValueOf => "valueOf",
            BuiltinFunction::MathAbs => "abs",
            BuiltinFunction::MathMax => "max",
            BuiltinFunction::MathMin => "min",
            BuiltinFunction::MathFloor => "floor",
            BuiltinFunction::MathCeil => "ceil",
            BuiltinFunction::MathRound => "round",
            BuiltinFunction::MathPow => "pow",
            BuiltinFunction::MathSqrt => "sqrt",
            BuiltinFunction::MathTrunc => "trunc",
            BuiltinFunction::MathSign => "sign",
            BuiltinFunction::MathLog => "log",
            BuiltinFunction::MathExp => "exp",
            BuiltinFunction::MathLog2 => "log2",
            BuiltinFunction::MathLog10 => "log10",
            BuiltinFunction::MathSin => "sin",
            BuiltinFunction::MathCos => "cos",
            BuiltinFunction::MathAtan2 => "atan2",
            BuiltinFunction::MathHypot => "hypot",
            BuiltinFunction::MathCbrt => "cbrt",
            BuiltinFunction::MathRandom => "random",
            BuiltinFunction::JsonStringify => "stringify",
            BuiltinFunction::JsonParse => "parse",
        }
    }

    fn builtin_function_length(function: BuiltinFunction) -> usize {
        match function {
            BuiltinFunction::FunctionCtor => 1,
            BuiltinFunction::FunctionCall => 1,
            BuiltinFunction::FunctionApply => 2,
            BuiltinFunction::FunctionBind => 1,
            BuiltinFunction::ArrayCtor => 1,
            BuiltinFunction::ArrayFrom => 1,
            BuiltinFunction::ArrayOf => 0,
            BuiltinFunction::ArrayIsArray => 1,
            BuiltinFunction::ArrayPush => 1,
            BuiltinFunction::ArrayPop => 0,
            BuiltinFunction::ArraySlice => 2,
            BuiltinFunction::ArraySplice => 2,
            BuiltinFunction::ArrayConcat => 1,
            BuiltinFunction::ArrayAt => 1,
            BuiltinFunction::ArrayJoin => 1,
            BuiltinFunction::ArrayIncludes => 1,
            BuiltinFunction::ArrayIndexOf => 1,
            BuiltinFunction::ArrayLastIndexOf => 1,
            BuiltinFunction::ArrayReverse => 0,
            BuiltinFunction::ArrayFill => 1,
            BuiltinFunction::ArraySort => 1,
            BuiltinFunction::ArrayValues => 0,
            BuiltinFunction::ArrayKeys => 0,
            BuiltinFunction::ArrayEntries => 0,
            BuiltinFunction::ArrayForEach => 1,
            BuiltinFunction::ArrayMap => 1,
            BuiltinFunction::ArrayFilter => 1,
            BuiltinFunction::ArrayFind => 1,
            BuiltinFunction::ArrayFindIndex => 1,
            BuiltinFunction::ArraySome => 1,
            BuiltinFunction::ArrayEvery => 1,
            BuiltinFunction::ArrayFlat => 0,
            BuiltinFunction::ArrayFlatMap => 1,
            BuiltinFunction::ArrayReduce => 1,
            BuiltinFunction::ArrayReduceRight => 1,
            BuiltinFunction::ArrayFindLast => 1,
            BuiltinFunction::ArrayFindLastIndex => 1,
            BuiltinFunction::ObjectCtor => 1,
            BuiltinFunction::ObjectAssign => 2,
            BuiltinFunction::ObjectCreate => 2,
            BuiltinFunction::ObjectFreeze => 1,
            BuiltinFunction::ObjectSeal => 1,
            BuiltinFunction::ObjectFromEntries => 1,
            BuiltinFunction::ObjectKeys => 1,
            BuiltinFunction::ObjectValues => 1,
            BuiltinFunction::ObjectEntries => 1,
            BuiltinFunction::ObjectHasOwn => 2,
            BuiltinFunction::MapCtor => 0,
            BuiltinFunction::MapGet => 1,
            BuiltinFunction::MapSet => 2,
            BuiltinFunction::MapHas => 1,
            BuiltinFunction::MapDelete => 1,
            BuiltinFunction::MapClear => 0,
            BuiltinFunction::MapEntries => 0,
            BuiltinFunction::MapKeys => 0,
            BuiltinFunction::MapValues => 0,
            BuiltinFunction::MapForEach => 1,
            BuiltinFunction::SetCtor => 0,
            BuiltinFunction::SetAdd => 1,
            BuiltinFunction::SetHas => 1,
            BuiltinFunction::SetDelete => 1,
            BuiltinFunction::SetClear => 0,
            BuiltinFunction::SetEntries => 0,
            BuiltinFunction::SetKeys => 0,
            BuiltinFunction::SetValues => 0,
            BuiltinFunction::SetForEach => 1,
            BuiltinFunction::IteratorNext => 0,
            BuiltinFunction::PromiseCtor => 1,
            BuiltinFunction::PromiseResolve => 1,
            BuiltinFunction::PromiseReject => 1,
            BuiltinFunction::PromiseResolveFunction(_) => 1,
            BuiltinFunction::PromiseRejectFunction(_) => 1,
            BuiltinFunction::PromiseThen => 2,
            BuiltinFunction::PromiseCatch => 1,
            BuiltinFunction::PromiseFinally => 1,
            BuiltinFunction::PromiseAll => 1,
            BuiltinFunction::PromiseRace => 1,
            BuiltinFunction::PromiseAny => 1,
            BuiltinFunction::PromiseAllSettled => 1,
            BuiltinFunction::RegExpCtor => 2,
            BuiltinFunction::RegExpExec => 1,
            BuiltinFunction::RegExpTest => 1,
            BuiltinFunction::ErrorCtor => 1,
            BuiltinFunction::TypeErrorCtor => 1,
            BuiltinFunction::ReferenceErrorCtor => 1,
            BuiltinFunction::RangeErrorCtor => 1,
            BuiltinFunction::SyntaxErrorCtor => 1,
            BuiltinFunction::NumberCtor => 1,
            BuiltinFunction::NumberParseInt => 2,
            BuiltinFunction::NumberParseFloat => 1,
            BuiltinFunction::NumberIsNaN => 1,
            BuiltinFunction::NumberIsFinite => 1,
            BuiltinFunction::NumberIsInteger => 1,
            BuiltinFunction::NumberIsSafeInteger => 1,
            BuiltinFunction::DateCtor => 7,
            BuiltinFunction::DateNow => 0,
            BuiltinFunction::DateGetTime => 0,
            BuiltinFunction::DateValueOf => 0,
            BuiltinFunction::DateToISOString => 0,
            BuiltinFunction::DateToJSON => 0,
            BuiltinFunction::DateGetUTCFullYear => 0,
            BuiltinFunction::DateGetUTCMonth => 0,
            BuiltinFunction::DateGetUTCDate => 0,
            BuiltinFunction::DateGetUTCHours => 0,
            BuiltinFunction::DateGetUTCMinutes => 0,
            BuiltinFunction::DateGetUTCSeconds => 0,
            BuiltinFunction::IntlDateTimeFormatCtor => 0,
            BuiltinFunction::IntlNumberFormatCtor => 0,
            BuiltinFunction::IntlDateTimeFormatFormat => 1,
            BuiltinFunction::IntlDateTimeFormatResolvedOptions => 0,
            BuiltinFunction::IntlNumberFormatFormat => 1,
            BuiltinFunction::IntlNumberFormatResolvedOptions => 0,
            BuiltinFunction::StringCtor => 1,
            BuiltinFunction::StringTrim => 0,
            BuiltinFunction::StringTrimStart => 0,
            BuiltinFunction::StringTrimEnd => 0,
            BuiltinFunction::StringIncludes => 1,
            BuiltinFunction::StringStartsWith => 1,
            BuiltinFunction::StringEndsWith => 1,
            BuiltinFunction::StringIndexOf => 1,
            BuiltinFunction::StringLastIndexOf => 1,
            BuiltinFunction::StringCharAt => 1,
            BuiltinFunction::StringAt => 1,
            BuiltinFunction::StringSlice => 2,
            BuiltinFunction::StringSubstring => 2,
            BuiltinFunction::StringToLowerCase => 0,
            BuiltinFunction::StringToUpperCase => 0,
            BuiltinFunction::StringRepeat => 1,
            BuiltinFunction::StringConcat => 1,
            BuiltinFunction::StringPadStart => 1,
            BuiltinFunction::StringPadEnd => 1,
            BuiltinFunction::StringSplit => 2,
            BuiltinFunction::StringReplace => 2,
            BuiltinFunction::StringReplaceAll => 2,
            BuiltinFunction::StringSearch => 1,
            BuiltinFunction::StringMatch => 1,
            BuiltinFunction::StringMatchAll => 1,
            BuiltinFunction::StringToString => 0,
            BuiltinFunction::StringValueOf => 0,
            BuiltinFunction::BooleanCtor => 1,
            BuiltinFunction::BooleanToString => 0,
            BuiltinFunction::BooleanValueOf => 0,
            BuiltinFunction::NumberToString => 0,
            BuiltinFunction::NumberValueOf => 0,
            BuiltinFunction::MathAbs => 1,
            BuiltinFunction::MathMax => 2,
            BuiltinFunction::MathMin => 2,
            BuiltinFunction::MathFloor => 1,
            BuiltinFunction::MathCeil => 1,
            BuiltinFunction::MathRound => 1,
            BuiltinFunction::MathPow => 2,
            BuiltinFunction::MathSqrt => 1,
            BuiltinFunction::MathTrunc => 1,
            BuiltinFunction::MathSign => 1,
            BuiltinFunction::MathLog => 1,
            BuiltinFunction::MathExp => 1,
            BuiltinFunction::MathLog2 => 1,
            BuiltinFunction::MathLog10 => 1,
            BuiltinFunction::MathSin => 1,
            BuiltinFunction::MathCos => 1,
            BuiltinFunction::MathAtan2 => 2,
            BuiltinFunction::MathHypot => 2,
            BuiltinFunction::MathCbrt => 1,
            BuiltinFunction::MathRandom => 0,
            BuiltinFunction::JsonStringify => 3,
            BuiltinFunction::JsonParse => 2,
        }
    }

    pub(super) fn builtin_function_own_property(
        &self,
        function: BuiltinFunction,
        key: &str,
    ) -> Option<Value> {
        match key {
            "name" => Some(Value::String(
                Self::builtin_function_name(function).to_string(),
            )),
            "length" => Some(Value::Number(Self::builtin_function_length(function) as f64)),
            "prototype" => self
                .builtin_prototypes
                .get(&function)
                .copied()
                .map(Value::Object),
            _ => match function {
                BuiltinFunction::ArrayCtor => match key {
                    "isArray" => Some(Value::BuiltinFunction(BuiltinFunction::ArrayIsArray)),
                    "from" => Some(Value::BuiltinFunction(BuiltinFunction::ArrayFrom)),
                    "of" => Some(Value::BuiltinFunction(BuiltinFunction::ArrayOf)),
                    _ => None,
                },
                BuiltinFunction::ObjectCtor => match key {
                    "assign" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectAssign)),
                    "create" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectCreate)),
                    "freeze" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectFreeze)),
                    "seal" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectSeal)),
                    "fromEntries" => {
                        Some(Value::BuiltinFunction(BuiltinFunction::ObjectFromEntries))
                    }
                    "keys" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectKeys)),
                    "values" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectValues)),
                    "entries" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectEntries)),
                    "hasOwn" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectHasOwn)),
                    _ => None,
                },
                BuiltinFunction::DateCtor if key == "now" => {
                    Some(Value::BuiltinFunction(BuiltinFunction::DateNow))
                }
                BuiltinFunction::NumberCtor => match key {
                    "parseInt" => Some(Value::BuiltinFunction(BuiltinFunction::NumberParseInt)),
                    "parseFloat" => Some(Value::BuiltinFunction(BuiltinFunction::NumberParseFloat)),
                    "isNaN" => Some(Value::BuiltinFunction(BuiltinFunction::NumberIsNaN)),
                    "isFinite" => Some(Value::BuiltinFunction(BuiltinFunction::NumberIsFinite)),
                    "isInteger" => Some(Value::BuiltinFunction(BuiltinFunction::NumberIsInteger)),
                    "isSafeInteger" => {
                        Some(Value::BuiltinFunction(BuiltinFunction::NumberIsSafeInteger))
                    }
                    "MAX_SAFE_INTEGER" => Some(Value::Number(9_007_199_254_740_991.0)),
                    "MIN_SAFE_INTEGER" => Some(Value::Number(-9_007_199_254_740_991.0)),
                    "EPSILON" => Some(Value::Number(f64::EPSILON)),
                    "MAX_VALUE" => Some(Value::Number(f64::MAX)),
                    "MIN_VALUE" => Some(Value::Number(f64::MIN_POSITIVE)),
                    "POSITIVE_INFINITY" => Some(Value::Number(f64::INFINITY)),
                    "NEGATIVE_INFINITY" => Some(Value::Number(f64::NEG_INFINITY)),
                    "NaN" => Some(Value::Number(f64::NAN)),
                    _ => None,
                },
                BuiltinFunction::PromiseCtor => match key {
                    "resolve" => Some(Value::BuiltinFunction(BuiltinFunction::PromiseResolve)),
                    "reject" => Some(Value::BuiltinFunction(BuiltinFunction::PromiseReject)),
                    "all" => Some(Value::BuiltinFunction(BuiltinFunction::PromiseAll)),
                    "race" => Some(Value::BuiltinFunction(BuiltinFunction::PromiseRace)),
                    "any" => Some(Value::BuiltinFunction(BuiltinFunction::PromiseAny)),
                    "allSettled" => {
                        Some(Value::BuiltinFunction(BuiltinFunction::PromiseAllSettled))
                    }
                    _ => None,
                },
                BuiltinFunction::IntlDateTimeFormatCtor if key == "supportedLocalesOf" => None,
                BuiltinFunction::IntlNumberFormatCtor if key == "supportedLocalesOf" => None,
                _ => None,
            },
        }
    }

    pub(super) fn has_property_in_supported_surface(
        &self,
        object: Value,
        property: Value,
    ) -> MustardResult<bool> {
        let key = self.to_property_key(property)?;
        match object {
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| MustardError::runtime("object missing"))?;
                if object.properties.contains_key(&key) {
                    return Ok(true);
                }
                Ok(match &object.kind {
                    ObjectKind::FunctionPrototype(constructor) => {
                        key == "constructor"
                            || matches!(
                                (constructor, key.as_str()),
                                (
                                    Value::BuiltinFunction(BuiltinFunction::DateCtor),
                                    "getTime"
                                        | "valueOf"
                                        | "toISOString"
                                        | "toJSON"
                                        | "getUTCFullYear"
                                        | "getUTCMonth"
                                        | "getUTCDate"
                                        | "getUTCHours"
                                        | "getUTCMinutes"
                                        | "getUTCSeconds"
                                )
                            )
                    }
                    ObjectKind::BoundFunction(_) => {
                        key == "name"
                            || key == "length"
                            || key == "constructor"
                            || matches!(key.as_str(), "call" | "apply" | "bind")
                    }
                    ObjectKind::Date(_) => matches!(
                        key.as_str(),
                        "getTime"
                            | "valueOf"
                            | "toISOString"
                            | "toJSON"
                            | "getUTCFullYear"
                            | "getUTCMonth"
                            | "getUTCDate"
                            | "getUTCHours"
                            | "getUTCMinutes"
                            | "getUTCSeconds"
                    ),
                    ObjectKind::RegExp(_) => matches!(
                        key.as_str(),
                        "source"
                            | "flags"
                            | "global"
                            | "ignoreCase"
                            | "multiline"
                            | "dotAll"
                            | "unicode"
                            | "sticky"
                            | "lastIndex"
                            | "exec"
                            | "test"
                    ),
                    ObjectKind::Intl => matches!(key.as_str(), "DateTimeFormat" | "NumberFormat"),
                    ObjectKind::IntlDateTimeFormat(_) => {
                        matches!(key.as_str(), "constructor" | "format" | "resolvedOptions")
                    }
                    ObjectKind::IntlNumberFormat(_) => {
                        matches!(key.as_str(), "constructor" | "format" | "resolvedOptions")
                    }
                    ObjectKind::StringObject(value) => {
                        key == "constructor"
                            || key == "length"
                            || matches!(
                                key.as_str(),
                                "trim"
                                    | "trimStart"
                                    | "trimEnd"
                                    | "includes"
                                    | "startsWith"
                                    | "endsWith"
                                    | "indexOf"
                                    | "lastIndexOf"
                                    | "charAt"
                                    | "at"
                                    | "slice"
                                    | "substring"
                                    | "toLowerCase"
                                    | "toUpperCase"
                                    | "repeat"
                                    | "concat"
                                    | "padStart"
                                    | "padEnd"
                                    | "split"
                                    | "replace"
                                    | "replaceAll"
                                    | "search"
                                    | "match"
                                    | "matchAll"
                                    | "toString"
                                    | "valueOf"
                            )
                            || array_index_from_property_key(&key)
                                .is_some_and(|index| value.chars().nth(index).is_some())
                    }
                    ObjectKind::NumberObject(_) | ObjectKind::BooleanObject(_) => {
                        matches!(key.as_str(), "constructor" | "toString" | "valueOf")
                    }
                    ObjectKind::Console => self.console_method(&key).is_some(),
                    ObjectKind::Plain
                    | ObjectKind::Global
                    | ObjectKind::Math
                    | ObjectKind::Json
                    | ObjectKind::Error(_) => key == "constructor",
                })
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?;
                Ok(key == "length"
                    || key.parse::<usize>().ok().is_some_and(|index| {
                        array.elements.get(index).is_some_and(Option::is_some)
                    })
                    || array.properties.contains_key(&key)
                    || matches!(
                        key.as_str(),
                        "constructor"
                            | "sort"
                            | "push"
                            | "pop"
                            | "slice"
                            | "splice"
                            | "concat"
                            | "at"
                            | "join"
                            | "includes"
                            | "indexOf"
                            | "lastIndexOf"
                            | "reverse"
                            | "fill"
                            | "values"
                            | "keys"
                            | "entries"
                            | "forEach"
                            | "map"
                            | "filter"
                            | "find"
                            | "findIndex"
                            | "some"
                            | "every"
                            | "flat"
                            | "flatMap"
                            | "reduce"
                            | "reduceRight"
                            | "findLast"
                            | "findLastIndex"
                    ))
            }
            Value::Map(map) => {
                self.maps
                    .get(map)
                    .ok_or_else(|| MustardError::runtime("map missing"))?;
                Ok(matches!(
                    key.as_str(),
                    "constructor"
                        | "size"
                        | "get"
                        | "set"
                        | "has"
                        | "delete"
                        | "clear"
                        | "entries"
                        | "keys"
                        | "values"
                        | "forEach"
                ))
            }
            Value::Set(set) => {
                self.sets
                    .get(set)
                    .ok_or_else(|| MustardError::runtime("set missing"))?;
                Ok(matches!(
                    key.as_str(),
                    "constructor"
                        | "size"
                        | "add"
                        | "has"
                        | "delete"
                        | "clear"
                        | "entries"
                        | "keys"
                        | "values"
                        | "forEach"
                ))
            }
            Value::Iterator(iterator) => {
                self.iterators
                    .get(iterator)
                    .ok_or_else(|| MustardError::runtime("iterator missing"))?;
                Ok(matches!(key.as_str(), "constructor" | "next"))
            }
            Value::Promise(promise) => {
                self.promises
                    .get(promise)
                    .ok_or_else(|| MustardError::runtime("promise missing"))?;
                Ok(matches!(
                    key.as_str(),
                    "constructor" | "then" | "catch" | "finally"
                ))
            }
            Value::BuiltinFunction(function) => Ok(self
                .builtin_function_custom_property(function, &key)?
                .is_some()
                || self.builtin_function_own_property(function, &key).is_some()
                || key == "constructor"
                || Self::function_helper_method(&key).is_some()),
            Value::Closure(closure) => Ok(self.closure_has_own_property(closure, &key)?
                || key == "constructor"
                || Self::function_helper_method(&key).is_some()),
            Value::HostFunction(capability) => Ok(self
                .host_function_custom_property(&capability, &key)?
                .is_some()
                || matches!(key.as_str(), "name" | "length" | "constructor")
                || Self::function_helper_method(&key).is_some()),
            Value::Undefined
            | Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::BigInt(_) => Err(MustardError::runtime(
                "TypeError: right-hand side of 'in' must be an object in the supported surface",
            )),
        }
    }

    pub(super) fn create_iterator(&mut self, iterable: Value) -> MustardResult<Value> {
        let iterator = match iterable {
            Value::Array(array) => {
                self.insert_iterator(IteratorState::Array(ArrayIteratorState {
                    array,
                    next_index: 0,
                }))?
            }
            Value::String(value) => {
                self.insert_iterator(IteratorState::String(StringIteratorState {
                    value,
                    next_index: 0,
                }))?
            }
            Value::Map(map) => {
                self.insert_iterator(IteratorState::MapEntries(MapIteratorState {
                    map,
                    next_index: 0,
                }))?
            }
            Value::Set(set) => {
                self.insert_iterator(IteratorState::SetValues(SetIteratorState {
                    set,
                    next_index: 0,
                }))?
            }
            Value::Iterator(iterator) => iterator,
            _ => {
                return Err(MustardError::runtime(
                    "TypeError: value is not iterable in the supported surface",
                ));
            }
        };
        Ok(Value::Iterator(iterator))
    }

    pub(super) fn iterator_next(&mut self, iterator: Value) -> MustardResult<(Value, bool)> {
        let key = match iterator {
            Value::Iterator(key) => key,
            _ => return Err(MustardError::runtime("value is not an iterator")),
        };

        let state = self
            .iterators
            .get(key)
            .ok_or_else(|| MustardError::runtime("iterator missing"))?
            .state
            .clone();

        let value = match state {
            IteratorState::Array(state) => self
                .arrays
                .get(state.array)
                .ok_or_else(|| MustardError::runtime("array missing"))?
                .elements
                .get(state.next_index)
                .map(|value| value.clone().unwrap_or(Value::Undefined)),
            IteratorState::ArrayKeys(state) => self
                .arrays
                .get(state.array)
                .ok_or_else(|| MustardError::runtime("array missing"))?
                .elements
                .get(state.next_index)
                .map(|_| Value::Number(state.next_index as f64)),
            IteratorState::ArrayEntries(state) => {
                let value = self
                    .arrays
                    .get(state.array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .get(state.next_index)
                    .map(|value| value.clone().unwrap_or(Value::Undefined));
                match value {
                    Some(value) => Some(Value::Array(self.insert_array(
                        vec![Value::Number(state.next_index as f64), value],
                        IndexMap::new(),
                    )?)),
                    None => None,
                }
            }
            IteratorState::String(state) => {
                let chars = state.value.chars().collect::<Vec<_>>();
                chars
                    .get(state.next_index)
                    .map(|ch| Value::String(ch.to_string()))
            }
            IteratorState::MapEntries(state) => {
                let entry = self
                    .maps
                    .get(state.map)
                    .ok_or_else(|| MustardError::runtime("map missing"))?
                    .entries
                    .get(state.next_index)
                    .cloned();
                match entry {
                    Some(entry) => Some(Value::Array(
                        self.insert_array(vec![entry.key, entry.value], IndexMap::new())?,
                    )),
                    None => None,
                }
            }
            IteratorState::MapKeys(state) => self
                .maps
                .get(state.map)
                .ok_or_else(|| MustardError::runtime("map missing"))?
                .entries
                .get(state.next_index)
                .map(|entry| entry.key.clone()),
            IteratorState::MapValues(state) => self
                .maps
                .get(state.map)
                .ok_or_else(|| MustardError::runtime("map missing"))?
                .entries
                .get(state.next_index)
                .map(|entry| entry.value.clone()),
            IteratorState::SetEntries(state) => self
                .sets
                .get(state.set)
                .ok_or_else(|| MustardError::runtime("set missing"))?
                .entries
                .get(state.next_index)
                .cloned()
                .map(|value| {
                    self.insert_array(vec![value.clone(), value], IndexMap::new())
                        .map(Value::Array)
                })
                .transpose()?,
            IteratorState::SetValues(state) => self
                .sets
                .get(state.set)
                .ok_or_else(|| MustardError::runtime("set missing"))?
                .entries
                .get(state.next_index)
                .cloned(),
        };

        if value.is_some() {
            if let Some(iterator) = self.iterators.get_mut(key) {
                match &mut iterator.state {
                    IteratorState::Array(state)
                    | IteratorState::ArrayKeys(state)
                    | IteratorState::ArrayEntries(state) => state.next_index += 1,
                    IteratorState::String(state) => state.next_index += 1,
                    IteratorState::MapEntries(state)
                    | IteratorState::MapKeys(state)
                    | IteratorState::MapValues(state) => state.next_index += 1,
                    IteratorState::SetEntries(state) | IteratorState::SetValues(state) => {
                        state.next_index += 1
                    }
                }
            }
            self.refresh_iterator_accounting(key)?;
        }

        Ok(match value {
            Some(value) => (value, false),
            None => (Value::Undefined, true),
        })
    }

    pub(super) fn get_property(
        &self,
        object: Value,
        property: Value,
        optional: bool,
    ) -> MustardResult<Value> {
        let key = self.to_property_key(property)?;
        self.get_property_by_key(object, &key, optional)
    }

    pub(super) fn get_property_static(
        &self,
        object: Value,
        key: &str,
        optional: bool,
    ) -> MustardResult<Value> {
        self.get_property_by_key(object, key, optional)
    }

    fn get_property_by_key(
        &self,
        object: Value,
        key: &str,
        optional: bool,
    ) -> MustardResult<Value> {
        if optional && matches!(object, Value::Null | Value::Undefined) {
            return Ok(Value::Undefined);
        }
        match object {
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| MustardError::runtime("object missing"))?;
                if let ObjectKind::Date(_) = &object.kind {
                    let built_in = match key {
                        "getTime" => Some(Value::BuiltinFunction(BuiltinFunction::DateGetTime)),
                        "valueOf" => Some(Value::BuiltinFunction(BuiltinFunction::DateValueOf)),
                        "toISOString" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::DateToISOString))
                        }
                        "toJSON" => Some(Value::BuiltinFunction(BuiltinFunction::DateToJSON)),
                        "getUTCFullYear" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::DateGetUTCFullYear))
                        }
                        "getUTCMonth" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::DateGetUTCMonth))
                        }
                        "getUTCDate" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::DateGetUTCDate))
                        }
                        "getUTCHours" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::DateGetUTCHours))
                        }
                        "getUTCMinutes" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::DateGetUTCMinutes))
                        }
                        "getUTCSeconds" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::DateGetUTCSeconds))
                        }
                        "constructor" => Some(Value::BuiltinFunction(BuiltinFunction::DateCtor)),
                        _ => None,
                    };
                    if let Some(value) = built_in {
                        return Ok(value);
                    }
                }
                if let ObjectKind::RegExp(regex) = &object.kind {
                    let built_in = match key {
                        "source" => Some(Value::String(regex.pattern.clone())),
                        "flags" => Some(Value::String(regex.flags.clone())),
                        "global" => Some(Value::Bool(regex.flags.contains('g'))),
                        "ignoreCase" => Some(Value::Bool(regex.flags.contains('i'))),
                        "multiline" => Some(Value::Bool(regex.flags.contains('m'))),
                        "dotAll" => Some(Value::Bool(regex.flags.contains('s'))),
                        "unicode" => Some(Value::Bool(regex.flags.contains('u'))),
                        "sticky" => Some(Value::Bool(regex.flags.contains('y'))),
                        "lastIndex" => Some(Value::Number(regex.last_index as f64)),
                        "exec" => Some(Value::BuiltinFunction(BuiltinFunction::RegExpExec)),
                        "test" => Some(Value::BuiltinFunction(BuiltinFunction::RegExpTest)),
                        "constructor" => Some(Value::BuiltinFunction(BuiltinFunction::RegExpCtor)),
                        _ => None,
                    };
                    if let Some(value) = built_in {
                        return Ok(value);
                    }
                }
                if let ObjectKind::StringObject(value) = &object.kind {
                    if key == "length" {
                        return Ok(Value::Number(value.chars().count() as f64));
                    }
                    if let Some(index) = array_index_from_property_key(key)
                        && let Some(ch) = value.chars().nth(index)
                    {
                        return Ok(Value::String(ch.to_string()));
                    }
                    if let Some(method) = match key {
                        "trim" => Some(Value::BuiltinFunction(BuiltinFunction::StringTrim)),
                        "trimStart" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::StringTrimStart))
                        }
                        "trimEnd" => Some(Value::BuiltinFunction(BuiltinFunction::StringTrimEnd)),
                        "includes" => Some(Value::BuiltinFunction(BuiltinFunction::StringIncludes)),
                        "startsWith" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::StringStartsWith))
                        }
                        "endsWith" => Some(Value::BuiltinFunction(BuiltinFunction::StringEndsWith)),
                        "indexOf" => Some(Value::BuiltinFunction(BuiltinFunction::StringIndexOf)),
                        "lastIndexOf" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::StringLastIndexOf))
                        }
                        "charAt" => Some(Value::BuiltinFunction(BuiltinFunction::StringCharAt)),
                        "at" => Some(Value::BuiltinFunction(BuiltinFunction::StringAt)),
                        "slice" => Some(Value::BuiltinFunction(BuiltinFunction::StringSlice)),
                        "substring" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::StringSubstring))
                        }
                        "toLowerCase" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::StringToLowerCase))
                        }
                        "toUpperCase" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::StringToUpperCase))
                        }
                        "repeat" => Some(Value::BuiltinFunction(BuiltinFunction::StringRepeat)),
                        "concat" => Some(Value::BuiltinFunction(BuiltinFunction::StringConcat)),
                        "padStart" => Some(Value::BuiltinFunction(BuiltinFunction::StringPadStart)),
                        "padEnd" => Some(Value::BuiltinFunction(BuiltinFunction::StringPadEnd)),
                        "split" => Some(Value::BuiltinFunction(BuiltinFunction::StringSplit)),
                        "replace" => Some(Value::BuiltinFunction(BuiltinFunction::StringReplace)),
                        "replaceAll" => {
                            Some(Value::BuiltinFunction(BuiltinFunction::StringReplaceAll))
                        }
                        "search" => Some(Value::BuiltinFunction(BuiltinFunction::StringSearch)),
                        "match" => Some(Value::BuiltinFunction(BuiltinFunction::StringMatch)),
                        "matchAll" => Some(Value::BuiltinFunction(BuiltinFunction::StringMatchAll)),
                        "toString" => Some(Value::BuiltinFunction(BuiltinFunction::StringToString)),
                        "valueOf" => Some(Value::BuiltinFunction(BuiltinFunction::StringValueOf)),
                        _ => None,
                    } {
                        return Ok(method);
                    }
                }
                if let ObjectKind::NumberObject(_) = &object.kind {
                    match key {
                        "toString" => {
                            return Ok(Value::BuiltinFunction(BuiltinFunction::NumberToString));
                        }
                        "valueOf" => {
                            return Ok(Value::BuiltinFunction(BuiltinFunction::NumberValueOf));
                        }
                        _ => {}
                    }
                }
                if let ObjectKind::BooleanObject(_) = &object.kind {
                    match key {
                        "toString" => {
                            return Ok(Value::BuiltinFunction(BuiltinFunction::BooleanToString));
                        }
                        "valueOf" => {
                            return Ok(Value::BuiltinFunction(BuiltinFunction::BooleanValueOf));
                        }
                        _ => {}
                    }
                }
                if matches!(object.kind, ObjectKind::Intl) {
                    let built_in = match key {
                        "DateTimeFormat" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlDateTimeFormatCtor,
                        )),
                        "NumberFormat" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlNumberFormatCtor,
                        )),
                        "constructor" => Some(Value::BuiltinFunction(BuiltinFunction::ObjectCtor)),
                        _ => None,
                    };
                    if let Some(value) = built_in {
                        return Ok(value);
                    }
                }
                if matches!(object.kind, ObjectKind::IntlDateTimeFormat(_)) {
                    let built_in = match key {
                        "constructor" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlDateTimeFormatCtor,
                        )),
                        "format" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlDateTimeFormatFormat,
                        )),
                        "resolvedOptions" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlDateTimeFormatResolvedOptions,
                        )),
                        _ => None,
                    };
                    if let Some(value) = built_in {
                        return Ok(value);
                    }
                }
                if matches!(object.kind, ObjectKind::IntlNumberFormat(_)) {
                    let built_in = match key {
                        "constructor" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlNumberFormatCtor,
                        )),
                        "format" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlNumberFormatFormat,
                        )),
                        "resolvedOptions" => Some(Value::BuiltinFunction(
                            BuiltinFunction::IntlNumberFormatResolvedOptions,
                        )),
                        _ => None,
                    };
                    if let Some(value) = built_in {
                        return Ok(value);
                    }
                }
                if let Some(value) = object.properties.get(key) {
                    return Ok(value.clone());
                }
                if let ObjectKind::FunctionPrototype(constructor) = &object.kind {
                    if key == "constructor" {
                        return Ok(constructor.clone());
                    }
                    if matches!(
                        constructor,
                        Value::BuiltinFunction(BuiltinFunction::DateCtor)
                    ) {
                        match key {
                            "getTime" => {
                                return Ok(Value::BuiltinFunction(BuiltinFunction::DateGetTime));
                            }
                            "valueOf" => {
                                return Ok(Value::BuiltinFunction(BuiltinFunction::DateValueOf));
                            }
                            "toISOString" => {
                                return Ok(Value::BuiltinFunction(
                                    BuiltinFunction::DateToISOString,
                                ));
                            }
                            "toJSON" => {
                                return Ok(Value::BuiltinFunction(BuiltinFunction::DateToJSON));
                            }
                            "getUTCFullYear" => {
                                return Ok(Value::BuiltinFunction(
                                    BuiltinFunction::DateGetUTCFullYear,
                                ));
                            }
                            "getUTCMonth" => {
                                return Ok(Value::BuiltinFunction(
                                    BuiltinFunction::DateGetUTCMonth,
                                ));
                            }
                            "getUTCDate" => {
                                return Ok(Value::BuiltinFunction(BuiltinFunction::DateGetUTCDate));
                            }
                            "getUTCHours" => {
                                return Ok(Value::BuiltinFunction(
                                    BuiltinFunction::DateGetUTCHours,
                                ));
                            }
                            "getUTCMinutes" => {
                                return Ok(Value::BuiltinFunction(
                                    BuiltinFunction::DateGetUTCMinutes,
                                ));
                            }
                            "getUTCSeconds" => {
                                return Ok(Value::BuiltinFunction(
                                    BuiltinFunction::DateGetUTCSeconds,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                if let ObjectKind::BoundFunction(bound) = &object.kind {
                    match key {
                        "name" => {
                            return Ok(Value::String(format!(
                                "bound {}",
                                self.callable_name(&bound.target)?
                            )));
                        }
                        "length" => {
                            let length = self
                                .callable_length(&bound.target)?
                                .saturating_sub(bound.args.len());
                            return Ok(Value::Number(length as f64));
                        }
                        "constructor" => return Ok(Self::callable_constructor()),
                        _ => {
                            if let Some(method) = Self::function_helper_method(key) {
                                return Ok(method);
                            }
                        }
                    }
                }
                if let ObjectKind::StringObject(_) = &object.kind
                    && key == "constructor"
                {
                    return Ok(Value::BuiltinFunction(BuiltinFunction::StringCtor));
                }
                if let ObjectKind::NumberObject(_) = &object.kind
                    && key == "constructor"
                {
                    return Ok(Value::BuiltinFunction(BuiltinFunction::NumberCtor));
                }
                if let ObjectKind::BooleanObject(_) = &object.kind
                    && key == "constructor"
                {
                    return Ok(Value::BuiltinFunction(BuiltinFunction::BooleanCtor));
                }
                if matches!(object.kind, ObjectKind::Console)
                    && let Some(value) = self.console_method(key)
                {
                    return Ok(value);
                }
                if key == "constructor" {
                    match &object.kind {
                        ObjectKind::Plain
                        | ObjectKind::Global
                        | ObjectKind::Math
                        | ObjectKind::Json
                        | ObjectKind::Intl
                        | ObjectKind::BoundFunction(_) => {
                            return Ok(Value::BuiltinFunction(BuiltinFunction::ObjectCtor));
                        }
                        ObjectKind::Error(name) => {
                            let ctor = match name.as_str() {
                                "TypeError" => BuiltinFunction::TypeErrorCtor,
                                "ReferenceError" => BuiltinFunction::ReferenceErrorCtor,
                                "RangeError" => BuiltinFunction::RangeErrorCtor,
                                "SyntaxError" => BuiltinFunction::SyntaxErrorCtor,
                                _ => BuiltinFunction::ErrorCtor,
                            };
                            return Ok(Value::BuiltinFunction(ctor));
                        }
                        _ => {}
                    }
                }
                Ok(Value::Undefined)
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?;
                if key == "length" {
                    Ok(Value::Number(array.elements.len() as f64))
                } else if key == "constructor" {
                    Ok(Value::BuiltinFunction(BuiltinFunction::ArrayCtor))
                } else if let Some(index) = array_index_from_property_key(key) {
                    Ok(array
                        .elements
                        .get(index)
                        .cloned()
                        .flatten()
                        .unwrap_or(Value::Undefined))
                } else if let Some(value) = array.properties.get(key) {
                    Ok(value.clone())
                } else {
                    match key {
                        "sort" => Ok(Value::BuiltinFunction(BuiltinFunction::ArraySort)),
                        "push" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayPush)),
                        "pop" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayPop)),
                        "slice" => Ok(Value::BuiltinFunction(BuiltinFunction::ArraySlice)),
                        "splice" => Ok(Value::BuiltinFunction(BuiltinFunction::ArraySplice)),
                        "concat" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayConcat)),
                        "at" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayAt)),
                        "join" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayJoin)),
                        "includes" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIncludes)),
                        "indexOf" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIndexOf)),
                        "lastIndexOf" => {
                            Ok(Value::BuiltinFunction(BuiltinFunction::ArrayLastIndexOf))
                        }
                        "reverse" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayReverse)),
                        "fill" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFill)),
                        "values" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayValues)),
                        "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayKeys)),
                        "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayEntries)),
                        "forEach" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayForEach)),
                        "map" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayMap)),
                        "filter" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFilter)),
                        "find" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFind)),
                        "findIndex" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFindIndex)),
                        "some" => Ok(Value::BuiltinFunction(BuiltinFunction::ArraySome)),
                        "every" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayEvery)),
                        "flat" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFlat)),
                        "flatMap" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFlatMap)),
                        "reduce" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayReduce)),
                        "reduceRight" => {
                            Ok(Value::BuiltinFunction(BuiltinFunction::ArrayReduceRight))
                        }
                        "findLast" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFindLast)),
                        "findLastIndex" => {
                            Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFindLastIndex))
                        }
                        _ => Ok(Value::Undefined),
                    }
                }
            }
            Value::Map(map) => {
                let map = self
                    .maps
                    .get(map)
                    .ok_or_else(|| MustardError::runtime("map missing"))?;
                match key {
                    "constructor" => Ok(Value::BuiltinFunction(BuiltinFunction::MapCtor)),
                    "size" => Ok(Value::Number(map.entries.len() as f64)),
                    "get" => Ok(Value::BuiltinFunction(BuiltinFunction::MapGet)),
                    "set" => Ok(Value::BuiltinFunction(BuiltinFunction::MapSet)),
                    "has" => Ok(Value::BuiltinFunction(BuiltinFunction::MapHas)),
                    "delete" => Ok(Value::BuiltinFunction(BuiltinFunction::MapDelete)),
                    "clear" => Ok(Value::BuiltinFunction(BuiltinFunction::MapClear)),
                    "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::MapEntries)),
                    "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::MapKeys)),
                    "values" => Ok(Value::BuiltinFunction(BuiltinFunction::MapValues)),
                    "forEach" => Ok(Value::BuiltinFunction(BuiltinFunction::MapForEach)),
                    _ => Ok(Value::Undefined),
                }
            }
            Value::Set(set) => {
                let set = self
                    .sets
                    .get(set)
                    .ok_or_else(|| MustardError::runtime("set missing"))?;
                match key {
                    "constructor" => Ok(Value::BuiltinFunction(BuiltinFunction::SetCtor)),
                    "size" => Ok(Value::Number(set.entries.len() as f64)),
                    "add" => Ok(Value::BuiltinFunction(BuiltinFunction::SetAdd)),
                    "has" => Ok(Value::BuiltinFunction(BuiltinFunction::SetHas)),
                    "delete" => Ok(Value::BuiltinFunction(BuiltinFunction::SetDelete)),
                    "clear" => Ok(Value::BuiltinFunction(BuiltinFunction::SetClear)),
                    "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::SetEntries)),
                    "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::SetKeys)),
                    "values" => Ok(Value::BuiltinFunction(BuiltinFunction::SetValues)),
                    "forEach" => Ok(Value::BuiltinFunction(BuiltinFunction::SetForEach)),
                    _ => Ok(Value::Undefined),
                }
            }
            Value::Iterator(_) if key == "next" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::IteratorNext))
            }
            Value::Iterator(_) if key == "constructor" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::ObjectCtor))
            }
            Value::Iterator(_) => Ok(Value::Undefined),
            Value::Promise(_) => match key {
                "constructor" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseCtor)),
                "then" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseThen)),
                "catch" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseCatch)),
                "finally" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseFinally)),
                _ => Ok(Value::Undefined),
            },
            Value::BuiltinFunction(function) => {
                if let Some(value) = self.builtin_function_custom_property(function, key)? {
                    return Ok(value);
                }
                if let Some(value) = self.builtin_function_own_property(function, key) {
                    return Ok(value);
                }
                if key == "constructor" {
                    return Ok(Self::callable_constructor());
                }
                Ok(Self::function_helper_method(key).unwrap_or(Value::Undefined))
            }
            Value::Closure(closure) => {
                if let Some(value) = self.closure_own_property(closure, key)? {
                    return Ok(value);
                }
                if key == "constructor" {
                    return Ok(Self::callable_constructor());
                }
                Ok(Self::function_helper_method(key).unwrap_or(Value::Undefined))
            }
            Value::HostFunction(capability) => {
                if let Some(value) = self.host_function_custom_property(&capability, key)? {
                    return Ok(value);
                }
                match key {
                    "name" => Ok(Value::String(capability)),
                    "length" => Ok(Value::Number(0.0)),
                    "constructor" => Ok(Self::callable_constructor()),
                    _ => Ok(Self::function_helper_method(key).unwrap_or(Value::Undefined)),
                }
            }
            Value::String(value) => match key {
                "length" => Ok(Value::Number(value.chars().count() as f64)),
                "constructor" => Ok(Value::BuiltinFunction(BuiltinFunction::StringCtor)),
                "trim" => Ok(Value::BuiltinFunction(BuiltinFunction::StringTrim)),
                "trimStart" => Ok(Value::BuiltinFunction(BuiltinFunction::StringTrimStart)),
                "trimEnd" => Ok(Value::BuiltinFunction(BuiltinFunction::StringTrimEnd)),
                "includes" => Ok(Value::BuiltinFunction(BuiltinFunction::StringIncludes)),
                "startsWith" => Ok(Value::BuiltinFunction(BuiltinFunction::StringStartsWith)),
                "endsWith" => Ok(Value::BuiltinFunction(BuiltinFunction::StringEndsWith)),
                "indexOf" => Ok(Value::BuiltinFunction(BuiltinFunction::StringIndexOf)),
                "lastIndexOf" => Ok(Value::BuiltinFunction(BuiltinFunction::StringLastIndexOf)),
                "charAt" => Ok(Value::BuiltinFunction(BuiltinFunction::StringCharAt)),
                "at" => Ok(Value::BuiltinFunction(BuiltinFunction::StringAt)),
                "slice" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSlice)),
                "substring" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSubstring)),
                "toLowerCase" => Ok(Value::BuiltinFunction(BuiltinFunction::StringToLowerCase)),
                "toUpperCase" => Ok(Value::BuiltinFunction(BuiltinFunction::StringToUpperCase)),
                "repeat" => Ok(Value::BuiltinFunction(BuiltinFunction::StringRepeat)),
                "concat" => Ok(Value::BuiltinFunction(BuiltinFunction::StringConcat)),
                "padStart" => Ok(Value::BuiltinFunction(BuiltinFunction::StringPadStart)),
                "padEnd" => Ok(Value::BuiltinFunction(BuiltinFunction::StringPadEnd)),
                "split" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSplit)),
                "replace" => Ok(Value::BuiltinFunction(BuiltinFunction::StringReplace)),
                "replaceAll" => Ok(Value::BuiltinFunction(BuiltinFunction::StringReplaceAll)),
                "search" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSearch)),
                "match" => Ok(Value::BuiltinFunction(BuiltinFunction::StringMatch)),
                "matchAll" => Ok(Value::BuiltinFunction(BuiltinFunction::StringMatchAll)),
                "toString" => Ok(Value::BuiltinFunction(BuiltinFunction::StringToString)),
                "valueOf" => Ok(Value::BuiltinFunction(BuiltinFunction::StringValueOf)),
                _ if array_index_from_property_key(key)
                    .is_some_and(|index| value.chars().nth(index).is_some()) =>
                {
                    let index = array_index_from_property_key(key).expect("index already checked");
                    Ok(Value::String(
                        value
                            .chars()
                            .nth(index)
                            .expect("index already checked")
                            .to_string(),
                    ))
                }
                _ => Ok(Value::Undefined),
            },
            Value::Number(_) => match key {
                "constructor" => Ok(Value::BuiltinFunction(BuiltinFunction::NumberCtor)),
                "toString" => Ok(Value::BuiltinFunction(BuiltinFunction::NumberToString)),
                "valueOf" => Ok(Value::BuiltinFunction(BuiltinFunction::NumberValueOf)),
                _ => Ok(Value::Undefined),
            },
            Value::Bool(_) => match key {
                "constructor" => Ok(Value::BuiltinFunction(BuiltinFunction::BooleanCtor)),
                "toString" => Ok(Value::BuiltinFunction(BuiltinFunction::BooleanToString)),
                "valueOf" => Ok(Value::BuiltinFunction(BuiltinFunction::BooleanValueOf)),
                _ => Ok(Value::Undefined),
            },
            Value::Null | Value::Undefined => Err(MustardError::runtime(
                "TypeError: cannot read properties of nullish value",
            )),
            _ => Ok(Value::Undefined),
        }
    }

    pub(super) fn callable_name(&self, value: &Value) -> MustardResult<String> {
        Ok(match value {
            Value::Closure(closure) => match self.closure_own_property(*closure, "name")? {
                Some(Value::String(name)) => name,
                _ => String::new(),
            },
            Value::BuiltinFunction(function) => Self::builtin_function_name(*function).to_string(),
            Value::HostFunction(capability) => capability.clone(),
            Value::Object(object) => match &self
                .objects
                .get(*object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::BoundFunction(bound) => {
                    format!("bound {}", self.callable_name(&bound.target)?)
                }
                _ => String::new(),
            },
            _ => String::new(),
        })
    }

    fn callable_length(&self, value: &Value) -> MustardResult<usize> {
        Ok(match value {
            Value::Closure(closure) => match self.closure_own_property(*closure, "length")? {
                Some(Value::Number(length)) => length.max(0.0) as usize,
                _ => 0,
            },
            Value::BuiltinFunction(function) => Self::builtin_function_length(*function),
            Value::HostFunction(_) => 0,
            Value::Object(object) => match &self
                .objects
                .get(*object)
                .ok_or_else(|| MustardError::runtime("object missing"))?
                .kind
            {
                ObjectKind::BoundFunction(bound) => self
                    .callable_length(&bound.target)?
                    .saturating_sub(bound.args.len()),
                _ => 0,
            },
            _ => 0,
        })
    }

    pub(super) fn set_property(
        &mut self,
        object: Value,
        property: Value,
        value: Value,
    ) -> MustardResult<()> {
        let key = self.to_property_key(property)?;
        self.set_property_by_key(object, &key, value)
    }

    pub(super) fn set_property_static(
        &mut self,
        object: Value,
        key: &str,
        value: Value,
    ) -> MustardResult<()> {
        self.set_property_by_key(object, key, value)
    }

    fn set_property_by_key(&mut self, object: Value, key: &str, value: Value) -> MustardResult<()> {
        self.infer_closure_name(&value, key)?;
        match object {
            Value::Object(object) => {
                if self.is_regexp_object(object) && key == "lastIndex" {
                    let index = self.to_integer(value.clone())?.max(0) as usize;
                    self.regexp_object_mut(object)?.last_index = index;
                    return Ok(());
                }
                let new_entry_bytes = Self::property_entry_bytes(key, &value);
                let old_entry_bytes = {
                    let object_ref = self
                        .objects
                        .get_mut(object)
                        .ok_or_else(|| MustardError::runtime("object missing"))?;
                    let old_entry_bytes = object_ref
                        .properties
                        .get(key)
                        .map(|existing| Self::property_entry_bytes(key, existing))
                        .unwrap_or(0);
                    object_ref.properties.insert(key.to_string(), value);
                    old_entry_bytes
                };
                self.apply_object_component_delta(object, old_entry_bytes, new_entry_bytes)?;
                Ok(())
            }
            Value::Array(array) => {
                if key == "length" {
                    self.set_array_length(array, value)?;
                    return Ok(());
                }
                if let Some(index) = array_index_from_property_key(key) {
                    self.set_array_element_at(array, index, value)?;
                } else {
                    let new_entry_bytes = Self::property_entry_bytes(key, &value);
                    let old_entry_bytes = {
                        let array_ref = self
                            .arrays
                            .get_mut(array)
                            .ok_or_else(|| MustardError::runtime("array missing"))?;
                        let old_entry_bytes = array_ref
                            .properties
                            .get(key)
                            .map(|existing| Self::property_entry_bytes(key, existing))
                            .unwrap_or(0);
                        array_ref.properties.insert(key.to_string(), value);
                        old_entry_bytes
                    };
                    self.apply_array_component_delta(array, old_entry_bytes, new_entry_bytes)?;
                }
                Ok(())
            }
            Value::Map(_) => Err(MustardError::runtime(
                "TypeError: custom properties on Map values are not supported",
            )),
            Value::Set(_) => Err(MustardError::runtime(
                "TypeError: custom properties on Set values are not supported",
            )),
            Value::Closure(closure) => self.set_closure_property(closure, key.to_string(), value),
            Value::BuiltinFunction(function) => {
                self.set_builtin_function_property(function, key.to_string(), value)
            }
            Value::HostFunction(capability) => {
                self.set_host_function_property(capability, key.to_string(), value)
            }
            _ => Err(MustardError::runtime("TypeError: value is not an object")),
        }
    }

    pub(super) fn console_method(&self, key: &str) -> Option<Value> {
        let capability = match key {
            "log" => "console.log",
            "warn" => "console.warn",
            "error" => "console.error",
            _ => return None,
        };
        self.capability_value(capability)
    }
}

pub(super) fn canonicalize_collection_key(value: Value) -> Value {
    match value {
        Value::Number(number) if number == 0.0 && number.is_sign_negative() => Value::Number(0.0),
        other => other,
    }
}

pub(super) fn property_name_to_key(name: &PropertyName) -> String {
    match name {
        PropertyName::Identifier(name) | PropertyName::String(name) => name.clone(),
        PropertyName::Number(number) => format_number_key(*number),
    }
}

pub(super) fn format_number_key(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

pub(super) fn array_index_from_property_key(key: &str) -> Option<usize> {
    if key == "0" {
        return Some(0);
    }

    if key.is_empty() || key.starts_with('0') {
        return None;
    }

    let index = key.parse::<u32>().ok()?;
    if index == u32::MAX {
        return None;
    }

    if key == index.to_string() {
        Some(index as usize)
    } else {
        None
    }
}

pub(super) fn ordered_own_property_keys(properties: &IndexMap<String, Value>) -> Vec<String> {
    ordered_own_property_keys_filtered(properties, |_, _| true)
}

pub(super) fn ordered_own_property_keys_filtered<F>(
    properties: &IndexMap<String, Value>,
    mut include: F,
) -> Vec<String>
where
    F: FnMut(&str, &Value) -> bool,
{
    let mut keys = Vec::with_capacity(properties.len());
    let mut index_keys = properties
        .iter()
        .filter(|(key, value)| include(key, value))
        .filter_map(|(key, _)| array_index_from_property_key(key).map(|index| (index, key.clone())))
        .collect::<Vec<_>>();
    index_keys.sort_unstable_by_key(|(index, _)| *index);
    keys.extend(index_keys.into_iter().map(|(_, key)| key));

    keys.extend(
        properties
            .iter()
            .filter(|(key, value)| {
                include(key, value) && array_index_from_property_key(key).is_none()
            })
            .map(|(key, _)| key.clone()),
    );

    keys
}
