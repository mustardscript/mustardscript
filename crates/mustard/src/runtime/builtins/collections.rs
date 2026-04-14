use crate::runtime::properties::canonicalize_collection_key;

use super::*;

impl Runtime {
    pub(crate) fn construct_map(&mut self, args: &[Value]) -> MustardResult<Value> {
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        let length_hint = self.iterable_length_hint(&iterable)?;
        let map = match length_hint {
            Some(length) => self.insert_map_slots(vec![None; length])?,
            None => self.insert_map(Vec::new())?,
        };
        if matches!(iterable, Value::Null | Value::Undefined) {
            return Ok(Value::Map(map));
        }

        let iterator = self.create_iterator(iterable)?;
        loop {
            let (entry, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            let (key, value) = self.array_entry_pair(
                entry,
                "TypeError: Map constructor expects an iterable of [key, value] pairs",
            )?;
            self.map_set(map, key, value)?;
        }
        if length_hint.is_some() {
            self.trim_trailing_map_builder_slots(map)?;
        }

        Ok(Value::Map(map))
    }

    pub(crate) fn construct_set(&mut self, args: &[Value]) -> MustardResult<Value> {
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        let length_hint = self.iterable_length_hint(&iterable)?;
        let set = match length_hint {
            Some(length) => self.insert_set_slots(vec![None; length])?,
            None => self.insert_set(Vec::new())?,
        };
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
        if length_hint.is_some() {
            self.trim_trailing_set_builder_slots(set)?;
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

    pub(in crate::runtime) fn next_map_entry_from_state(
        &self,
        map: MapKey,
        next_index: &mut usize,
        observed_clear_epoch: &mut u64,
    ) -> MustardResult<Option<MapEntry>> {
        let map_ref = self
            .maps
            .get(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?;
        if *observed_clear_epoch != map_ref.clear_epoch {
            *observed_clear_epoch = map_ref.clear_epoch;
            *next_index = 0;
        }
        while let Some(entry) = map_ref.entries.get(*next_index) {
            *next_index += 1;
            if let Some(entry) = entry.clone() {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    pub(in crate::runtime) fn next_set_value_from_state(
        &self,
        set: SetKey,
        next_index: &mut usize,
        observed_clear_epoch: &mut u64,
    ) -> MustardResult<Option<Value>> {
        let set_ref = self
            .sets
            .get(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?;
        if *observed_clear_epoch != set_ref.clear_epoch {
            *observed_clear_epoch = set_ref.clear_epoch;
            *next_index = 0;
        }
        while let Some(value) = set_ref.entries.get(*next_index) {
            *next_index += 1;
            if let Some(value) = value.clone() {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    fn map_lookup_bytes(map: &MapObject) -> usize {
        map.lookup
            .keys()
            .map(Self::collection_index_entry_bytes)
            .sum()
    }

    fn set_lookup_bytes(set: &SetObject) -> usize {
        set.lookup
            .keys()
            .map(Self::collection_index_entry_bytes)
            .sum()
    }

    fn map_slot_by_key(map: &MapObject, key: &Value) -> Option<usize> {
        if map.lookup.is_empty() {
            map.entries.iter().enumerate().find_map(|(index, entry)| {
                entry
                    .as_ref()
                    .is_some_and(|entry| same_value_zero(&entry.key, key))
                    .then_some(index)
            })
        } else {
            map.lookup
                .get(&CollectionLookupKey::from_value(key))
                .copied()
        }
    }

    fn set_slot_by_value(set: &SetObject, value: &Value) -> Option<usize> {
        if set.lookup.is_empty() {
            set.entries.iter().enumerate().find_map(|(index, entry)| {
                entry
                    .as_ref()
                    .is_some_and(|entry| same_value_zero(entry, value))
                    .then_some(index)
            })
        } else {
            set.lookup
                .get(&CollectionLookupKey::from_value(value))
                .copied()
        }
    }

    pub(in crate::runtime) fn map_get(
        &self,
        map: MapKey,
        key: &Value,
    ) -> MustardResult<Option<MapEntry>> {
        let map_ref = self
            .maps
            .get(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?;
        let Some(slot) = Self::map_slot_by_key(map_ref, key) else {
            return Ok(None);
        };
        let entry = map_ref
            .entries
            .get(slot)
            .and_then(|entry| entry.clone())
            .ok_or_else(|| MustardError::runtime("map entry missing"))?;
        Ok(Some(entry))
    }

    pub(in crate::runtime) fn map_set(
        &mut self,
        map: MapKey,
        key: Value,
        value: Value,
    ) -> MustardResult<()> {
        let key = canonicalize_collection_key(key);
        let string_key = uses_string_heavy_collection_lookup(&key);
        let index_key = CollectionIndexKey::from_value(&key);
        let existing_slot = {
            let map_ref = self
                .maps
                .get(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?;
            Self::map_slot_by_key(map_ref, &key)
        };
        let (old_bytes, new_bytes) = {
            let map_ref = self
                .maps
                .get_mut(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?;
            if let Some(slot) = existing_slot {
                let entry = map_ref
                    .entries
                    .get_mut(slot)
                    .ok_or_else(|| MustardError::runtime("map entry missing"))?;
                let old_bytes = Self::map_slot_bytes(entry.as_ref());
                let entry = entry
                    .as_mut()
                    .ok_or_else(|| MustardError::runtime("map entry missing"))?;
                entry.value = value;
                let new_bytes = Self::map_slot_bytes(Some(entry));
                (old_bytes, new_bytes)
            } else {
                let new_entry = MapEntry { key, value };
                let (slot, old_slot_bytes) = if map_ref.live_len < map_ref.entries.len()
                    && map_ref.entries[map_ref.live_len].is_none()
                {
                    let slot = map_ref.live_len;
                    let old_slot_bytes = Self::map_slot_bytes(None);
                    map_ref.entries[slot] = Some(new_entry);
                    (slot, old_slot_bytes)
                } else {
                    map_ref.entries.push(Some(new_entry));
                    let slot = map_ref
                        .entries
                        .len()
                        .checked_sub(1)
                        .ok_or_else(|| MustardError::runtime("map entry missing"))?;
                    (slot, 0)
                };
                let old_lookup_bytes = Self::map_lookup_bytes(map_ref);
                map_ref.live_len = map_ref
                    .live_len
                    .checked_add(1)
                    .ok_or_else(|| MustardError::runtime("map size overflow"))?;
                if string_key {
                    map_ref.string_key_live_len = map_ref
                        .string_key_live_len
                        .checked_add(1)
                        .ok_or_else(|| MustardError::runtime("map string-key overflow"))?;
                }
                let lookup_bytes = if map_ref.lookup.is_empty()
                    && map_ref.live_len >= map_ref.lookup_promotion_len()
                {
                    map_ref.rebuild_lookup();
                    Self::map_lookup_bytes(map_ref)
                } else if !map_ref.lookup.is_empty() {
                    map_ref.lookup.insert(index_key, slot);
                    if map_ref.live_len < map_ref.lookup_promotion_len() {
                        map_ref.lookup.clear();
                    }
                    Self::map_lookup_bytes(map_ref)
                } else {
                    0
                };
                let entry = map_ref
                    .entries
                    .get(slot)
                    .and_then(|entry| entry.as_ref())
                    .ok_or_else(|| MustardError::runtime("map entry missing"))?;
                let old_bytes = old_slot_bytes + old_lookup_bytes;
                let new_bytes = Self::map_slot_bytes(Some(entry)) + lookup_bytes;
                (old_bytes, new_bytes)
            }
        };
        self.apply_map_component_delta(map, old_bytes, new_bytes)
    }

    fn map_delete(&mut self, map: MapKey, key: &Value) -> MustardResult<bool> {
        let slot = {
            let map_ref = self
                .maps
                .get(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?;
            Self::map_slot_by_key(map_ref, key)
        };
        let Some(slot) = slot else {
            return Ok(false);
        };
        let (old_bytes, new_bytes) = {
            let map_ref = self
                .maps
                .get_mut(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?;
            let old_lookup_bytes = Self::map_lookup_bytes(map_ref);
            let entry = map_ref
                .entries
                .get_mut(slot)
                .ok_or_else(|| MustardError::runtime("map entry missing"))?;
            let old_slot_bytes = Self::map_slot_bytes(entry.as_ref());
            let removed_key = entry
                .as_ref()
                .map(|entry| CollectionIndexKey::from_value(&entry.key))
                .ok_or_else(|| MustardError::runtime("map entry missing"))?;
            let removed_string_key = entry
                .as_ref()
                .is_some_and(|entry| uses_string_heavy_collection_lookup(&entry.key));
            *entry = None;
            map_ref.lookup.swap_remove(&removed_key);
            map_ref.live_len = map_ref
                .live_len
                .checked_sub(1)
                .ok_or_else(|| MustardError::runtime("map size underflow"))?;
            if removed_string_key {
                map_ref.string_key_live_len = map_ref
                    .string_key_live_len
                    .checked_sub(1)
                    .ok_or_else(|| MustardError::runtime("map string-key underflow"))?;
            }
            if !map_ref.lookup.is_empty() && map_ref.live_len < map_ref.lookup_promotion_len() {
                map_ref.lookup.clear();
            }
            let new_bytes = Self::map_slot_bytes(None) + Self::map_lookup_bytes(map_ref);
            (old_slot_bytes + old_lookup_bytes, new_bytes)
        };
        self.apply_map_component_delta(map, old_bytes, new_bytes)?;
        Ok(true)
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
                .map(|entry| Self::map_slot_bytes(entry.as_ref()))
                .sum::<usize>()
                + Self::map_lookup_bytes(map_ref)
        };
        let map_ref = self
            .maps
            .get_mut(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?;
        map_ref.entries.clear();
        map_ref.lookup.clear();
        map_ref.live_len = 0;
        map_ref.string_key_live_len = 0;
        map_ref.clear_epoch = map_ref.clear_epoch.wrapping_add(1);
        self.apply_map_component_delta(map, removed_bytes, 0)
    }

    fn set_add(&mut self, set: SetKey, value: Value) -> MustardResult<()> {
        let value = canonicalize_collection_key(value);
        let string_key = uses_string_heavy_collection_lookup(&value);
        let index_key = CollectionIndexKey::from_value(&value);
        let existing_slot = {
            let set_ref = self
                .sets
                .get(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?;
            Self::set_slot_by_value(set_ref, &value)
        };
        let (old_bytes, new_bytes) = {
            let set_ref = self
                .sets
                .get_mut(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?;
            if existing_slot.is_some() {
                (0, 0)
            } else {
                let (slot, old_slot_bytes) = if set_ref.live_len < set_ref.entries.len()
                    && set_ref.entries[set_ref.live_len].is_none()
                {
                    let slot = set_ref.live_len;
                    let old_slot_bytes = Self::set_slot_bytes(None);
                    set_ref.entries[slot] = Some(value);
                    (slot, old_slot_bytes)
                } else {
                    set_ref.entries.push(Some(value));
                    let slot = set_ref
                        .entries
                        .len()
                        .checked_sub(1)
                        .ok_or_else(|| MustardError::runtime("set entry missing"))?;
                    (slot, 0)
                };
                let old_lookup_bytes = Self::set_lookup_bytes(set_ref);
                set_ref.live_len = set_ref
                    .live_len
                    .checked_add(1)
                    .ok_or_else(|| MustardError::runtime("set size overflow"))?;
                if string_key {
                    set_ref.string_key_live_len = set_ref
                        .string_key_live_len
                        .checked_add(1)
                        .ok_or_else(|| MustardError::runtime("set string-key overflow"))?;
                }
                let lookup_bytes = if set_ref.lookup.is_empty()
                    && set_ref.live_len >= set_ref.lookup_promotion_len()
                {
                    set_ref.rebuild_lookup();
                    Self::set_lookup_bytes(set_ref)
                } else if !set_ref.lookup.is_empty() {
                    set_ref.lookup.insert(index_key, slot);
                    if set_ref.live_len < set_ref.lookup_promotion_len() {
                        set_ref.lookup.clear();
                    }
                    Self::set_lookup_bytes(set_ref)
                } else {
                    0
                };
                let value = set_ref
                    .entries
                    .get(slot)
                    .and_then(|value| value.as_ref())
                    .ok_or_else(|| MustardError::runtime("set entry missing"))?;
                let old_bytes = old_slot_bytes + old_lookup_bytes;
                let new_bytes = Self::set_slot_bytes(Some(value)) + lookup_bytes;
                (old_bytes, new_bytes)
            }
        };
        if old_bytes == 0 && new_bytes == 0 {
            return Ok(());
        }
        self.apply_set_component_delta(set, old_bytes, new_bytes)
    }

    fn trim_trailing_map_builder_slots(&mut self, map: MapKey) -> MustardResult<()> {
        let removed_slots = {
            let map_ref = self
                .maps
                .get_mut(map)
                .ok_or_else(|| MustardError::runtime("map missing"))?;
            let removed_slots = map_ref
                .entries
                .iter()
                .rev()
                .take_while(|entry| entry.is_none())
                .count();
            if removed_slots == 0 {
                return Ok(());
            }
            let next_len = map_ref
                .entries
                .len()
                .checked_sub(removed_slots)
                .ok_or_else(|| MustardError::runtime("map entry underflow"))?;
            map_ref.entries.truncate(next_len);
            removed_slots
        };
        let removed_bytes = removed_slots
            .checked_mul(Self::map_slot_bytes(None))
            .ok_or_else(|| MustardError::runtime("map accounting overflow"))?;
        self.apply_map_component_delta(map, removed_bytes, 0)
    }

    fn trim_trailing_set_builder_slots(&mut self, set: SetKey) -> MustardResult<()> {
        let removed_slots = {
            let set_ref = self
                .sets
                .get_mut(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?;
            let removed_slots = set_ref
                .entries
                .iter()
                .rev()
                .take_while(|entry| entry.is_none())
                .count();
            if removed_slots == 0 {
                return Ok(());
            }
            let next_len = set_ref
                .entries
                .len()
                .checked_sub(removed_slots)
                .ok_or_else(|| MustardError::runtime("set entry underflow"))?;
            set_ref.entries.truncate(next_len);
            removed_slots
        };
        let removed_bytes = removed_slots
            .checked_mul(Self::set_slot_bytes(None))
            .ok_or_else(|| MustardError::runtime("set accounting overflow"))?;
        self.apply_set_component_delta(set, removed_bytes, 0)
    }

    fn set_contains(&self, set: SetKey, value: &Value) -> MustardResult<bool> {
        let set_ref = self
            .sets
            .get(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?;
        let Some(slot) = Self::set_slot_by_value(set_ref, value) else {
            return Ok(false);
        };
        let present = set_ref
            .entries
            .get(slot)
            .is_some_and(|value| value.is_some());
        if !present {
            return Err(MustardError::runtime("set entry missing"));
        }
        Ok(true)
    }

    fn set_delete(&mut self, set: SetKey, value: &Value) -> MustardResult<bool> {
        let slot = {
            let set_ref = self
                .sets
                .get(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?;
            Self::set_slot_by_value(set_ref, value)
        };
        let Some(slot) = slot else {
            return Ok(false);
        };
        let (old_bytes, new_bytes) = {
            let set_ref = self
                .sets
                .get_mut(set)
                .ok_or_else(|| MustardError::runtime("set missing"))?;
            let old_lookup_bytes = Self::set_lookup_bytes(set_ref);
            let entry = set_ref
                .entries
                .get_mut(slot)
                .ok_or_else(|| MustardError::runtime("set entry missing"))?;
            let old_slot_bytes = Self::set_slot_bytes(entry.as_ref());
            let removed_key = entry
                .as_ref()
                .map(CollectionIndexKey::from_value)
                .ok_or_else(|| MustardError::runtime("set entry missing"))?;
            let removed_string_key = entry
                .as_ref()
                .is_some_and(uses_string_heavy_collection_lookup);
            *entry = None;
            set_ref.lookup.swap_remove(&removed_key);
            set_ref.live_len = set_ref
                .live_len
                .checked_sub(1)
                .ok_or_else(|| MustardError::runtime("set size underflow"))?;
            if removed_string_key {
                set_ref.string_key_live_len = set_ref
                    .string_key_live_len
                    .checked_sub(1)
                    .ok_or_else(|| MustardError::runtime("set string-key underflow"))?;
            }
            if !set_ref.lookup.is_empty() && set_ref.live_len < set_ref.lookup_promotion_len() {
                set_ref.lookup.clear();
            }
            let new_bytes = Self::set_slot_bytes(None) + Self::set_lookup_bytes(set_ref);
            (old_slot_bytes + old_lookup_bytes, new_bytes)
        };
        self.apply_set_component_delta(set, old_bytes, new_bytes)?;
        Ok(true)
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
                .map(|value| Self::set_slot_bytes(value.as_ref()))
                .sum::<usize>()
                + Self::set_lookup_bytes(set_ref)
        };
        let set_ref = self
            .sets
            .get_mut(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?;
        set_ref.entries.clear();
        set_ref.lookup.clear();
        set_ref.live_len = 0;
        set_ref.string_key_live_len = 0;
        set_ref.clear_epoch = set_ref.clear_epoch.wrapping_add(1);
        self.apply_set_component_delta(set, removed_bytes, 0)
    }

    pub(crate) fn call_map_get(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "get")?;
        self.record_map_get_call();
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
        self.record_map_set_call();
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
        let observed_clear_epoch = self
            .maps
            .get(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .clear_epoch;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapEntries(MapIteratorState {
                map,
                next_index: 0,
                observed_clear_epoch,
            }),
        )?))
    }

    pub(crate) fn call_map_keys(&mut self, this_value: Value) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "keys")?;
        let observed_clear_epoch = self
            .maps
            .get(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .clear_epoch;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapKeys(MapIteratorState {
                map,
                next_index: 0,
                observed_clear_epoch,
            }),
        )?))
    }

    pub(crate) fn call_map_values(&mut self, this_value: Value) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "values")?;
        let observed_clear_epoch = self
            .maps
            .get(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .clear_epoch;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapValues(MapIteratorState {
                map,
                next_index: 0,
                observed_clear_epoch,
            }),
        )?))
    }

    pub(crate) fn call_map_for_each(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let map = self.map_receiver(this_value, "forEach")?;
        let (callback, this_arg) = self.collection_callback("Map.prototype.forEach", args)?;
        let mut next_index = 0usize;
        let mut observed_clear_epoch = self
            .maps
            .get(map)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .clear_epoch;
        while let Some(entry) =
            self.next_map_entry_from_state(map, &mut next_index, &mut observed_clear_epoch)?
        {
            self.charge_native_helper_work(1)?;
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
        Ok(Value::Undefined)
    }

    pub(crate) fn call_set_add(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "add")?;
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        self.record_set_add_call();
        self.set_add(set, value)?;
        Ok(Value::Set(set))
    }

    pub(crate) fn call_set_has(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "has")?;
        self.record_set_has_call();
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
        let observed_clear_epoch = self
            .sets
            .get(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .clear_epoch;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetEntries(SetIteratorState {
                set,
                next_index: 0,
                observed_clear_epoch,
            }),
        )?))
    }

    pub(crate) fn call_set_keys(&mut self, this_value: Value) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "keys")?;
        let observed_clear_epoch = self
            .sets
            .get(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .clear_epoch;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState {
                set,
                next_index: 0,
                observed_clear_epoch,
            }),
        )?))
    }

    pub(crate) fn call_set_values(&mut self, this_value: Value) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "values")?;
        let observed_clear_epoch = self
            .sets
            .get(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .clear_epoch;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState {
                set,
                next_index: 0,
                observed_clear_epoch,
            }),
        )?))
    }

    pub(crate) fn call_set_for_each(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<Value> {
        let set = self.set_receiver(this_value, "forEach")?;
        let (callback, this_arg) = self.collection_callback("Set.prototype.forEach", args)?;
        let mut next_index = 0usize;
        let mut observed_clear_epoch = self
            .sets
            .get(set)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .clear_epoch;
        while let Some(value) =
            self.next_set_value_from_state(set, &mut next_index, &mut observed_clear_epoch)?
        {
            self.charge_native_helper_work(1)?;
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
        Ok(Value::Undefined)
    }
}
