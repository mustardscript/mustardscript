use super::*;

impl Runtime {
    fn enforce_loaded_accounting_limits(
        &self,
        heap_bytes_used: usize,
        allocation_count: usize,
    ) -> MustardResult<()> {
        if heap_bytes_used > self.limits.heap_limit_bytes {
            return Err(serialization_error(
                "snapshot validation failed: heap usage exceeds configured heap limit",
            ));
        }
        if allocation_count > self.limits.allocation_budget {
            return Err(serialization_error(
                "snapshot validation failed: allocation count exceeds configured allocation budget",
            ));
        }
        Ok(())
    }

    #[cfg(debug_assertions)]
    fn debug_assert_cached_accounting_matches_full_walk(&mut self) {
        let cached_totals = (self.heap_bytes_used, self.allocation_count);
        let measured_totals = self
            .recompute_accounting_totals()
            .unwrap_or_else(|message| panic!("cached accounting recompute failed: {message}"));
        debug_assert_eq!(
            cached_totals, measured_totals,
            "cached runtime accounting diverged from a full heap walk",
        );
    }

    fn growth_fits(
        &self,
        additional_bytes: usize,
        additional_allocations: usize,
    ) -> MustardResult<bool> {
        let next_allocations = self
            .allocation_count
            .checked_add(additional_allocations)
            .ok_or_else(|| limit_error("allocation budget exhausted"))?;
        if next_allocations > self.limits.allocation_budget {
            return Ok(false);
        }

        let next_heap = self
            .heap_bytes_used
            .checked_add(additional_bytes)
            .ok_or_else(|| limit_error("heap limit exceeded"))?;
        Ok(next_heap <= self.limits.heap_limit_bytes)
    }

    fn enforce_growth_limits(
        &self,
        additional_bytes: usize,
        additional_allocations: usize,
    ) -> MustardResult<()> {
        let next_allocations = self
            .allocation_count
            .checked_add(additional_allocations)
            .ok_or_else(|| limit_error("allocation budget exhausted"))?;
        if next_allocations > self.limits.allocation_budget {
            return Err(limit_error("allocation budget exhausted"));
        }

        let next_heap = self
            .heap_bytes_used
            .checked_add(additional_bytes)
            .ok_or_else(|| limit_error("heap limit exceeded"))?;
        if next_heap > self.limits.heap_limit_bytes {
            return Err(limit_error("heap limit exceeded"));
        }
        Ok(())
    }

    pub(super) fn prepare_for_growth(
        &mut self,
        additional_bytes: usize,
        additional_allocations: usize,
    ) -> MustardResult<()> {
        if self.growth_fits(additional_bytes, additional_allocations)? {
            return Ok(());
        }

        self.collect_garbage()?;

        if self.growth_fits(additional_bytes, additional_allocations)? {
            return Ok(());
        }

        self.enforce_growth_limits(additional_bytes, additional_allocations)
    }

    pub(super) fn ensure_heap_capacity(&mut self, additional_bytes: usize) -> MustardResult<()> {
        self.prepare_for_growth(additional_bytes, 0)
    }

    pub(super) fn account_new_allocation(&mut self, bytes: usize) -> MustardResult<()> {
        self.prepare_for_growth(bytes, 1)?;
        self.allocation_count += 1;
        self.heap_bytes_used += bytes;
        self.record_gc_growth(bytes, 1);
        Ok(())
    }

    pub(super) fn apply_heap_delta(
        &mut self,
        old_bytes: usize,
        new_bytes: usize,
    ) -> MustardResult<()> {
        if new_bytes >= old_bytes {
            let added_bytes = new_bytes - old_bytes;
            self.prepare_for_growth(added_bytes, 0)?;
            self.heap_bytes_used += added_bytes;
            self.record_gc_growth(added_bytes, 0);
        } else {
            self.heap_bytes_used -= old_bytes - new_bytes;
        }
        Ok(())
    }

    pub(super) fn property_entry_bytes(key: &str, value: &Value) -> usize {
        measure_property_entry_bytes(key, value)
    }

    pub(super) fn array_slot_bytes(value: Option<&Value>) -> usize {
        measure_array_slot_bytes(value)
    }

    pub(super) fn apply_object_component_delta(
        &mut self,
        key: ObjectKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .objects
            .get(key)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("object accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("object accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.objects
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn apply_array_component_delta(
        &mut self,
        key: ArrayKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .arrays
            .get(key)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("array accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("array accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.arrays
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn push_array_element(
        &mut self,
        key: ArrayKey,
        value: Option<Value>,
    ) -> MustardResult<()> {
        let added_bytes = Self::array_slot_bytes(value.as_ref());
        self.arrays
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .push(value);
        self.apply_array_component_delta(key, 0, added_bytes)
    }

    pub(super) fn extend_array_elements<I>(&mut self, key: ArrayKey, values: I) -> MustardResult<()>
    where
        I: IntoIterator<Item = Option<Value>>,
    {
        let values: Vec<_> = values.into_iter().collect();
        let added_bytes = values
            .iter()
            .map(|value| Self::array_slot_bytes(value.as_ref()))
            .sum::<usize>();
        self.arrays
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements
            .extend(values);
        self.apply_array_component_delta(key, 0, added_bytes)
    }

    pub(super) fn pop_array_element(&mut self, key: ArrayKey) -> MustardResult<Option<Value>> {
        let (removed, removed_bytes) = {
            let array = self
                .arrays
                .get_mut(key)
                .ok_or_else(|| MustardError::runtime("array missing"))?;
            let removed_bytes = array
                .elements
                .last()
                .map(|value| Self::array_slot_bytes(value.as_ref()))
                .unwrap_or(0);
            let removed = array.elements.pop().flatten();
            (removed, removed_bytes)
        };
        if removed_bytes != 0 {
            self.apply_array_component_delta(key, removed_bytes, 0)?;
        }
        Ok(removed)
    }

    pub(super) fn set_array_element_at(
        &mut self,
        key: ArrayKey,
        index: usize,
        value: Value,
    ) -> MustardResult<()> {
        let empty_slot_bytes = Self::array_slot_bytes(None);
        let new_slot_bytes = Self::array_slot_bytes(Some(&value));
        let (old_component_bytes, new_component_bytes) = {
            let array = self
                .arrays
                .get_mut(key)
                .ok_or_else(|| MustardError::runtime("array missing"))?;
            let old_len = array.elements.len();
            let old_component_bytes = if index < old_len {
                Self::array_slot_bytes(array.elements[index].as_ref())
            } else {
                0
            };
            if index >= old_len {
                array.elements.resize(index + 1, None);
            }
            array.elements[index] = Some(value);
            let new_component_bytes = if index < old_len {
                new_slot_bytes
            } else {
                let added_slots = index + 1 - old_len;
                added_slots
                    .saturating_sub(1)
                    .checked_mul(empty_slot_bytes)
                    .and_then(|bytes| bytes.checked_add(new_slot_bytes))
                    .ok_or_else(|| MustardError::runtime("array accounting overflow"))?
            };
            (old_component_bytes, new_component_bytes)
        };
        self.apply_array_component_delta(key, old_component_bytes, new_component_bytes)
    }

    pub(super) fn insert_env(&mut self, parent: Option<EnvKey>) -> MustardResult<EnvKey> {
        let mut env = Env {
            parent,
            bindings: IndexMap::new(),
            accounted_bytes: 0,
        };
        env.accounted_bytes = measure_env_bytes(&env);
        self.account_new_allocation(env.accounted_bytes)?;
        Ok(self.envs.insert(env))
    }

    pub(super) fn insert_cell(
        &mut self,
        value: Value,
        mutable: bool,
        initialized: bool,
    ) -> MustardResult<CellKey> {
        let mut cell = Cell {
            value,
            mutable,
            initialized,
            accounted_bytes: 0,
        };
        cell.accounted_bytes = measure_cell_bytes(&cell);
        self.account_new_allocation(cell.accounted_bytes)?;
        Ok(self.cells.insert(cell))
    }

    pub(super) fn insert_object(
        &mut self,
        properties: IndexMap<String, Value>,
        kind: ObjectKind,
    ) -> MustardResult<ObjectKey> {
        let mut object = PlainObject {
            properties,
            kind,
            accounted_bytes: 0,
        };
        object.accounted_bytes = measure_object_bytes(&object);
        self.account_new_allocation(object.accounted_bytes)?;
        Ok(self.objects.insert(object))
    }

    pub(super) fn insert_array(
        &mut self,
        elements: Vec<Value>,
        properties: IndexMap<String, Value>,
    ) -> MustardResult<ArrayKey> {
        self.insert_sparse_array(elements.into_iter().map(Some).collect(), properties)
    }

    pub(super) fn insert_sparse_array(
        &mut self,
        elements: Vec<Option<Value>>,
        properties: IndexMap<String, Value>,
    ) -> MustardResult<ArrayKey> {
        let mut array = ArrayObject {
            elements,
            properties,
            accounted_bytes: 0,
        };
        array.accounted_bytes = measure_array_bytes(&array);
        self.account_new_allocation(array.accounted_bytes)?;
        Ok(self.arrays.insert(array))
    }

    pub(super) fn insert_map(&mut self, entries: Vec<MapEntry>) -> MustardResult<MapKey> {
        let mut map = MapObject {
            entries,
            accounted_bytes: 0,
        };
        map.accounted_bytes = measure_map_bytes(&map);
        self.account_new_allocation(map.accounted_bytes)?;
        Ok(self.maps.insert(map))
    }

    pub(super) fn insert_set(&mut self, entries: Vec<Value>) -> MustardResult<SetKey> {
        let mut set = SetObject {
            entries,
            accounted_bytes: 0,
        };
        set.accounted_bytes = measure_set_bytes(&set);
        self.account_new_allocation(set.accounted_bytes)?;
        Ok(self.sets.insert(set))
    }

    pub(super) fn insert_iterator(&mut self, state: IteratorState) -> MustardResult<IteratorKey> {
        let mut iterator = IteratorObject {
            state,
            accounted_bytes: 0,
        };
        iterator.accounted_bytes = measure_iterator_bytes(&iterator);
        self.account_new_allocation(iterator.accounted_bytes)?;
        Ok(self.iterators.insert(iterator))
    }

    pub(super) fn insert_closure(
        &mut self,
        function_id: usize,
        env: EnvKey,
        this_value: Value,
    ) -> MustardResult<ClosureKey> {
        let mut closure = Closure {
            function_id,
            env,
            name: self
                .program
                .functions
                .get(function_id)
                .and_then(|function| function.name.clone()),
            this_value,
            prototype: None,
            properties: IndexMap::new(),
            accounted_bytes: 0,
        };
        closure.accounted_bytes = measure_closure_bytes(&closure);
        self.account_new_allocation(closure.accounted_bytes)?;
        let key = self.closures.insert(closure);
        let is_arrow = self
            .program
            .functions
            .get(function_id)
            .map(|function| function.is_arrow)
            .ok_or_else(|| MustardError::runtime("function not found"))?;
        if !is_arrow {
            let prototype = self.insert_object(
                IndexMap::new(),
                ObjectKind::FunctionPrototype(Value::Closure(key)),
            )?;
            self.closures
                .get_mut(key)
                .ok_or_else(|| MustardError::runtime("closure missing"))?
                .prototype = Some(prototype);
            self.refresh_closure_accounting(key)?;
        }
        Ok(key)
    }

    pub(super) fn insert_promise(&mut self, state: PromiseState) -> MustardResult<PromiseKey> {
        let mut promise = PromiseObject {
            state,
            awaiters: Vec::new(),
            dependents: Vec::new(),
            reactions: Vec::new(),
            driver: None,
            accounted_bytes: 0,
        };
        promise.accounted_bytes = measure_promise_bytes(&promise);
        self.account_new_allocation(promise.accounted_bytes)?;
        Ok(self.promises.insert(promise))
    }

    pub(super) fn account_existing_env(&mut self, key: EnvKey) -> MustardResult<()> {
        let bytes = {
            let env = self
                .envs
                .get(key)
                .ok_or_else(|| MustardError::runtime("environment missing"))?;
            measure_env_bytes(env)
        };
        self.account_new_allocation(bytes)?;
        self.envs
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("environment missing"))?
            .accounted_bytes = bytes;
        Ok(())
    }

    pub(super) fn refresh_env_accounting(&mut self, key: EnvKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let env = self
                .envs
                .get(key)
                .ok_or_else(|| MustardError::runtime("environment missing"))?;
            (env.accounted_bytes, measure_env_bytes(env))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.envs
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("environment missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_cell_accounting(&mut self, key: CellKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let cell = self
                .cells
                .get(key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
            (cell.accounted_bytes, measure_cell_bytes(cell))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.cells
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("binding cell missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_object_accounting(&mut self, key: ObjectKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let object = self
                .objects
                .get(key)
                .ok_or_else(|| MustardError::runtime("object missing"))?;
            (object.accounted_bytes, measure_object_bytes(object))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.objects
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("object missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_array_accounting(&mut self, key: ArrayKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let array = self
                .arrays
                .get(key)
                .ok_or_else(|| MustardError::runtime("array missing"))?;
            (array.accounted_bytes, measure_array_bytes(array))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.arrays
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_map_accounting(&mut self, key: MapKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let map = self
                .maps
                .get(key)
                .ok_or_else(|| MustardError::runtime("map missing"))?;
            (map.accounted_bytes, measure_map_bytes(map))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.maps
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_set_accounting(&mut self, key: SetKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let set = self
                .sets
                .get(key)
                .ok_or_else(|| MustardError::runtime("set missing"))?;
            (set.accounted_bytes, measure_set_bytes(set))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.sets
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_iterator_accounting(&mut self, key: IteratorKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let iterator = self
                .iterators
                .get(key)
                .ok_or_else(|| MustardError::runtime("iterator missing"))?;
            (iterator.accounted_bytes, measure_iterator_bytes(iterator))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.iterators
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("iterator missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_closure_accounting(&mut self, key: ClosureKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let closure = self
                .closures
                .get(key)
                .ok_or_else(|| MustardError::runtime("closure missing"))?;
            (closure.accounted_bytes, measure_closure_bytes(closure))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.closures
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn enforce_loaded_accounting(&mut self) -> MustardResult<()> {
        if self.accounting_recount_required {
            return self.recompute_accounting_after_load();
        }

        #[cfg(debug_assertions)]
        self.debug_assert_cached_accounting_matches_full_walk();

        self.enforce_loaded_accounting_limits(self.heap_bytes_used, self.allocation_count)
    }

    pub(super) fn recompute_accounting_after_load(&mut self) -> MustardResult<()> {
        let (heap_bytes_used, allocation_count) =
            self.recompute_accounting_totals().map_err(|message| {
                serialization_error(format!("snapshot validation failed: {message}"))
            })?;

        self.enforce_loaded_accounting_limits(heap_bytes_used, allocation_count)?;
        self.heap_bytes_used = heap_bytes_used;
        self.allocation_count = allocation_count;
        self.accounting_recount_required = false;
        Ok(())
    }

    pub(super) fn refresh_promise_accounting(&mut self, key: PromiseKey) -> MustardResult<()> {
        let (old_bytes, new_bytes) = {
            let promise = self
                .promises
                .get(key)
                .ok_or_else(|| MustardError::runtime("promise missing"))?;
            (promise.accounted_bytes, measure_promise_bytes(promise))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.promises
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("promise missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn recompute_accounting_totals(&mut self) -> Result<(usize, usize), String> {
        let mut heap_bytes_used = 0usize;
        let mut allocation_count = 0usize;

        for env in self.envs.values_mut() {
            env.accounted_bytes = measure_env_bytes(env);
            heap_bytes_used = heap_bytes_used
                .checked_add(env.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for cell in self.cells.values_mut() {
            cell.accounted_bytes = measure_cell_bytes(cell);
            heap_bytes_used = heap_bytes_used
                .checked_add(cell.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for object in self.objects.values_mut() {
            object.accounted_bytes = measure_object_bytes(object);
            heap_bytes_used = heap_bytes_used
                .checked_add(object.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for array in self.arrays.values_mut() {
            array.accounted_bytes = measure_array_bytes(array);
            heap_bytes_used = heap_bytes_used
                .checked_add(array.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for map in self.maps.values_mut() {
            map.accounted_bytes = measure_map_bytes(map);
            heap_bytes_used = heap_bytes_used
                .checked_add(map.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for set in self.sets.values_mut() {
            set.accounted_bytes = measure_set_bytes(set);
            heap_bytes_used = heap_bytes_used
                .checked_add(set.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for iterator in self.iterators.values_mut() {
            iterator.accounted_bytes = measure_iterator_bytes(iterator);
            heap_bytes_used = heap_bytes_used
                .checked_add(iterator.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for closure in self.closures.values_mut() {
            closure.accounted_bytes = measure_closure_bytes(closure);
            heap_bytes_used = heap_bytes_used
                .checked_add(closure.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for promise in self.promises.values_mut() {
            promise.accounted_bytes = measure_promise_bytes(promise);
            heap_bytes_used = heap_bytes_used
                .checked_add(promise.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }

        Ok((heap_bytes_used, allocation_count))
    }
}

fn extra_value_bytes(value: &Value) -> usize {
    match value {
        Value::String(value) | Value::HostFunction(value) => value.len(),
        Value::BigInt(value) => value.to_signed_bytes_le().len(),
        _ => 0,
    }
}

fn measure_bindings_bytes(bindings: &IndexMap<String, CellKey>) -> usize {
    bindings.len() * std::mem::size_of::<(String, CellKey)>()
        + bindings.keys().map(|key| key.len()).sum::<usize>()
}

fn measure_property_entry_bytes(key: &str, value: &Value) -> usize {
    std::mem::size_of::<(String, Value)>() + key.len() + extra_value_bytes(value)
}

fn measure_properties_bytes(properties: &IndexMap<String, Value>) -> usize {
    properties
        .iter()
        .map(|(key, value)| measure_property_entry_bytes(key, value))
        .sum::<usize>()
}

fn measure_env_bytes(env: &Env) -> usize {
    std::mem::size_of::<Env>() + measure_bindings_bytes(&env.bindings)
}

fn measure_cell_bytes(cell: &Cell) -> usize {
    std::mem::size_of::<Cell>() + extra_value_bytes(&cell.value)
}

fn measure_object_bytes(object: &PlainObject) -> usize {
    std::mem::size_of::<PlainObject>()
        + measure_properties_bytes(&object.properties)
        + match &object.kind {
            ObjectKind::FunctionPrototype(constructor) => extra_value_bytes(constructor),
            ObjectKind::BoundFunction(bound) => {
                extra_value_bytes(&bound.target)
                    + extra_value_bytes(&bound.this_value)
                    + bound.args.iter().map(extra_value_bytes).sum::<usize>()
            }
            ObjectKind::Error(name) => name.len(),
            ObjectKind::RegExp(regex) => regex.pattern.len() + regex.flags.len(),
            ObjectKind::StringObject(value) => value.len(),
            _ => 0,
        }
}

fn measure_array_bytes(array: &ArrayObject) -> usize {
    std::mem::size_of::<ArrayObject>()
        + array
            .elements
            .iter()
            .map(|value| measure_array_slot_bytes(value.as_ref()))
            .sum::<usize>()
        + measure_properties_bytes(&array.properties)
}

fn measure_array_slot_bytes(value: Option<&Value>) -> usize {
    std::mem::size_of::<Option<Value>>() + value.map_or(0, extra_value_bytes)
}

fn measure_map_bytes(map: &MapObject) -> usize {
    std::mem::size_of::<MapObject>()
        + map.entries.len() * std::mem::size_of::<MapEntry>()
        + map
            .entries
            .iter()
            .map(|entry| extra_value_bytes(&entry.key) + extra_value_bytes(&entry.value))
            .sum::<usize>()
}

fn measure_set_bytes(set: &SetObject) -> usize {
    std::mem::size_of::<SetObject>()
        + set.entries.len() * std::mem::size_of::<Value>()
        + set.entries.iter().map(extra_value_bytes).sum::<usize>()
}

fn measure_iterator_bytes(iterator: &IteratorObject) -> usize {
    let state_bytes = match &iterator.state {
        IteratorState::String(state) => state.value.len(),
        IteratorState::Array(_)
        | IteratorState::ArrayKeys(_)
        | IteratorState::ArrayEntries(_)
        | IteratorState::MapEntries(_)
        | IteratorState::MapKeys(_)
        | IteratorState::MapValues(_)
        | IteratorState::SetEntries(_)
        | IteratorState::SetValues(_) => 0,
    };
    std::mem::size_of::<IteratorObject>() + state_bytes
}

fn measure_closure_bytes(closure: &Closure) -> usize {
    std::mem::size_of::<Closure>()
        + closure.name.as_ref().map_or(0, String::len)
        + extra_value_bytes(&closure.this_value)
        + measure_properties_bytes(&closure.properties)
}

fn measure_promise_bytes(promise: &PromiseObject) -> usize {
    let state_bytes = match &promise.state {
        PromiseState::Pending => 0,
        PromiseState::Fulfilled(value) => extra_value_bytes(value),
        PromiseState::Rejected(rejection) => {
            extra_value_bytes(&rejection.value)
                + rejection
                    .traceback
                    .iter()
                    .map(|frame| frame.function_name.as_ref().map_or(0, String::len))
                    .sum::<usize>()
        }
    };
    let reaction_bytes = promise
        .reactions
        .iter()
        .map(|reaction| match reaction {
            PromiseReaction::Then {
                on_fulfilled,
                on_rejected,
                ..
            } => on_fulfilled
                .iter()
                .chain(on_rejected.iter())
                .map(extra_value_bytes)
                .sum::<usize>(),
            PromiseReaction::Finally { callback, .. } => {
                callback.iter().map(extra_value_bytes).sum::<usize>()
            }
            PromiseReaction::FinallyPassThrough {
                original_outcome, ..
            } => match original_outcome {
                PromiseOutcome::Fulfilled(value) => extra_value_bytes(value),
                PromiseOutcome::Rejected(rejection) => extra_value_bytes(&rejection.value),
            },
            PromiseReaction::Combinator { .. } => 0,
        })
        .sum::<usize>();
    let driver_bytes = match &promise.driver {
        Some(PromiseDriver::Thenable { value }) => extra_value_bytes(value),
        Some(PromiseDriver::All { values, .. }) => values
            .iter()
            .flatten()
            .map(extra_value_bytes)
            .sum::<usize>(),
        Some(PromiseDriver::AllSettled { results, .. }) => results
            .iter()
            .flatten()
            .map(|result| match result {
                PromiseSettledResult::Fulfilled(value) | PromiseSettledResult::Rejected(value) => {
                    extra_value_bytes(value)
                }
            })
            .sum::<usize>(),
        Some(PromiseDriver::Any { reasons, .. }) => reasons
            .iter()
            .flatten()
            .map(extra_value_bytes)
            .sum::<usize>(),
        None => 0,
    };
    std::mem::size_of::<PromiseObject>()
        + promise.awaiters.len() * std::mem::size_of::<AsyncContinuation>()
        + promise.dependents.len() * std::mem::size_of::<PromiseKey>()
        + promise.reactions.len() * std::mem::size_of::<PromiseReaction>()
        + state_bytes
        + reaction_bytes
        + driver_bytes
}
