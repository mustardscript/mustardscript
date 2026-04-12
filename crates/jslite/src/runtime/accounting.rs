use super::*;

impl Runtime {
    pub(super) fn ensure_heap_capacity(&self, additional_bytes: usize) -> JsliteResult<()> {
        let next = self
            .heap_bytes_used
            .checked_add(additional_bytes)
            .ok_or_else(|| limit_error("heap limit exceeded"))?;
        if next > self.limits.heap_limit_bytes {
            return Err(limit_error("heap limit exceeded"));
        }
        Ok(())
    }

    pub(super) fn account_new_allocation(&mut self, bytes: usize) -> JsliteResult<()> {
        let next_allocations = self
            .allocation_count
            .checked_add(1)
            .ok_or_else(|| limit_error("allocation budget exhausted"))?;
        if next_allocations > self.limits.allocation_budget {
            return Err(limit_error("allocation budget exhausted"));
        }
        self.ensure_heap_capacity(bytes)?;
        self.allocation_count = next_allocations;
        self.heap_bytes_used += bytes;
        Ok(())
    }

    pub(super) fn apply_heap_delta(
        &mut self,
        old_bytes: usize,
        new_bytes: usize,
    ) -> JsliteResult<()> {
        if new_bytes >= old_bytes {
            self.ensure_heap_capacity(new_bytes - old_bytes)?;
            self.heap_bytes_used += new_bytes - old_bytes;
        } else {
            self.heap_bytes_used -= old_bytes - new_bytes;
        }
        Ok(())
    }

    pub(super) fn insert_env(&mut self, parent: Option<EnvKey>) -> JsliteResult<EnvKey> {
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
    ) -> JsliteResult<CellKey> {
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
    ) -> JsliteResult<ObjectKey> {
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
    ) -> JsliteResult<ArrayKey> {
        self.insert_sparse_array(elements.into_iter().map(Some).collect(), properties)
    }

    pub(super) fn insert_sparse_array(
        &mut self,
        elements: Vec<Option<Value>>,
        properties: IndexMap<String, Value>,
    ) -> JsliteResult<ArrayKey> {
        let mut array = ArrayObject {
            elements,
            properties,
            accounted_bytes: 0,
        };
        array.accounted_bytes = measure_array_bytes(&array);
        self.account_new_allocation(array.accounted_bytes)?;
        Ok(self.arrays.insert(array))
    }

    pub(super) fn insert_map(&mut self, entries: Vec<MapEntry>) -> JsliteResult<MapKey> {
        let mut map = MapObject {
            entries,
            accounted_bytes: 0,
        };
        map.accounted_bytes = measure_map_bytes(&map);
        self.account_new_allocation(map.accounted_bytes)?;
        Ok(self.maps.insert(map))
    }

    pub(super) fn insert_set(&mut self, entries: Vec<Value>) -> JsliteResult<SetKey> {
        let mut set = SetObject {
            entries,
            accounted_bytes: 0,
        };
        set.accounted_bytes = measure_set_bytes(&set);
        self.account_new_allocation(set.accounted_bytes)?;
        Ok(self.sets.insert(set))
    }

    pub(super) fn insert_iterator(&mut self, state: IteratorState) -> JsliteResult<IteratorKey> {
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
    ) -> JsliteResult<ClosureKey> {
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
            .ok_or_else(|| JsliteError::runtime("function not found"))?;
        if !is_arrow {
            let prototype = self.insert_object(
                IndexMap::new(),
                ObjectKind::FunctionPrototype(Value::Closure(key)),
            )?;
            self.closures
                .get_mut(key)
                .ok_or_else(|| JsliteError::runtime("closure missing"))?
                .prototype = Some(prototype);
            self.refresh_closure_accounting(key)?;
        }
        Ok(key)
    }

    pub(super) fn insert_promise(&mut self, state: PromiseState) -> JsliteResult<PromiseKey> {
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

    pub(super) fn account_existing_env(&mut self, key: EnvKey) -> JsliteResult<()> {
        let bytes = {
            let env = self
                .envs
                .get(key)
                .ok_or_else(|| JsliteError::runtime("environment missing"))?;
            measure_env_bytes(env)
        };
        self.account_new_allocation(bytes)?;
        self.envs
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .accounted_bytes = bytes;
        Ok(())
    }

    pub(super) fn refresh_env_accounting(&mut self, key: EnvKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let env = self
                .envs
                .get(key)
                .ok_or_else(|| JsliteError::runtime("environment missing"))?;
            (env.accounted_bytes, measure_env_bytes(env))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.envs
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_cell_accounting(&mut self, key: CellKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let cell = self
                .cells
                .get(key)
                .ok_or_else(|| JsliteError::runtime("binding cell missing"))?;
            (cell.accounted_bytes, measure_cell_bytes(cell))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.cells
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("binding cell missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_object_accounting(&mut self, key: ObjectKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let object = self
                .objects
                .get(key)
                .ok_or_else(|| JsliteError::runtime("object missing"))?;
            (object.accounted_bytes, measure_object_bytes(object))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.objects
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_array_accounting(&mut self, key: ArrayKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let array = self
                .arrays
                .get(key)
                .ok_or_else(|| JsliteError::runtime("array missing"))?;
            (array.accounted_bytes, measure_array_bytes(array))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.arrays
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_map_accounting(&mut self, key: MapKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let map = self
                .maps
                .get(key)
                .ok_or_else(|| JsliteError::runtime("map missing"))?;
            (map.accounted_bytes, measure_map_bytes(map))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.maps
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_set_accounting(&mut self, key: SetKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let set = self
                .sets
                .get(key)
                .ok_or_else(|| JsliteError::runtime("set missing"))?;
            (set.accounted_bytes, measure_set_bytes(set))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.sets
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_iterator_accounting(&mut self, key: IteratorKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let iterator = self
                .iterators
                .get(key)
                .ok_or_else(|| JsliteError::runtime("iterator missing"))?;
            (iterator.accounted_bytes, measure_iterator_bytes(iterator))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.iterators
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("iterator missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn refresh_closure_accounting(&mut self, key: ClosureKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let closure = self
                .closures
                .get(key)
                .ok_or_else(|| JsliteError::runtime("closure missing"))?;
            (closure.accounted_bytes, measure_closure_bytes(closure))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.closures
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("closure missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn recompute_accounting_after_load(&mut self) -> JsliteResult<()> {
        let (heap_bytes_used, allocation_count) =
            self.recompute_accounting_totals().map_err(|message| {
                serialization_error(format!("snapshot validation failed: {message}"))
            })?;

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

        self.heap_bytes_used = heap_bytes_used;
        self.allocation_count = allocation_count;
        Ok(())
    }

    pub(super) fn refresh_promise_accounting(&mut self, key: PromiseKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let promise = self
                .promises
                .get(key)
                .ok_or_else(|| JsliteError::runtime("promise missing"))?;
            (promise.accounted_bytes, measure_promise_bytes(promise))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.promises
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
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

fn measure_properties_bytes(properties: &IndexMap<String, Value>) -> usize {
    properties.len() * std::mem::size_of::<(String, Value)>()
        + properties
            .iter()
            .map(|(key, value)| key.len() + extra_value_bytes(value))
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
        + array.elements.len() * std::mem::size_of::<Option<Value>>()
        + array
            .elements
            .iter()
            .filter_map(Option::as_ref)
            .map(extra_value_bytes)
            .sum::<usize>()
        + measure_properties_bytes(&array.properties)
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
