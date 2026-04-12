use super::*;

impl Runtime {
    pub(super) fn has_property_in_supported_surface(
        &self,
        object: Value,
        property: Value,
    ) -> JsliteResult<bool> {
        let key = self.to_property_key(property)?;
        match object {
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?;
                if object.properties.contains_key(&key) {
                    return Ok(true);
                }
                Ok(match &object.kind {
                    ObjectKind::Date(_) => matches!(key.as_str(), "getTime" | "valueOf"),
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
                    ObjectKind::Console => self.console_method(&key).is_some(),
                    _ => false,
                })
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                Ok(key == "length"
                    || key
                        .parse::<usize>()
                        .ok()
                        .is_some_and(|index| index < array.elements.len())
                    || array.properties.contains_key(&key)
                    || matches!(
                        key.as_str(),
                        "sort"
                            | "push"
                            | "pop"
                            | "slice"
                            | "concat"
                            | "at"
                            | "join"
                            | "includes"
                            | "indexOf"
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
                            | "reduce"
                    ))
            }
            Value::Map(map) => {
                self.maps
                    .get(map)
                    .ok_or_else(|| JsliteError::runtime("map missing"))?;
                Ok(matches!(
                    key.as_str(),
                    "size"
                        | "get"
                        | "set"
                        | "has"
                        | "delete"
                        | "clear"
                        | "entries"
                        | "keys"
                        | "values"
                ))
            }
            Value::Set(set) => {
                self.sets
                    .get(set)
                    .ok_or_else(|| JsliteError::runtime("set missing"))?;
                Ok(matches!(
                    key.as_str(),
                    "size" | "add" | "has" | "delete" | "clear" | "entries" | "keys" | "values"
                ))
            }
            Value::Iterator(iterator) => {
                self.iterators
                    .get(iterator)
                    .ok_or_else(|| JsliteError::runtime("iterator missing"))?;
                Ok(key == "next")
            }
            Value::Promise(promise) => {
                self.promises
                    .get(promise)
                    .ok_or_else(|| JsliteError::runtime("promise missing"))?;
                Ok(matches!(key.as_str(), "then" | "catch" | "finally"))
            }
            Value::BuiltinFunction(function) => Ok(match function {
                BuiltinFunction::ArrayCtor => matches!(key.as_str(), "isArray" | "from" | "of"),
                BuiltinFunction::ObjectCtor => matches!(
                    key.as_str(),
                    "assign"
                        | "create"
                        | "freeze"
                        | "seal"
                        | "fromEntries"
                        | "keys"
                        | "values"
                        | "entries"
                        | "hasOwn"
                ),
                BuiltinFunction::DateCtor => key == "now",
                BuiltinFunction::PromiseCtor => matches!(
                    key.as_str(),
                    "resolve" | "reject" | "all" | "race" | "any" | "allSettled"
                ),
                _ => false,
            }),
            Value::Closure(closure) => {
                self.closures
                    .get(closure)
                    .ok_or_else(|| JsliteError::runtime("closure missing"))?;
                Ok(false)
            }
            Value::HostFunction(_) => Ok(false),
            Value::Undefined
            | Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::BigInt(_) => Err(JsliteError::runtime(
                "TypeError: right-hand side of 'in' must be an object in the supported surface",
            )),
        }
    }

    pub(super) fn create_iterator(&mut self, iterable: Value) -> JsliteResult<Value> {
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
                return Err(JsliteError::runtime(
                    "TypeError: value is not iterable in the supported surface",
                ));
            }
        };
        Ok(Value::Iterator(iterator))
    }

    pub(super) fn iterator_next(&mut self, iterator: Value) -> JsliteResult<(Value, bool)> {
        let key = match iterator {
            Value::Iterator(key) => key,
            _ => return Err(JsliteError::runtime("value is not an iterator")),
        };

        let state = self
            .iterators
            .get(key)
            .ok_or_else(|| JsliteError::runtime("iterator missing"))?
            .state
            .clone();

        let value = match state {
            IteratorState::Array(state) => self
                .arrays
                .get(state.array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements
                .get(state.next_index)
                .cloned(),
            IteratorState::ArrayKeys(state) => self
                .arrays
                .get(state.array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements
                .get(state.next_index)
                .map(|_| Value::Number(state.next_index as f64)),
            IteratorState::ArrayEntries(state) => {
                let value = self
                    .arrays
                    .get(state.array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .get(state.next_index)
                    .cloned();
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
                    .ok_or_else(|| JsliteError::runtime("map missing"))?
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
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries
                .get(state.next_index)
                .map(|entry| entry.key.clone()),
            IteratorState::MapValues(state) => self
                .maps
                .get(state.map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries
                .get(state.next_index)
                .map(|entry| entry.value.clone()),
            IteratorState::SetEntries(state) => self
                .sets
                .get(state.set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
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
                .ok_or_else(|| JsliteError::runtime("set missing"))?
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
    ) -> JsliteResult<Value> {
        if optional && matches!(object, Value::Null | Value::Undefined) {
            return Ok(Value::Undefined);
        }
        let key = self.to_property_key(property)?;
        match object {
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?;
                if let ObjectKind::Date(date) = &object.kind {
                    let built_in = match key.as_str() {
                        "getTime" => Some(Value::BuiltinFunction(BuiltinFunction::DateGetTime)),
                        "valueOf" => Some(Value::Number(date.timestamp_ms)),
                        _ => None,
                    };
                    if let Some(value) = built_in {
                        return Ok(value);
                    }
                }
                if let ObjectKind::RegExp(regex) = &object.kind {
                    let built_in = match key.as_str() {
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
                        _ => None,
                    };
                    if let Some(value) = built_in {
                        return Ok(value);
                    }
                }
                if let Some(value) = object.properties.get(&key) {
                    return Ok(value.clone());
                }
                if matches!(object.kind, ObjectKind::Console)
                    && let Some(value) = self.console_method(&key)
                {
                    return Ok(value);
                }
                Ok(Value::Undefined)
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                if key == "length" {
                    Ok(Value::Number(array.elements.len() as f64))
                } else if let Ok(index) = key.parse::<usize>() {
                    Ok(array
                        .elements
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Undefined))
                } else if let Some(value) = array.properties.get(&key) {
                    Ok(value.clone())
                } else {
                    match key.as_str() {
                        "sort" => Ok(Value::BuiltinFunction(BuiltinFunction::ArraySort)),
                        "push" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayPush)),
                        "pop" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayPop)),
                        "slice" => Ok(Value::BuiltinFunction(BuiltinFunction::ArraySlice)),
                        "concat" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayConcat)),
                        "at" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayAt)),
                        "join" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayJoin)),
                        "includes" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIncludes)),
                        "indexOf" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIndexOf)),
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
                        "reduce" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayReduce)),
                        _ => Ok(Value::Undefined),
                    }
                }
            }
            Value::Map(map) => {
                let map = self
                    .maps
                    .get(map)
                    .ok_or_else(|| JsliteError::runtime("map missing"))?;
                match key.as_str() {
                    "size" => Ok(Value::Number(map.entries.len() as f64)),
                    "get" => Ok(Value::BuiltinFunction(BuiltinFunction::MapGet)),
                    "set" => Ok(Value::BuiltinFunction(BuiltinFunction::MapSet)),
                    "has" => Ok(Value::BuiltinFunction(BuiltinFunction::MapHas)),
                    "delete" => Ok(Value::BuiltinFunction(BuiltinFunction::MapDelete)),
                    "clear" => Ok(Value::BuiltinFunction(BuiltinFunction::MapClear)),
                    "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::MapEntries)),
                    "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::MapKeys)),
                    "values" => Ok(Value::BuiltinFunction(BuiltinFunction::MapValues)),
                    _ => Ok(Value::Undefined),
                }
            }
            Value::Set(set) => {
                let set = self
                    .sets
                    .get(set)
                    .ok_or_else(|| JsliteError::runtime("set missing"))?;
                match key.as_str() {
                    "size" => Ok(Value::Number(set.entries.len() as f64)),
                    "add" => Ok(Value::BuiltinFunction(BuiltinFunction::SetAdd)),
                    "has" => Ok(Value::BuiltinFunction(BuiltinFunction::SetHas)),
                    "delete" => Ok(Value::BuiltinFunction(BuiltinFunction::SetDelete)),
                    "clear" => Ok(Value::BuiltinFunction(BuiltinFunction::SetClear)),
                    "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::SetEntries)),
                    "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::SetKeys)),
                    "values" => Ok(Value::BuiltinFunction(BuiltinFunction::SetValues)),
                    _ => Ok(Value::Undefined),
                }
            }
            Value::Iterator(_) if key == "next" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::IteratorNext))
            }
            Value::Iterator(_) => Ok(Value::Undefined),
            Value::Promise(_) => match key.as_str() {
                "then" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseThen)),
                "catch" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseCatch)),
                "finally" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseFinally)),
                _ => Ok(Value::Undefined),
            },
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor) if key == "isArray" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIsArray))
            }
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor) if key == "from" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::ArrayFrom))
            }
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor) if key == "of" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::ArrayOf))
            }
            Value::BuiltinFunction(BuiltinFunction::ObjectCtor) => match key.as_str() {
                "assign" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectAssign)),
                "create" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectCreate)),
                "freeze" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectFreeze)),
                "seal" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectSeal)),
                "fromEntries" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectFromEntries)),
                "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectKeys)),
                "values" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectValues)),
                "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectEntries)),
                "hasOwn" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectHasOwn)),
                _ => Ok(Value::Undefined),
            },
            Value::BuiltinFunction(BuiltinFunction::DateCtor) if key == "now" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::DateNow))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "resolve" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseResolve))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "reject" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseReject))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "all" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseAll))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "race" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseRace))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "any" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseAny))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "allSettled" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseAllSettled))
            }
            Value::BuiltinFunction(BuiltinFunction::RegExpCtor) => Ok(Value::Undefined),
            Value::String(value) => match key.as_str() {
                "length" => Ok(Value::Number(value.chars().count() as f64)),
                "trim" => Ok(Value::BuiltinFunction(BuiltinFunction::StringTrim)),
                "includes" => Ok(Value::BuiltinFunction(BuiltinFunction::StringIncludes)),
                "startsWith" => Ok(Value::BuiltinFunction(BuiltinFunction::StringStartsWith)),
                "endsWith" => Ok(Value::BuiltinFunction(BuiltinFunction::StringEndsWith)),
                "slice" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSlice)),
                "substring" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSubstring)),
                "toLowerCase" => Ok(Value::BuiltinFunction(BuiltinFunction::StringToLowerCase)),
                "toUpperCase" => Ok(Value::BuiltinFunction(BuiltinFunction::StringToUpperCase)),
                "split" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSplit)),
                "replace" => Ok(Value::BuiltinFunction(BuiltinFunction::StringReplace)),
                "replaceAll" => Ok(Value::BuiltinFunction(BuiltinFunction::StringReplaceAll)),
                "search" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSearch)),
                "match" => Ok(Value::BuiltinFunction(BuiltinFunction::StringMatch)),
                "matchAll" => Ok(Value::BuiltinFunction(BuiltinFunction::StringMatchAll)),
                _ => {
                    let _ = value;
                    Ok(Value::Undefined)
                }
            },
            Value::Null | Value::Undefined => Err(JsliteError::runtime(
                "TypeError: cannot read properties of nullish value",
            )),
            _ => Ok(Value::Undefined),
        }
    }

    pub(super) fn set_property(
        &mut self,
        object: Value,
        property: Value,
        value: Value,
    ) -> JsliteResult<()> {
        let key = self.to_property_key(property)?;
        match object {
            Value::Object(object) => {
                if self.is_regexp_object(object) && key == "lastIndex" {
                    let index = self.to_integer(value.clone())?.max(0) as usize;
                    self.regexp_object_mut(object)?.last_index = index;
                    self.refresh_object_accounting(object)?;
                    return Ok(());
                }
                self.objects
                    .get_mut(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .insert(key, value);
                self.refresh_object_accounting(object)?;
                Ok(())
            }
            Value::Array(array) => {
                {
                    let array_ref = self
                        .arrays
                        .get_mut(array)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?;
                    if let Ok(index) = key.parse::<usize>() {
                        if index >= array_ref.elements.len() {
                            array_ref.elements.resize(index + 1, Value::Undefined);
                        }
                        array_ref.elements[index] = value;
                    } else {
                        array_ref.properties.insert(key, value);
                    }
                }
                self.refresh_array_accounting(array)?;
                Ok(())
            }
            Value::Map(_) => Err(JsliteError::runtime(
                "TypeError: custom properties on Map values are not supported",
            )),
            Value::Set(_) => Err(JsliteError::runtime(
                "TypeError: custom properties on Set values are not supported",
            )),
            _ => Err(JsliteError::runtime("TypeError: value is not an object")),
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
