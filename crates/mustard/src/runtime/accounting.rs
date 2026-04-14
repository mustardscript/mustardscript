use super::*;

impl Runtime {
    pub(super) fn debug_metrics(&self) -> RuntimeDebugMetrics {
        let mut metrics = self.debug_metrics.clone();
        metrics.dynamic_instructions = self.instruction_counter as u64;
        let mut collection_call_sites = self
            .collection_call_sites
            .values()
            .cloned()
            .collect::<Vec<_>>();
        collection_call_sites.sort_by(|left, right| {
            right
                .total_calls()
                .cmp(&left.total_calls())
                .then_with(|| left.span.start.cmp(&right.span.start))
                .then_with(|| left.instruction_offset.cmp(&right.instruction_offset))
                .then_with(|| left.function_name.cmp(&right.function_name))
        });
        metrics.collection_call_sites = collection_call_sites;
        metrics
    }

    pub(super) fn enable_operation_counters(&mut self) {
        self.operation_counters_enabled = true;
    }

    pub(super) fn disable_operation_counters(&mut self) {
        self.operation_counters_enabled = false;
    }

    fn bump_metric(counter: &mut u64) {
        *counter = counter.saturating_add(1);
    }

    fn current_collection_call_site(
        &self,
    ) -> Option<(CollectionCallSiteKey, CollectionCallSiteMetrics)> {
        let frame = self.frames.last()?;
        let instruction_offset = frame.ip.checked_sub(1)?;
        let function = self.program.functions.get(frame.function_id)?;
        let span = match function.code.get(instruction_offset)? {
            Instruction::MapSetCounter { span }
            | Instruction::SetAddDirect { span }
            | Instruction::SetHasDirect { span } => *span,
            Instruction::Call { span, .. } | Instruction::CallWithArray { span, .. } => *span,
            _ => return None,
        };
        Some((
            CollectionCallSiteKey {
                function_id: frame.function_id,
                instruction_offset,
            },
            CollectionCallSiteMetrics {
                function_name: function.name.clone(),
                instruction_offset: instruction_offset as u32,
                span,
                map_get_calls: 0,
                map_set_calls: 0,
                set_add_calls: 0,
                set_has_calls: 0,
            },
        ))
    }

    fn record_collection_call_site<F>(&mut self, bump: F)
    where
        F: FnOnce(&mut CollectionCallSiteMetrics),
    {
        let Some((key, default_metrics)) = self.current_collection_call_site() else {
            return;
        };
        let metrics = self
            .collection_call_sites
            .entry(key)
            .or_insert(default_metrics);
        bump(metrics);
    }

    pub(super) fn record_accounting_refresh(&mut self) {
        self.debug_metrics.accounting_refreshes =
            self.debug_metrics.accounting_refreshes.saturating_add(1);
    }

    pub(super) fn record_static_property_read(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.static_property_reads);
        }
    }

    pub(super) fn record_computed_property_read(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.computed_property_reads);
        }
    }

    pub(super) fn record_property_ic_hit(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.property_ic_hits);
        }
    }

    pub(super) fn record_property_ic_miss(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.property_ic_misses);
        }
    }

    pub(super) fn record_property_ic_deopt(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.property_ic_deopts);
        }
    }

    pub(super) fn record_object_allocation(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.object_allocations);
        }
    }

    pub(super) fn record_array_allocation(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.array_allocations);
        }
    }

    pub(super) fn record_map_get_call(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.map_get_calls);
            self.record_collection_call_site(|metrics| {
                Self::bump_metric(&mut metrics.map_get_calls);
            });
        }
    }

    pub(super) fn record_map_set_call(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.map_set_calls);
            self.record_collection_call_site(|metrics| {
                Self::bump_metric(&mut metrics.map_set_calls);
            });
        }
    }

    pub(super) fn record_set_add_call(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.set_add_calls);
            self.record_collection_call_site(|metrics| {
                Self::bump_metric(&mut metrics.set_add_calls);
            });
        }
    }

    pub(super) fn record_set_has_call(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.set_has_calls);
            self.record_collection_call_site(|metrics| {
                Self::bump_metric(&mut metrics.set_has_calls);
            });
        }
    }

    pub(super) fn record_string_case_conversion(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.string_case_conversions);
        }
    }

    pub(super) fn record_ascii_case_fast_path_hit(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_case_fast_path_hits);
        }
    }

    pub(super) fn record_ascii_case_fast_path_fallback(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_case_fast_path_fallbacks);
        }
    }

    pub(super) fn record_literal_string_search(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.literal_string_searches);
        }
    }

    pub(super) fn record_ascii_substring_fast_path_hit(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_substring_fast_path_hits);
        }
    }

    pub(super) fn record_ascii_substring_fast_path_fallback(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_substring_fast_path_fallbacks);
        }
    }

    pub(super) fn record_regex_search_or_replacement(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.regex_search_or_replacements);
        }
    }

    pub(super) fn record_ascii_token_regex_fast_path_hit(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_token_regex_fast_path_hits);
        }
    }

    pub(super) fn record_ascii_token_regex_fast_path_fallback(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_token_regex_fast_path_fallbacks);
        }
    }

    pub(super) fn record_ascii_cleanup_fast_path_hit(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_cleanup_fast_path_hits);
        }
    }

    pub(super) fn record_ascii_cleanup_fast_path_fallback(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.ascii_cleanup_fast_path_fallbacks);
        }
    }

    pub(super) fn record_comparator_sort_invocation(&mut self) {
        if self.operation_counters_enabled {
            Self::bump_metric(&mut self.debug_metrics.comparator_sort_invocations);
        }
    }

    pub(super) fn queue_microtask(&mut self, job: MicrotaskJob) {
        self.debug_metrics.queued_microtasks =
            self.debug_metrics.queued_microtasks.saturating_add(1);
        match &job {
            MicrotaskJob::ResumeAsync { .. } => {
                self.debug_metrics.queued_resume_async_microtasks = self
                    .debug_metrics
                    .queued_resume_async_microtasks
                    .saturating_add(1);
            }
            MicrotaskJob::PromiseReaction { .. } => {
                self.debug_metrics.queued_promise_reactions = self
                    .debug_metrics
                    .queued_promise_reactions
                    .saturating_add(1);
            }
            MicrotaskJob::PromiseCombinator { .. } => {
                self.debug_metrics.queued_promise_combinators = self
                    .debug_metrics
                    .queued_promise_combinators
                    .saturating_add(1);
            }
        }
        self.microtasks.push_back(job);
        self.debug_metrics.peak_microtask_queue_len = self
            .debug_metrics
            .peak_microtask_queue_len
            .max(self.microtasks.len() as u64);
    }

    pub(super) fn record_executed_microtask(&mut self, job: &MicrotaskJob) {
        self.debug_metrics.executed_microtasks =
            self.debug_metrics.executed_microtasks.saturating_add(1);
        match job {
            MicrotaskJob::ResumeAsync { .. } => {
                self.debug_metrics.executed_resume_async_microtasks = self
                    .debug_metrics
                    .executed_resume_async_microtasks
                    .saturating_add(1);
            }
            MicrotaskJob::PromiseReaction { .. } => {
                self.debug_metrics.executed_promise_reactions = self
                    .debug_metrics
                    .executed_promise_reactions
                    .saturating_add(1);
            }
            MicrotaskJob::PromiseCombinator { .. } => {
                self.debug_metrics.executed_promise_combinators = self
                    .debug_metrics
                    .executed_promise_combinators
                    .saturating_add(1);
            }
        }
    }

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
    pub(super) fn debug_assert_cached_accounting_matches_full_walk(&mut self) {
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

    pub(super) fn map_slot_bytes(entry: Option<&MapEntry>) -> usize {
        measure_map_slot_bytes(entry)
    }

    pub(super) fn set_slot_bytes(value: Option<&Value>) -> usize {
        measure_set_slot_bytes(value)
    }

    pub(super) fn collection_index_entry_bytes(key: &CollectionIndexKey) -> usize {
        measure_collection_index_entry_bytes(key)
    }

    pub(super) fn value_bytes(value: &Value) -> usize {
        extra_value_bytes(value)
    }

    pub(super) fn binding_entry_bytes(name: &str) -> usize {
        measure_binding_entry_bytes(name)
    }

    pub(super) fn cell_value_bytes(value: &Value) -> usize {
        measure_cell_value_bytes(value)
    }

    pub(super) fn promise_awaiters_bytes(count: usize) -> usize {
        measure_promise_awaiters_bytes(count)
    }

    pub(super) fn promise_dependents_bytes(count: usize) -> usize {
        measure_promise_dependents_bytes(count)
    }

    pub(super) fn promise_reaction_bytes(reaction: &PromiseReaction) -> usize {
        measure_promise_reaction_entry_bytes(reaction)
    }

    pub(super) fn promise_state_bytes(state: &PromiseState) -> usize {
        measure_promise_state_bytes(state)
    }

    pub(super) fn promise_driver_bytes(driver: &Option<PromiseDriver>) -> usize {
        measure_promise_driver_bytes(driver)
    }

    pub(super) fn promise_settled_result_bytes(result: &PromiseSettledResult) -> usize {
        measure_promise_settled_result_bytes(result)
    }

    pub(super) fn promise_dynamic_accounted_bytes(promise: &PromiseObject) -> usize {
        let static_bytes = std::mem::size_of::<PromiseObject>();
        debug_assert!(promise.accounted_bytes >= static_bytes);
        promise.accounted_bytes.saturating_sub(static_bytes)
    }

    pub(super) fn promise_driver_accounted_bytes(promise: &PromiseObject) -> usize {
        let non_driver_bytes = Self::promise_state_bytes(&promise.state)
            + Self::promise_awaiters_bytes(promise.awaiters.len())
            + Self::promise_dependents_bytes(promise.dependents.len())
            + measure_promise_reactions_bytes(&promise.reactions);
        let dynamic_bytes = Self::promise_dynamic_accounted_bytes(promise);
        debug_assert!(dynamic_bytes >= non_driver_bytes);
        dynamic_bytes.saturating_sub(non_driver_bytes)
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

    pub(super) fn apply_map_component_delta(
        &mut self,
        key: MapKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .maps
            .get(key)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("map accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("map accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.maps
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("map missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn apply_set_component_delta(
        &mut self,
        key: SetKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .sets
            .get(key)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("set accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("set accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.sets
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("set missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn apply_env_component_delta(
        &mut self,
        key: EnvKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .envs
            .get(key)
            .ok_or_else(|| MustardError::runtime("environment missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("environment accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("environment accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.envs
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("environment missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn apply_cell_component_delta(
        &mut self,
        key: CellKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .cells
            .get(key)
            .ok_or_else(|| MustardError::runtime("binding cell missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("cell accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("cell accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.cells
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("binding cell missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn apply_closure_component_delta(
        &mut self,
        key: ClosureKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .closures
            .get(key)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("closure accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("closure accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.closures
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn apply_promise_component_delta(
        &mut self,
        key: PromiseKey,
        old_component_bytes: usize,
        new_component_bytes: usize,
    ) -> MustardResult<()> {
        let old_bytes = self
            .promises
            .get(key)
            .ok_or_else(|| MustardError::runtime("promise missing"))?
            .accounted_bytes;
        let new_bytes = old_bytes
            .checked_sub(old_component_bytes)
            .ok_or_else(|| MustardError::runtime("promise accounting underflow"))?
            .checked_add(new_component_bytes)
            .ok_or_else(|| MustardError::runtime("promise accounting overflow"))?;
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.promises
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("promise missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    pub(super) fn replace_promise_driver(
        &mut self,
        key: PromiseKey,
        driver: Option<PromiseDriver>,
    ) -> MustardResult<()> {
        let old_driver_bytes = {
            let promise = self
                .promises
                .get(key)
                .ok_or_else(|| MustardError::runtime("promise missing"))?;
            Self::promise_driver_accounted_bytes(promise)
        };
        let new_driver_bytes = Self::promise_driver_bytes(&driver);
        self.promises
            .get_mut(key)
            .ok_or_else(|| MustardError::runtime("promise missing"))?
            .driver = driver;
        self.apply_promise_component_delta(key, old_driver_bytes, new_driver_bytes)
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
        self.record_object_allocation();
        let mut object = PlainObject {
            properties: ObjectProperties::Plain(properties),
            kind,
            accounted_bytes: 0,
        };
        object.accounted_bytes = measure_object_bytes(&object);
        self.account_new_allocation(object.accounted_bytes)?;
        Ok(self.objects.insert(object))
    }

    pub(super) fn insert_shape_backed_object(
        &mut self,
        properties: IndexMap<String, Value>,
    ) -> MustardResult<ObjectKey> {
        self.record_object_allocation();
        let keys = properties.keys().cloned().collect::<Vec<_>>();
        let shape = self.intern_object_shape(keys)?;
        let mut object = PlainObject {
            properties: ObjectProperties::Shaped(ShapedObjectProperties {
                shape,
                slots: properties.into_values().collect(),
            }),
            kind: ObjectKind::Plain,
            accounted_bytes: 0,
        };
        object.accounted_bytes = measure_object_bytes(&object);
        self.account_new_allocation(object.accounted_bytes)?;
        Ok(self.objects.insert(object))
    }

    fn intern_object_shape(&mut self, keys: Vec<String>) -> MustardResult<Arc<SharedObjectShape>> {
        if let Some(shape) = self.object_shapes.get(&keys) {
            return Ok(shape.clone());
        }

        let mut shape = SharedObjectShape::from_keys(self.next_object_shape_id, keys.clone());
        shape.accounted_bytes = measure_shared_object_shape_bytes(&shape);
        self.account_new_allocation(shape.accounted_bytes)?;
        self.next_object_shape_id = self.next_object_shape_id.saturating_add(1);
        let shape = Arc::new(shape);
        self.object_shapes.insert(keys, shape.clone());
        Ok(shape)
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
        self.record_array_allocation();
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
        let mut map = MapObject::from_entries(entries);
        map.accounted_bytes = measure_map_bytes(&map);
        self.account_new_allocation(map.accounted_bytes)?;
        Ok(self.maps.insert(map))
    }

    pub(super) fn insert_map_slots(
        &mut self,
        entries: Vec<Option<MapEntry>>,
    ) -> MustardResult<MapKey> {
        let mut map = MapObject {
            entries,
            live_len: 0,
            string_key_live_len: 0,
            clear_epoch: 0,
            lookup: IndexMap::new(),
            accounted_bytes: 0,
        };
        map.rebuild_lookup();
        map.accounted_bytes = measure_map_bytes(&map);
        self.account_new_allocation(map.accounted_bytes)?;
        Ok(self.maps.insert(map))
    }

    pub(super) fn insert_set(&mut self, entries: Vec<Value>) -> MustardResult<SetKey> {
        let mut set = SetObject::from_entries(entries);
        set.accounted_bytes = measure_set_bytes(&set);
        self.account_new_allocation(set.accounted_bytes)?;
        Ok(self.sets.insert(set))
    }

    pub(super) fn insert_set_slots(
        &mut self,
        entries: Vec<Option<Value>>,
    ) -> MustardResult<SetKey> {
        let mut set = SetObject {
            entries,
            live_len: 0,
            string_key_live_len: 0,
            clear_epoch: 0,
            lookup: IndexMap::new(),
            accounted_bytes: 0,
        };
        set.rebuild_lookup();
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

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn refresh_env_accounting(&mut self, key: EnvKey) -> MustardResult<()> {
        self.record_accounting_refresh();
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

    pub(super) fn refresh_object_accounting(&mut self, key: ObjectKey) -> MustardResult<()> {
        self.record_accounting_refresh();
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

    pub(super) fn enforce_loaded_accounting(&mut self) -> MustardResult<()> {
        if self.accounting_recount_required {
            return self.recompute_accounting_after_load();
        }

        #[cfg(debug_assertions)]
        self.debug_assert_cached_accounting_matches_full_walk();

        self.enforce_loaded_accounting_limits(self.heap_bytes_used, self.allocation_count)
    }

    pub(super) fn recompute_accounting_after_load(&mut self) -> MustardResult<()> {
        self.record_accounting_refresh();
        self.rebuild_object_shape_state();
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

    pub(super) fn recompute_accounting_totals(&mut self) -> Result<(usize, usize), String> {
        let mut heap_bytes_used = 0usize;
        let mut allocation_count = 0usize;

        for shape in self.object_shapes.values() {
            heap_bytes_used = heap_bytes_used
                .checked_add(measure_shared_object_shape_bytes(shape.as_ref()))
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
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
            map.rebuild_lookup();
            map.accounted_bytes = measure_map_bytes(map);
            heap_bytes_used = heap_bytes_used
                .checked_add(map.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for set in self.sets.values_mut() {
            set.rebuild_lookup();
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

    pub(super) fn rebuild_object_shape_state(&mut self) {
        let mut object_shapes: HashMap<Vec<String>, Arc<SharedObjectShape>> = HashMap::new();
        let mut next_object_shape_id = 1u64;
        for object in self.objects.values_mut() {
            let ObjectProperties::Shaped(properties) = &mut object.properties else {
                continue;
            };
            let keys = properties
                .shape
                .property_slots
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            next_object_shape_id = next_object_shape_id.max(properties.shape.id.saturating_add(1));
            if let Some(shape) = object_shapes.get(&keys) {
                properties.shape = shape.clone();
            } else {
                object_shapes.insert(keys, properties.shape.clone());
            }
        }
        self.object_shapes = object_shapes;
        self.next_object_shape_id = next_object_shape_id;
        self.static_property_inline_caches.clear();
    }
}

fn extra_value_bytes(value: &Value) -> usize {
    match value {
        Value::String(value) | Value::HostFunction(value) => value.len(),
        Value::BigInt(value) => value.to_signed_bytes_le().len(),
        _ => 0,
    }
}

fn measure_binding_entry_bytes(name: &str) -> usize {
    std::mem::size_of::<(String, CellKey)>() + name.len()
}

fn measure_cell_value_bytes(value: &Value) -> usize {
    extra_value_bytes(value)
}

fn measure_bindings_bytes(bindings: &IndexMap<String, CellKey>) -> usize {
    bindings
        .keys()
        .map(|key| measure_binding_entry_bytes(key))
        .sum::<usize>()
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

fn measure_object_properties_bytes(properties: &ObjectProperties) -> usize {
    match properties {
        ObjectProperties::Plain(properties) => measure_properties_bytes(properties),
        ObjectProperties::Shaped(properties) => {
            std::mem::size_of::<ShapedObjectProperties>()
                + properties
                    .slots
                    .iter()
                    .map(|value| std::mem::size_of::<Value>() + extra_value_bytes(value))
                    .sum::<usize>()
        }
    }
}

fn measure_shared_object_shape_bytes(shape: &SharedObjectShape) -> usize {
    std::mem::size_of::<SharedObjectShape>()
        + shape
            .property_slots
            .keys()
            .map(|key| std::mem::size_of::<(String, usize)>() + key.len())
            .sum::<usize>()
}

fn measure_env_bytes(env: &Env) -> usize {
    std::mem::size_of::<Env>() + measure_bindings_bytes(&env.bindings)
}

fn measure_cell_bytes(cell: &Cell) -> usize {
    std::mem::size_of::<Cell>() + measure_cell_value_bytes(&cell.value)
}

fn measure_object_bytes(object: &PlainObject) -> usize {
    std::mem::size_of::<PlainObject>()
        + measure_object_properties_bytes(&object.properties)
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

fn extra_collection_index_key_bytes(key: &CollectionIndexKey) -> usize {
    match key {
        CollectionIndexKey::String(value) | CollectionIndexKey::HostFunction(value) => value.len(),
        CollectionIndexKey::BigInt(value) => value.to_signed_bytes_le().len(),
        _ => 0,
    }
}

fn measure_collection_index_entry_bytes(key: &CollectionIndexKey) -> usize {
    std::mem::size_of::<(CollectionIndexKey, usize)>() + extra_collection_index_key_bytes(key)
}

fn measure_map_slot_bytes(entry: Option<&MapEntry>) -> usize {
    std::mem::size_of::<Option<MapEntry>>()
        + entry.map_or(0, |entry| {
            extra_value_bytes(&entry.key) + extra_value_bytes(&entry.value)
        })
}

fn measure_map_bytes(map: &MapObject) -> usize {
    std::mem::size_of::<MapObject>()
        + map
            .entries
            .iter()
            .map(|entry| measure_map_slot_bytes(entry.as_ref()))
            .sum::<usize>()
        + map
            .lookup
            .keys()
            .map(measure_collection_index_entry_bytes)
            .sum::<usize>()
}

fn measure_set_slot_bytes(value: Option<&Value>) -> usize {
    std::mem::size_of::<Option<Value>>() + value.map_or(0, extra_value_bytes)
}

fn measure_set_bytes(set: &SetObject) -> usize {
    std::mem::size_of::<SetObject>()
        + set
            .entries
            .iter()
            .map(|value| measure_set_slot_bytes(value.as_ref()))
            .sum::<usize>()
        + set
            .lookup
            .keys()
            .map(measure_collection_index_entry_bytes)
            .sum::<usize>()
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

fn measure_promise_traceback_bytes(traceback: &[TraceFrameSnapshot]) -> usize {
    traceback
        .iter()
        .map(|frame| frame.function_name.as_ref().map_or(0, String::len))
        .sum::<usize>()
}

fn measure_promise_state_bytes(state: &PromiseState) -> usize {
    match state {
        PromiseState::Pending => 0,
        PromiseState::Fulfilled(value) => extra_value_bytes(value),
        PromiseState::Rejected(rejection) => {
            extra_value_bytes(&rejection.value)
                + measure_promise_traceback_bytes(&rejection.traceback)
        }
    }
}

fn measure_promise_outcome_payload_bytes(outcome: &PromiseOutcome) -> usize {
    match outcome {
        PromiseOutcome::Fulfilled(value) => extra_value_bytes(value),
        PromiseOutcome::Rejected(rejection) => extra_value_bytes(&rejection.value),
    }
}

fn measure_promise_awaiters_bytes(count: usize) -> usize {
    count * std::mem::size_of::<AsyncContinuation>()
}

fn measure_promise_dependents_bytes(count: usize) -> usize {
    count * std::mem::size_of::<PromiseKey>()
}

fn measure_promise_reaction_entry_bytes(reaction: &PromiseReaction) -> usize {
    std::mem::size_of::<PromiseReaction>()
        + match reaction {
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
            } => measure_promise_outcome_payload_bytes(original_outcome),
            PromiseReaction::Combinator { .. } => 0,
        }
}

fn measure_promise_reactions_bytes(reactions: &[PromiseReaction]) -> usize {
    reactions
        .iter()
        .map(measure_promise_reaction_entry_bytes)
        .sum::<usize>()
}

fn measure_promise_settled_result_bytes(result: &PromiseSettledResult) -> usize {
    match result {
        PromiseSettledResult::Fulfilled(value) | PromiseSettledResult::Rejected(value) => {
            extra_value_bytes(value)
        }
    }
}

fn measure_promise_driver_bytes(driver: &Option<PromiseDriver>) -> usize {
    match driver {
        Some(PromiseDriver::Thenable { value }) => extra_value_bytes(value),
        Some(PromiseDriver::All { values, .. }) => values
            .iter()
            .flatten()
            .map(extra_value_bytes)
            .sum::<usize>(),
        Some(PromiseDriver::AllSettled { results, .. }) => results
            .iter()
            .flatten()
            .map(measure_promise_settled_result_bytes)
            .sum::<usize>(),
        Some(PromiseDriver::Any { reasons, .. }) => reasons
            .iter()
            .flatten()
            .map(extra_value_bytes)
            .sum::<usize>(),
        None => 0,
    }
}

fn measure_promise_dynamic_bytes(promise: &PromiseObject) -> usize {
    measure_promise_awaiters_bytes(promise.awaiters.len())
        + measure_promise_dependents_bytes(promise.dependents.len())
        + measure_promise_reactions_bytes(&promise.reactions)
        + measure_promise_state_bytes(&promise.state)
        + measure_promise_driver_bytes(&promise.driver)
}

fn measure_promise_bytes(promise: &PromiseObject) -> usize {
    std::mem::size_of::<PromiseObject>() + measure_promise_dynamic_bytes(promise)
}
