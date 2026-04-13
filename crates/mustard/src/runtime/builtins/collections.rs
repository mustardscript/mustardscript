use crate::runtime::properties::canonicalize_collection_key;

use super::*;

impl Runtime {
    pub(crate) fn construct_map(&mut self, args: &[Value]) -> MustardResult<Value> {
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
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .iter()
                    .map(|value| value.clone().unwrap_or(Value::Undefined))
                    .collect(),
                _ => {
                    return Err(MustardError::runtime(
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

    pub(crate) fn construct_set(&mut self, args: &[Value]) -> MustardResult<Value> {
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

    fn map_receiver(&self, value: Value, method: &str) -> MustardResult<MapKey> {
        match value {
            Value::Map(key) => Ok(key),
            _ => Err(MustardError::runtime(format!(
                "TypeError: Map.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn set_receiver(&self, value: Value, method: &str) -> MustardResult<SetKey> {
        match value {
            Value::Set(key) => Ok(key),
            _ => Err(MustardError::runtime(format!(
                "TypeError: Set.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn iterator_receiver(&self, value: Value, method: &str) -> MustardResult<IteratorKey> {
        match value {
            Value::Iterator(key) => Ok(key),
            _ => Err(MustardError::runtime(format!(
                "TypeError: iterator.{method} called on incompatible receiver",
            ))),
        }
    }

    fn collection_callback(&self, method: &str, args: &[Value]) -> MustardResult<(Value, Value)> {
        let callback = args.first().cloned().unwrap_or(Value::Undefined);
        if !is_callable(&callback) {
            return Err(MustardError::runtime(format!(
                "TypeError: {method} expects a callable callback",
            )));
        }
        let this_arg = args.get(1).cloned().unwrap_or(Value::Undefined);
        Ok((callback, this_arg))
    }

    fn call_collection_callback(
        &mut self,
        method: &str,
        callback: Value,
        this_arg: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let host_suspension_message =
            format!("TypeError: {method} does not support synchronous host suspensions");
        let unsettled_message = format!("synchronous {method} callback did not settle");
        self.call_callback(
            callback,
            this_arg,
            args,
            CallbackCallOptions {
                non_callable_message: "TypeError: collection callback is not callable",
                host_suspension_message: &host_suspension_message,
                unsettled_message: &unsettled_message,
                allow_host_suspension: false,
                allow_pending_promise_result: false,
            },
        )
    }

    pub(crate) fn call_iterator_next(&mut self, this_value: Value) -> MustardResult<Value> {
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

    fn map_get(&self, map: MapKey, key: &Value) -> MustardResult<Option<MapEntry>> {
        let normalized = canonicalize_collection_key(key.clone());
        Ok(self
            .maps
            .get(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .entries
            .iter()
            .find(|entry| same_value_zero(&entry.key, &normalized))
            .cloned())
    }

    fn map_set(&mut self, map: MapKey, key: Value, value: Value) -> MustardResult<()> {
        let key = canonicalize_collection_key(key);
        let (old_bytes, new_bytes) = {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(&entry.key, &key))
            {
                let entry = entries
                    .get_mut(index)
                    .ok_or_else(|| MustardError::runtime("map entry missing"))?;
                let old_bytes = Self::map_entry_bytes(entry);
                entry.value = value;
                let new_bytes = Self::map_entry_bytes(entry);
                (old_bytes, new_bytes)
            } else {
                entries.push(MapEntry { key, value });
                let entry = entries
                    .last()
                    .ok_or_else(|| MustardError::runtime("map entry missing"))?;
                (0, Self::map_entry_bytes(entry))
            }
        };
        self.apply_map_component_delta(map, old_bytes, new_bytes)
    }

    fn adjust_map_iterators_after_delete(&mut self, map: MapKey, removed_index: usize) {
        let iterator_keys: Vec<_> = self
            .iterators
            .iter()
            .filter_map(|(key, iterator)| match &iterator.state {
                IteratorState::MapEntries(state)
                | IteratorState::MapKeys(state)
                | IteratorState::MapValues(state)
                    if state.map == map && removed_index < state.next_index =>
                {
                    Some(key)
                }
                _ => None,
            })
            .collect();
        for key in iterator_keys {
            if let Some(iterator) = self.iterators.get_mut(key) {
                match &mut iterator.state {
                    IteratorState::MapEntries(state)
                    | IteratorState::MapKeys(state)
                    | IteratorState::MapValues(state) => state.next_index -= 1,
                    _ => unreachable!("filtered iterator kind changed unexpectedly"),
                }
            }
        }
    }

    fn reset_map_iterators_after_clear(&mut self, map: MapKey) {
        let iterator_keys: Vec<_> = self
            .iterators
            .iter()
            .filter_map(|(key, iterator)| match &iterator.state {
                IteratorState::MapEntries(state)
                | IteratorState::MapKeys(state)
                | IteratorState::MapValues(state)
                    if state.map == map =>
                {
                    Some(key)
                }
                _ => None,
            })
            .collect();
        for key in iterator_keys {
            if let Some(iterator) = self.iterators.get_mut(key) {
                match &mut iterator.state {
                    IteratorState::MapEntries(state)
                    | IteratorState::MapKeys(state)
                    | IteratorState::MapValues(state) => state.next_index = 0,
                    _ => unreachable!("filtered iterator kind changed unexpectedly"),
                }
            }
        }
    }

    fn map_delete(&mut self, map: MapKey, key: &Value) -> MustardResult<bool> {
        let normalized = canonicalize_collection_key(key.clone());
        let removed = {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(&entry.key, &normalized))
            {
                let removed_bytes = Self::map_entry_bytes(
                    entries
                        .get(index)
                        .ok_or_else(|| MustardError::runtime("map entry missing"))?,
                );
                entries.remove(index);
                Some((index, removed_bytes))
            } else {
                None
            }
        };
        if let Some((index, removed_bytes)) = removed {
            self.adjust_map_iterators_after_delete(map, index);
            self.apply_map_component_delta(map, removed_bytes, 0)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn map_clear(&mut self, map: MapKey) -> MustardResult<()> {
        let removed_bytes = {
            let map_ref = self
                .maps
                .get(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?;
            map_ref
                .entries
                .iter()
                .map(Self::map_entry_bytes)
                .sum::<usize>()
        };
        self.maps
            .get_mut(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .entries
            .clear();
        self.reset_map_iterators_after_clear(map);
        self.apply_map_component_delta(map, removed_bytes, 0)
    }

    fn set_add(&mut self, set: SetKey, value: Value) -> MustardResult<()> {
        let value = canonicalize_collection_key(value);
        let added_bytes = {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?
                .entries;
            if entries.iter().any(|entry| same_value_zero(entry, &value)) {
                0
            } else {
                let added_bytes = Self::set_entry_bytes(&value);
                entries.push(value);
                added_bytes
            }
        };
        if added_bytes == 0 {
            return Ok(());
        }
        self.apply_set_component_delta(set, 0, added_bytes)
    }

    fn adjust_set_iterators_after_delete(&mut self, set: SetKey, removed_index: usize) {
        let iterator_keys: Vec<_> = self
            .iterators
            .iter()
            .filter_map(|(key, iterator)| match &iterator.state {
                IteratorState::SetEntries(state) | IteratorState::SetValues(state)
                    if state.set == set && removed_index < state.next_index =>
                {
                    Some(key)
                }
                _ => None,
            })
            .collect();
        for key in iterator_keys {
            if let Some(iterator) = self.iterators.get_mut(key) {
                match &mut iterator.state {
                    IteratorState::SetEntries(state) | IteratorState::SetValues(state) => {
                        state.next_index -= 1
                    }
                    _ => unreachable!("filtered iterator kind changed unexpectedly"),
                }
            }
        }
    }

    fn reset_set_iterators_after_clear(&mut self, set: SetKey) {
        let iterator_keys: Vec<_> = self
            .iterators
            .iter()
            .filter_map(|(key, iterator)| match &iterator.state {
                IteratorState::SetEntries(state) | IteratorState::SetValues(state)
                    if state.set == set =>
                {
                    Some(key)
                }
                _ => None,
            })
            .collect();
        for key in iterator_keys {
            if let Some(iterator) = self.iterators.get_mut(key) {
                match &mut iterator.state {
                    IteratorState::SetEntries(state) | IteratorState::SetValues(state) => {
                        state.next_index = 0
                    }
                    _ => unreachable!("filtered iterator kind changed unexpectedly"),
                }
            }
        }
    }

    fn set_contains(&self, set: SetKey, value: &Value) -> MustardResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        Ok(self
            .sets
            .get(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .entries
            .iter()
            .any(|entry| same_value_zero(entry, &normalized)))
    }

    fn set_delete(&mut self, set: SetKey, value: &Value) -> MustardResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        let removed = {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(entry, &normalized))
            {
                let removed_bytes = Self::set_entry_bytes(
                    entries
                        .get(index)
                        .ok_or_else(|| MustardError::runtime("set entry missing"))?,
                );
                entries.remove(index);
                Some((index, removed_bytes))
            } else {
                None
            }
        };
        if let Some((index, removed_bytes)) = removed {
            self.adjust_set_iterators_after_delete(set, index);
            self.apply_set_component_delta(set, removed_bytes, 0)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn set_clear(&mut self, set: SetKey) -> MustardResult<()> {
        let removed_bytes = {
            let set_ref = self
                .sets
                .get(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?;
            set_ref
                .entries
                .iter()
                .map(Self::set_entry_bytes)
                .sum::<usize>()
        };
        self.sets
            .get_mut(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .entries
            .clear();
        self.reset_set_iterators_after_clear(set);
        self.apply_set_component_delta(set, removed_bytes, 0)
    }

    pub(crate) fn call_map_get(&self, this_value: Value, args: &[Value]) -> MustardResult<Value> {
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
    ) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "set")?;
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let value = args.get(1).cloned().unwrap_or(Value::Undefined);
        self.map_set(map, key, value)?;
        Ok(Value::Map(map))
    }

    pub(crate) fn call_map_has(&self, this_value: Value, args: &[Value]) -> MustardResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "has")?;
        Ok(Value::Bool(self.map_get(map, &key)?.is_some()))
    }

    pub(crate) fn call_map_delete(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.map_delete(map, &key)?))
    }

    pub(crate) fn call_map_clear(&mut self, this_value: Value) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "clear")?;
        self.map_clear(map)?;
        Ok(Value::Undefined)
    }

    pub(crate) fn call_map_entries(&mut self, this_value: Value) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapEntries(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_map_keys(&mut self, this_value: Value) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapKeys(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_map_values(&mut self, this_value: Value) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapValues(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_map_for_each(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "forEach")?;
        let (callback, this_arg) = self.collection_callback("Map.prototype.forEach", args)?;
        let mut index = 0usize;
        while index
            < self
                .maps
                .get(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?
                .entries
                .len()
        {
            self.charge_native_helper_work(1)?;
            if let Some(entry) = self
                .maps
                .get(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?
                .entries
                .get(index)
                .cloned()
            {
                self.with_temporary_roots(
                    &[Value::Map(map), callback.clone(), this_arg.clone()],
                    |runtime| {
                        runtime.call_collection_callback(
                            "Map.prototype.forEach",
                            callback.clone(),
                            this_arg.clone(),
                            &[entry.value, entry.key, Value::Map(map)],
                        )?;
                        Ok(())
                    },
                )?;
            }
            index += 1;
        }
        Ok(Value::Undefined)
    }

    pub(crate) fn call_set_add(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "add")?;
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        self.set_add(set, value)?;
        Ok(Value::Set(set))
    }

    pub(crate) fn call_set_has(&self, this_value: Value, args: &[Value]) -> MustardResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "has")?;
        Ok(Value::Bool(self.set_contains(set, &value)?))
    }

    pub(crate) fn call_set_delete(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.set_delete(set, &value)?))
    }

    pub(crate) fn call_set_clear(&mut self, this_value: Value) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "clear")?;
        self.set_clear(set)?;
        Ok(Value::Undefined)
    }

    pub(crate) fn call_set_entries(&mut self, this_value: Value) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetEntries(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_set_keys(&mut self, this_value: Value) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_set_values(&mut self, this_value: Value) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    pub(crate) fn call_set_for_each(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "forEach")?;
        let (callback, this_arg) = self.collection_callback("Set.prototype.forEach", args)?;
        let mut index = 0usize;
        while index
            < self
                .sets
                .get(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?
                .entries
                .len()
        {
            self.charge_native_helper_work(1)?;
            if let Some(value) = self
                .sets
                .get(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?
                .entries
                .get(index)
                .cloned()
            {
                self.with_temporary_roots(
                    &[Value::Set(set), callback.clone(), this_arg.clone()],
                    |runtime| {
                        runtime.call_collection_callback(
                            "Set.prototype.forEach",
                            callback.clone(),
                            this_arg.clone(),
                            &[value.clone(), value, Value::Set(set)],
                        )?;
                        Ok(())
                    },
                )?;
            }
            index += 1;
        }
        Ok(Value::Undefined)
    }
}
