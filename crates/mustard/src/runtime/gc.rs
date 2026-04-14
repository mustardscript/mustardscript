use super::*;

const GC_PRESSURE_NUMERATOR: usize = 7;
const GC_PRESSURE_DENOMINATOR: usize = 8;
const MIN_GC_DEBT_BYTES: usize = 4 * 1024;
const MAX_GC_DEBT_BYTES: usize = 256 * 1024;
const MIN_GC_DEBT_ALLOCATIONS: usize = 32;
const MAX_GC_DEBT_ALLOCATIONS: usize = 2_048;

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn mustard_now_millis() -> f64;
}

#[cfg(target_arch = "wasm32")]
fn gc_started_at() -> f64 {
    // `Instant::now()` traps on `wasm32-unknown-unknown`, so the browser build
    // measures GC wall time through the existing host-provided millisecond clock.
    unsafe { mustard_now_millis() }
}

#[cfg(target_arch = "wasm32")]
fn gc_elapsed(started_at: f64) -> std::time::Duration {
    let elapsed_ms = unsafe { mustard_now_millis() } - started_at;
    if !elapsed_ms.is_finite() || elapsed_ms <= 0.0 {
        return std::time::Duration::ZERO;
    }
    let elapsed_ns = (elapsed_ms * 1_000_000.0).round().min(u64::MAX as f64) as u64;
    std::time::Duration::from_nanos(elapsed_ns)
}

#[cfg(not(target_arch = "wasm32"))]
fn gc_started_at() -> std::time::Instant {
    std::time::Instant::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn gc_elapsed(started_at: std::time::Instant) -> std::time::Duration {
    started_at.elapsed()
}

impl Runtime {
    fn record_gc_collection(
        &mut self,
        stats: GarbageCollectionStats,
        elapsed: std::time::Duration,
    ) {
        self.debug_metrics.gc_collections = self.debug_metrics.gc_collections.saturating_add(1);
        self.debug_metrics.gc_total_time_ns = self
            .debug_metrics
            .gc_total_time_ns
            .saturating_add(u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX));
        self.debug_metrics.gc_reclaimed_bytes = self
            .debug_metrics
            .gc_reclaimed_bytes
            .saturating_add(u64::try_from(stats.reclaimed_bytes).unwrap_or(u64::MAX));
        self.debug_metrics.gc_reclaimed_allocations = self
            .debug_metrics
            .gc_reclaimed_allocations
            .saturating_add(u64::try_from(stats.reclaimed_allocations).unwrap_or(u64::MAX));
    }

    pub(super) fn reset_gc_debt(&mut self) {
        self.gc_allocation_debt_bytes = 0;
        self.gc_allocation_debt_count = 0;
    }

    pub(super) fn record_gc_growth(&mut self, bytes: usize, allocations: usize) {
        self.gc_allocation_debt_bytes = self.gc_allocation_debt_bytes.saturating_add(bytes);
        self.gc_allocation_debt_count = self.gc_allocation_debt_count.saturating_add(allocations);
    }

    fn gc_debt_byte_threshold(&self) -> usize {
        scaled_gc_threshold(
            self.limits.heap_limit_bytes,
            32,
            MIN_GC_DEBT_BYTES,
            MAX_GC_DEBT_BYTES,
        )
    }

    fn gc_debt_allocation_threshold(&self) -> usize {
        scaled_gc_threshold(
            self.limits.allocation_budget,
            32,
            MIN_GC_DEBT_ALLOCATIONS,
            MAX_GC_DEBT_ALLOCATIONS,
        )
    }

    fn should_collect_before_allocating(&self) -> bool {
        self.gc_allocation_debt_bytes >= self.gc_debt_byte_threshold()
            || self.gc_allocation_debt_count >= self.gc_debt_allocation_threshold()
            || budget_is_under_pressure(self.heap_bytes_used, self.limits.heap_limit_bytes)
            || budget_is_under_pressure(self.allocation_count, self.limits.allocation_budget)
    }

    pub(super) fn collect_garbage_before_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> MustardResult<()> {
        if instruction_may_allocate(instruction) && self.should_collect_before_allocating() {
            self.collect_garbage()?;
        }
        Ok(())
    }

    pub(super) fn collect_garbage(&mut self) -> MustardResult<GarbageCollectionStats> {
        let started = gc_started_at();
        let baseline_bytes = self.heap_bytes_used;
        let baseline_allocations = self.allocation_count;
        let marks = self.mark_reachable_heap()?;
        let mut stats = GarbageCollectionStats::default();

        self.sweep_unreachable_envs(&marks, &mut stats)?;
        self.sweep_unreachable_cells(&marks, &mut stats)?;
        self.sweep_unreachable_objects(&marks, &mut stats)?;
        self.sweep_unreachable_arrays(&marks, &mut stats)?;
        self.sweep_unreachable_maps(&marks, &mut stats)?;
        self.sweep_unreachable_sets(&marks, &mut stats)?;
        self.sweep_unreachable_iterators(&marks, &mut stats)?;
        self.sweep_unreachable_closures(&marks, &mut stats)?;
        self.sweep_unreachable_promises(&marks, &mut stats)?;

        self.heap_bytes_used = baseline_bytes
            .checked_sub(stats.reclaimed_bytes)
            .ok_or_else(|| MustardError::runtime("gc heap accounting underflow"))?;
        self.allocation_count = baseline_allocations
            .checked_sub(stats.reclaimed_allocations)
            .ok_or_else(|| MustardError::runtime("gc allocation accounting underflow"))?;

        #[cfg(debug_assertions)]
        self.debug_assert_cached_accounting_matches_full_walk();

        self.record_gc_collection(stats, gc_elapsed(started));
        self.reset_gc_debt();
        Ok(stats)
    }

    pub(super) fn mark_reachable_heap(&self) -> MustardResult<GarbageCollectionMarks> {
        let mut marks = GarbageCollectionMarks::default();
        let mut worklist = GarbageCollectionWorklist::default();

        self.mark_env(self.globals, &mut marks, &mut worklist);
        for prototype in self.builtin_prototypes.values() {
            self.mark_value(&Value::Object(*prototype), &mut marks, &mut worklist);
        }
        for object in self.builtin_function_objects.values() {
            self.mark_value(&Value::Object(*object), &mut marks, &mut worklist);
        }
        for object in self.host_function_objects.values() {
            self.mark_value(&Value::Object(*object), &mut marks, &mut worklist);
        }
        if let Some(root_result) = &self.root_result {
            self.mark_value(root_result, &mut marks, &mut worklist);
        }
        for frame in &self.frames {
            self.mark_frame_roots(frame, &mut marks, &mut worklist);
        }
        for job in &self.microtasks {
            match job {
                MicrotaskJob::ResumeAsync {
                    continuation,
                    source,
                } => {
                    for frame in &continuation.frames {
                        self.mark_frame_roots(frame, &mut marks, &mut worklist);
                    }
                    self.mark_promise(*source, &mut marks, &mut worklist);
                }
                MicrotaskJob::PromiseReaction { reaction, source } => {
                    self.mark_promise(
                        self.promise_reaction_target(reaction),
                        &mut marks,
                        &mut worklist,
                    );
                    self.mark_promise(*source, &mut marks, &mut worklist);
                    match reaction {
                        PromiseReaction::Then {
                            on_fulfilled,
                            on_rejected,
                            ..
                        } => {
                            if let Some(handler) = on_fulfilled {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                            if let Some(handler) = on_rejected {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::Finally { callback, .. } => {
                            if let Some(callback) = callback {
                                self.mark_value(callback, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::FinallyPassThrough {
                            original_outcome, ..
                        } => match original_outcome {
                            PromiseOutcome::Fulfilled(value) => {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                            PromiseOutcome::Rejected(rejection) => {
                                self.mark_value(&rejection.value, &mut marks, &mut worklist);
                            }
                        },
                        PromiseReaction::Combinator { .. } => {}
                    }
                }
                MicrotaskJob::PromiseCombinator { target, input, .. } => {
                    self.mark_promise(*target, &mut marks, &mut worklist);
                    match input {
                        PromiseCombinatorInput::Promise(source) => {
                            self.mark_promise(*source, &mut marks, &mut worklist);
                        }
                        PromiseCombinatorInput::Fulfilled(value) => {
                            self.mark_value(value, &mut marks, &mut worklist);
                        }
                    }
                }
            }
        }
        for request in &self.pending_host_calls {
            if let Some(promise) = request.promise {
                self.mark_promise(promise, &mut marks, &mut worklist);
            }
        }
        if let Some(request) = &self.suspended_host_call
            && let Some(promise) = request.promise
        {
            self.mark_promise(promise, &mut marks, &mut worklist);
        }

        while !worklist.envs.is_empty()
            || !worklist.cells.is_empty()
            || !worklist.objects.is_empty()
            || !worklist.arrays.is_empty()
            || !worklist.maps.is_empty()
            || !worklist.sets.is_empty()
            || !worklist.iterators.is_empty()
            || !worklist.closures.is_empty()
            || !worklist.promises.is_empty()
        {
            while let Some(key) = worklist.envs.pop() {
                let env = self
                    .envs
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing environment"))?;
                if let Some(parent) = env.parent {
                    self.mark_env(parent, &mut marks, &mut worklist);
                }
                for cell in env.bindings.values() {
                    self.mark_cell(*cell, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.cells.pop() {
                let cell = self
                    .cells
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing binding cell"))?;
                self.mark_value(&cell.value, &mut marks, &mut worklist);
            }

            while let Some(key) = worklist.objects.pop() {
                let object = self
                    .objects
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing object"))?;
                if let ObjectKind::FunctionPrototype(constructor) = &object.kind {
                    self.mark_value(constructor, &mut marks, &mut worklist);
                }
                if let ObjectKind::BoundFunction(bound) = &object.kind {
                    self.mark_value(&bound.target, &mut marks, &mut worklist);
                    self.mark_value(&bound.this_value, &mut marks, &mut worklist);
                    for value in &bound.args {
                        self.mark_value(value, &mut marks, &mut worklist);
                    }
                }
                for value in object.properties.values() {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.arrays.pop() {
                let array = self
                    .arrays
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing array"))?;
                for value in array.elements.iter().flatten() {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
                for value in array.properties.values() {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.maps.pop() {
                let map = self
                    .maps
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing map"))?;
                for entry in &map.entries {
                    let Some(entry) = entry else {
                        continue;
                    };
                    self.mark_value(&entry.key, &mut marks, &mut worklist);
                    self.mark_value(&entry.value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.sets.pop() {
                let set = self
                    .sets
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing set"))?;
                for value in &set.entries {
                    let Some(value) = value else {
                        continue;
                    };
                    self.mark_value(value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.iterators.pop() {
                let iterator = self
                    .iterators
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing iterator"))?;
                match iterator.state {
                    IteratorState::Array(ref state) => {
                        self.mark_value(&Value::Array(state.array), &mut marks, &mut worklist);
                    }
                    IteratorState::ArrayKeys(ref state)
                    | IteratorState::ArrayEntries(ref state) => {
                        self.mark_value(&Value::Array(state.array), &mut marks, &mut worklist);
                    }
                    IteratorState::String(_) => {}
                    IteratorState::MapEntries(ref state)
                    | IteratorState::MapKeys(ref state)
                    | IteratorState::MapValues(ref state) => {
                        self.mark_value(&Value::Map(state.map), &mut marks, &mut worklist);
                    }
                    IteratorState::SetEntries(ref state) | IteratorState::SetValues(ref state) => {
                        self.mark_value(&Value::Set(state.set), &mut marks, &mut worklist);
                    }
                }
            }

            while let Some(key) = worklist.closures.pop() {
                let closure = self
                    .closures
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing closure"))?;
                self.mark_env(closure.env, &mut marks, &mut worklist);
                self.mark_value(&closure.this_value, &mut marks, &mut worklist);
                if let Some(prototype) = closure.prototype {
                    self.mark_value(&Value::Object(prototype), &mut marks, &mut worklist);
                }
                for value in closure.properties.values() {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.promises.pop() {
                let promise = self
                    .promises
                    .get(key)
                    .ok_or_else(|| MustardError::runtime("gc encountered missing promise"))?;
                match &promise.state {
                    PromiseState::Pending => {}
                    PromiseState::Fulfilled(value) => {
                        self.mark_value(value, &mut marks, &mut worklist);
                    }
                    PromiseState::Rejected(rejection) => {
                        self.mark_value(&rejection.value, &mut marks, &mut worklist);
                    }
                }
                for continuation in &promise.awaiters {
                    for frame in &continuation.frames {
                        self.mark_frame_roots(frame, &mut marks, &mut worklist);
                    }
                }
                for dependent in &promise.dependents {
                    self.mark_promise(*dependent, &mut marks, &mut worklist);
                }
                for reaction in &promise.reactions {
                    self.mark_promise(
                        self.promise_reaction_target(reaction),
                        &mut marks,
                        &mut worklist,
                    );
                    match reaction {
                        PromiseReaction::Then {
                            on_fulfilled,
                            on_rejected,
                            ..
                        } => {
                            if let Some(handler) = on_fulfilled {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                            if let Some(handler) = on_rejected {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::Finally { callback, .. } => {
                            if let Some(callback) = callback {
                                self.mark_value(callback, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::FinallyPassThrough {
                            original_outcome, ..
                        } => match original_outcome {
                            PromiseOutcome::Fulfilled(value) => {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                            PromiseOutcome::Rejected(rejection) => {
                                self.mark_value(&rejection.value, &mut marks, &mut worklist);
                            }
                        },
                        PromiseReaction::Combinator { .. } => {}
                    }
                }
                if let Some(driver) = &promise.driver {
                    match driver {
                        PromiseDriver::Thenable { value } => {
                            self.mark_value(value, &mut marks, &mut worklist);
                        }
                        PromiseDriver::All { values, .. } => {
                            for value in values.iter().flatten() {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                        }
                        PromiseDriver::AllSettled { results, .. } => {
                            for result in results.iter().flatten() {
                                match result {
                                    PromiseSettledResult::Fulfilled(value)
                                    | PromiseSettledResult::Rejected(value) => {
                                        self.mark_value(value, &mut marks, &mut worklist);
                                    }
                                }
                            }
                        }
                        PromiseDriver::Any { reasons, .. } => {
                            for value in reasons.iter().flatten() {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                        }
                    }
                }
            }
        }

        Ok(marks)
    }

    pub(super) fn mark_env(
        &self,
        key: EnvKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.envs.insert(key, ()).is_none() {
            worklist.envs.push(key);
        }
    }

    pub(super) fn mark_cell(
        &self,
        key: CellKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.cells.insert(key, ()).is_none() {
            worklist.cells.push(key);
        }
    }

    pub(super) fn mark_frame_roots(
        &self,
        frame: &Frame,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        self.mark_env(frame.env, marks, worklist);
        for env in &frame.scope_stack {
            self.mark_env(*env, marks, worklist);
        }
        for value in &frame.stack {
            self.mark_value(value, marks, worklist);
        }
        if let Some(value) = &frame.pending_exception {
            self.mark_value(value, marks, worklist);
        }
        for handler in &frame.handlers {
            self.mark_env(handler.env, marks, worklist);
        }
        for completion in &frame.pending_completions {
            match completion {
                CompletionRecord::Jump { .. } => {}
                CompletionRecord::Return(value) | CompletionRecord::Throw(value) => {
                    self.mark_value(value, marks, worklist);
                }
            }
        }
        if let Some(async_promise) = frame.async_promise {
            self.mark_promise(async_promise, marks, worklist);
        }
    }

    pub(super) fn mark_value(
        &self,
        value: &Value,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        match value {
            Value::Object(key) => {
                if marks.objects.insert(*key, ()).is_none() {
                    worklist.objects.push(*key);
                }
            }
            Value::Array(key) => {
                if marks.arrays.insert(*key, ()).is_none() {
                    worklist.arrays.push(*key);
                }
            }
            Value::Map(key) => {
                if marks.maps.insert(*key, ()).is_none() {
                    worklist.maps.push(*key);
                }
            }
            Value::Set(key) => {
                if marks.sets.insert(*key, ()).is_none() {
                    worklist.sets.push(*key);
                }
            }
            Value::Iterator(key) => {
                if marks.iterators.insert(*key, ()).is_none() {
                    worklist.iterators.push(*key);
                }
            }
            Value::Closure(key) => {
                if marks.closures.insert(*key, ()).is_none() {
                    worklist.closures.push(*key);
                }
            }
            Value::Promise(key) => self.mark_promise(*key, marks, worklist),
            Value::BuiltinFunction(BuiltinFunction::PromiseResolveFunction(key))
            | Value::BuiltinFunction(BuiltinFunction::PromiseRejectFunction(key)) => {
                self.mark_promise(*key, marks, worklist)
            }
            Value::Undefined
            | Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::BigInt(_)
            | Value::String(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {}
        }
    }

    fn record_reclaimed_item(
        stats: &mut GarbageCollectionStats,
        accounted_bytes: usize,
    ) -> MustardResult<()> {
        stats.reclaimed_bytes = stats
            .reclaimed_bytes
            .checked_add(accounted_bytes)
            .ok_or_else(|| MustardError::runtime("gc reclaimed byte overflow"))?;
        stats.reclaimed_allocations = stats
            .reclaimed_allocations
            .checked_add(1)
            .ok_or_else(|| MustardError::runtime("gc reclaimed allocation overflow"))?;
        Ok(())
    }

    pub(super) fn sweep_unreachable_envs(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .envs
            .keys()
            .filter(|key| !marks.envs.contains_key(*key))
            .collect();
        for key in dead {
            let env = self
                .envs
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing environment"))?;
            Self::record_reclaimed_item(stats, env.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_cells(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .cells
            .keys()
            .filter(|key| !marks.cells.contains_key(*key))
            .collect();
        for key in dead {
            let cell = self
                .cells
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing binding cell"))?;
            Self::record_reclaimed_item(stats, cell.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_objects(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .objects
            .keys()
            .filter(|key| !marks.objects.contains_key(*key))
            .collect();
        for key in dead {
            let object = self
                .objects
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing object"))?;
            Self::record_reclaimed_item(stats, object.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_arrays(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .arrays
            .keys()
            .filter(|key| !marks.arrays.contains_key(*key))
            .collect();
        for key in dead {
            let array = self
                .arrays
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing array"))?;
            Self::record_reclaimed_item(stats, array.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_maps(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .maps
            .keys()
            .filter(|key| !marks.maps.contains_key(*key))
            .collect();
        for key in dead {
            let map = self
                .maps
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing map"))?;
            Self::record_reclaimed_item(stats, map.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_sets(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .sets
            .keys()
            .filter(|key| !marks.sets.contains_key(*key))
            .collect();
        for key in dead {
            let set = self
                .sets
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing set"))?;
            Self::record_reclaimed_item(stats, set.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_iterators(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .iterators
            .keys()
            .filter(|key| !marks.iterators.contains_key(*key))
            .collect();
        for key in dead {
            let iterator = self
                .iterators
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing iterator"))?;
            Self::record_reclaimed_item(stats, iterator.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_closures(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .closures
            .keys()
            .filter(|key| !marks.closures.contains_key(*key))
            .collect();
        for key in dead {
            let closure = self
                .closures
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing closure"))?;
            Self::record_reclaimed_item(stats, closure.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn sweep_unreachable_promises(
        &mut self,
        marks: &GarbageCollectionMarks,
        stats: &mut GarbageCollectionStats,
    ) -> MustardResult<()> {
        let dead: Vec<_> = self
            .promises
            .keys()
            .filter(|key| !marks.promises.contains_key(*key))
            .collect();
        for key in dead {
            let promise = self
                .promises
                .remove(key)
                .ok_or_else(|| MustardError::runtime("gc encountered missing promise"))?;
            Self::record_reclaimed_item(stats, promise.accounted_bytes)?;
        }
        Ok(())
    }

    pub(super) fn mark_promise(
        &self,
        key: PromiseKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.promises.insert(key, ()).is_none() {
            worklist.promises.push(key);
        }
    }
}

fn scaled_gc_threshold(limit: usize, divisor: usize, min: usize, max: usize) -> usize {
    if limit == 0 {
        return 0;
    }
    let scaled = (limit / divisor).max(1);
    scaled.clamp(min.min(limit), max.min(limit))
}

fn budget_is_under_pressure(used: usize, limit: usize) -> bool {
    if limit == 0 {
        return used > 0;
    }
    if limit <= GC_PRESSURE_DENOMINATOR {
        return used >= limit;
    }
    used >= limit.saturating_mul(GC_PRESSURE_NUMERATOR) / GC_PRESSURE_DENOMINATOR
}

fn instruction_may_allocate(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::StoreName(_)
            | Instruction::StoreGlobal(_)
            | Instruction::StoreNameDiscard(_)
            | Instruction::StoreGlobalDiscard(_)
            | Instruction::InitializePattern(_)
            | Instruction::PushEnv
            | Instruction::DeclareName { .. }
            | Instruction::MakeClosure { .. }
            | Instruction::MakeArray { .. }
            | Instruction::ArrayPush
            | Instruction::ArrayPushHole
            | Instruction::ArrayExtend
            | Instruction::MakeObject { .. }
            | Instruction::CopyDataProperties
            | Instruction::CreateIterator
            | Instruction::SetPropStatic { .. }
            | Instruction::SetPropStaticDiscard { .. }
            | Instruction::SetPropComputed
            | Instruction::SetPropComputedDiscard
            | Instruction::Call { .. }
            | Instruction::CallWithArray { .. }
            | Instruction::Await
            | Instruction::Construct { .. }
            | Instruction::ConstructWithArray
    )
}
