use super::*;

impl Runtime {
    pub(super) fn collect_garbage_before_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> MustardResult<()> {
        if instruction_may_allocate(instruction) {
            self.collect_garbage()?;
        }
        Ok(())
    }

    pub(super) fn collect_garbage(&mut self) -> MustardResult<GarbageCollectionStats> {
        let baseline_bytes = self.heap_bytes_used;
        let baseline_allocations = self.allocation_count;
        let marks = self.mark_reachable_heap()?;

        self.sweep_unreachable_envs(&marks);
        self.sweep_unreachable_cells(&marks);
        self.sweep_unreachable_objects(&marks);
        self.sweep_unreachable_arrays(&marks);
        self.sweep_unreachable_maps(&marks);
        self.sweep_unreachable_sets(&marks);
        self.sweep_unreachable_iterators(&marks);
        self.sweep_unreachable_closures(&marks);
        self.sweep_unreachable_promises(&marks);

        let (heap_bytes_used, allocation_count) = self
            .recompute_accounting_totals()
            .map_err(MustardError::runtime)?;
        self.heap_bytes_used = heap_bytes_used;
        self.allocation_count = allocation_count;

        Ok(GarbageCollectionStats {
            reclaimed_bytes: baseline_bytes.saturating_sub(heap_bytes_used),
            reclaimed_allocations: baseline_allocations.saturating_sub(allocation_count),
        })
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
                    outcome,
                } => {
                    for frame in &continuation.frames {
                        self.mark_frame_roots(frame, &mut marks, &mut worklist);
                    }
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => {
                            self.mark_value(value, &mut marks, &mut worklist);
                        }
                        PromiseOutcome::Rejected(rejection) => {
                            self.mark_value(&rejection.value, &mut marks, &mut worklist);
                        }
                    }
                }
                MicrotaskJob::PromiseReaction { reaction, outcome } => {
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
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => {
                            self.mark_value(value, &mut marks, &mut worklist);
                        }
                        PromiseOutcome::Rejected(rejection) => {
                            self.mark_value(&rejection.value, &mut marks, &mut worklist);
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
        if marks.envs.insert(key) {
            worklist.envs.push(key);
        }
    }

    pub(super) fn mark_cell(
        &self,
        key: CellKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.cells.insert(key) {
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
                if marks.objects.insert(*key) {
                    worklist.objects.push(*key);
                }
            }
            Value::Array(key) => {
                if marks.arrays.insert(*key) {
                    worklist.arrays.push(*key);
                }
            }
            Value::Map(key) => {
                if marks.maps.insert(*key) {
                    worklist.maps.push(*key);
                }
            }
            Value::Set(key) => {
                if marks.sets.insert(*key) {
                    worklist.sets.push(*key);
                }
            }
            Value::Iterator(key) => {
                if marks.iterators.insert(*key) {
                    worklist.iterators.push(*key);
                }
            }
            Value::Closure(key) => {
                if marks.closures.insert(*key) {
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

    pub(super) fn sweep_unreachable_envs(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .envs
            .keys()
            .filter(|key| !marks.envs.contains(key))
            .collect();
        for key in dead {
            self.envs.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_cells(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .cells
            .keys()
            .filter(|key| !marks.cells.contains(key))
            .collect();
        for key in dead {
            self.cells.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_objects(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .objects
            .keys()
            .filter(|key| !marks.objects.contains(key))
            .collect();
        for key in dead {
            self.objects.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_arrays(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .arrays
            .keys()
            .filter(|key| !marks.arrays.contains(key))
            .collect();
        for key in dead {
            self.arrays.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_maps(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .maps
            .keys()
            .filter(|key| !marks.maps.contains(key))
            .collect();
        for key in dead {
            self.maps.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_sets(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .sets
            .keys()
            .filter(|key| !marks.sets.contains(key))
            .collect();
        for key in dead {
            self.sets.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_iterators(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .iterators
            .keys()
            .filter(|key| !marks.iterators.contains(key))
            .collect();
        for key in dead {
            self.iterators.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_closures(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .closures
            .keys()
            .filter(|key| !marks.closures.contains(key))
            .collect();
        for key in dead {
            self.closures.remove(key);
        }
    }

    pub(super) fn sweep_unreachable_promises(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .promises
            .keys()
            .filter(|key| !marks.promises.contains(key))
            .collect();
        for key in dead {
            self.promises.remove(key);
        }
    }

    pub(super) fn mark_promise(
        &self,
        key: PromiseKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.promises.insert(key) {
            worklist.promises.push(key);
        }
    }
}

fn instruction_may_allocate(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::StoreName(_)
            | Instruction::StoreGlobal(_)
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
            | Instruction::SetPropComputed
            | Instruction::Call { .. }
            | Instruction::CallWithArray { .. }
            | Instruction::Await
            | Instruction::Construct { .. }
            | Instruction::ConstructWithArray
    )
}
