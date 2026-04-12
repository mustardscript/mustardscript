use crate::runtime::properties::canonicalize_collection_key;

use super::*;

impl Runtime {
    pub(crate) fn construct_map(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let map = self.insert_map(Vec::new())?;
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        if matches!(iterable, Value::Null | Value::Undefined) {
            return Ok(Value::Map(map));
        }

        let iterator = self.create_iterator(iterable)?;
        loop {
            let (entry, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            let items: Vec<Value> = match entry {
                Value::Array(array) => self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .iter()
                    .map(|value| value.clone().unwrap_or(Value::Undefined))
                    .collect(),
                _ => {
                    return Err(JsliteError::runtime(
                        "TypeError: Map constructor expects an iterable of [key, value] pairs",
                    ));
                }
            };
            let key = items.first().cloned().unwrap_or(Value::Undefined);
            let value = items.get(1).cloned().unwrap_or(Value::Undefined);
            self.map_set(map, key, value)?;
        }

        Ok(Value::Map(map))
    }

    pub(crate) fn construct_set(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let set = self.insert_set(Vec::new())?;
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        if matches!(iterable, Value::Null | Value::Undefined) {
            return Ok(Value::Set(set));
        }

        let iterator = self.create_iterator(iterable)?;
        loop {
            let (value, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            self.set_add(set, value)?;
        }

        Ok(Value::Set(set))
    }

    fn map_receiver(&self, value: Value, method: &str) -> JsliteResult<MapKey> {
        match value {
            Value::Map(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Map.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn set_receiver(&self, value: Value, method: &str) -> JsliteResult<SetKey> {
        match value {
            Value::Set(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Set.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn iterator_receiver(&self, value: Value, method: &str) -> JsliteResult<IteratorKey> {
        match value {
            Value::Iterator(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: iterator.{method} called on incompatible receiver",
            ))),
        }
    }

    pub(crate) fn call_iterator_next(&mut self, this_value: Value) -> JsliteResult<Value> {
        let iterator = self.iterator_receiver(this_value, "next")?;
        let (value, done) = self.iterator_next(Value::Iterator(iterator))?;
        let result = self.insert_object(
            IndexMap::from([
                ("value".to_string(), value),
                ("done".to_string(), Value::Bool(done)),
            ]),
            ObjectKind::Plain,
        )?;
        Ok(Value::Object(result))
    }

    fn map_get(&self, map: MapKey, key: &Value) -> JsliteResult<Option<MapEntry>> {
        let normalized = canonicalize_collection_key(key.clone());
        Ok(self
            .maps
            .get(map)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .entries
            .iter()
            .find(|entry| same_value_zero(&entry.key, &normalized))
            .cloned())
    }

    fn map_set(&mut self, map: MapKey, key: Value, value: Value) -> JsliteResult<()> {
        let key = canonicalize_collection_key(key);
        {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries;
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| same_value_zero(&entry.key, &key))
            {
                entry.value = value;
            } else {
                entries.push(MapEntry { key, value });
            }
        }
        self.refresh_map_accounting(map)
    }

    fn map_delete(&mut self, map: MapKey, key: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(key.clone());
        let removed = {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(&entry.key, &normalized))
            {
                entries.remove(index);
                true
            } else {
                false
            }
        };
        if removed {
            self.refresh_map_accounting(map)?;
        }
        Ok(removed)
    }

    fn map_clear(&mut self, map: MapKey) -> JsliteResult<()> {
        self.maps
            .get_mut(map)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .entries
            .clear();
        self.refresh_map_accounting(map)
    }

    fn set_add(&mut self, set: SetKey, value: Value) -> JsliteResult<()> {
        let value = canonicalize_collection_key(value);
        {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries;
            if !entries.iter().any(|entry| same_value_zero(entry, &value)) {
                entries.push(value);
            }
        }
        self.refresh_set_accounting(set)
    }

    fn set_contains(&self, set: SetKey, value: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        Ok(self
            .sets
            .get(set)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .entries
            .iter()
            .any(|entry| same_value_zero(entry, &normalized)))
    }

    fn set_delete(&mut self, set: SetKey, value: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        let removed = {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(entry, &normalized))
            {
                entries.remove(index);
                true
            } else {
                false
            }
        };
        if removed {
            self.refresh_set_accounting(set)?;
        }
        Ok(removed)
    }

    fn set_clear(&mut self, set: SetKey) -> JsliteResult<()> {
        self.sets
            .get_mut(set)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .entries
            .clear();
        self.refresh_set_accounting(set)
    }

    pub(crate) fn call_map_get(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "get")?;
        Ok(self
            .map_get(map, &key)?
            .map(|entry| entry.value)
            .unwrap_or(Value::Undefined))
    }

    pub(crate) fn call_map_set(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "set")?;
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let value = args.get(1).cloned().unwrap_or(Value::Undefined);
        self.map_set(map, key, value)?;
        Ok(Value::Map(map))
    }

    pub(crate) fn call_map_has(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "has")?;
        Ok(Value::Bool(self.map_get(map, &key)?.is_some()))
    }

    pub(crate) fn call_map_delete(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.map_delete(map, &key)?))
    }

    pub(crate) fn call_map_clear(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "clear")?;
        self.map_clear(map)?;
        Ok(Value::Undefined)
    }

    pub(crate) fn call_map_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapEntries(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_map_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapKeys(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_map_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapValues(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_set_add(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "add")?;
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        self.set_add(set, value)?;
        Ok(Value::Set(set))
    }

    pub(crate) fn call_set_has(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "has")?;
        Ok(Value::Bool(self.set_contains(set, &value)?))
    }

    pub(crate) fn call_set_delete(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.set_delete(set, &value)?))
    }

    pub(crate) fn call_set_clear(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "clear")?;
        self.set_clear(set)?;
        Ok(Value::Undefined)
    }

    pub(crate) fn call_set_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetEntries(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_set_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_set_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }
}
