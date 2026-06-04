use super::*;

pub(super) fn is_ephemeral_internal_binding(name: &str) -> bool {
    name.starts_with("\0mustard_pattern_")
        || name.starts_with("\0mustard_assign_")
        || name.starts_with("\0mustard_update_")
}

impl Runtime {
    pub(super) fn global_object_key(&self) -> Option<ObjectKey> {
        let cell = self
            .envs
            .get(self.globals)?
            .bindings
            .get("globalThis")
            .copied()?;
        match self.cells.get(cell)?.value {
            Value::Object(object) => Some(object),
            _ => None,
        }
    }

    fn global_property_value(&self, name: &str) -> Option<Value> {
        let global_object = self.global_object_key()?;
        self.objects
            .get(global_object)?
            .properties
            .get(name)
            .cloned()
    }

    fn global_binding_cell(&self, name: &str) -> Option<CellKey> {
        self.envs.get(self.globals)?.bindings.get(name).copied()
    }

    fn set_global_property_value(&mut self, name: String, value: Value) -> MustardResult<()> {
        let Some(global_object) = self.global_object_key() else {
            return Ok(());
        };
        let new_entry_bytes = Self::property_entry_bytes(&name, &value);
        let old_entry_bytes = {
            let object = self
                .objects
                .get_mut(global_object)
                .ok_or_else(|| MustardError::runtime("global object missing"))?;
            let old_entry_bytes = object
                .properties
                .get(&name)
                .map(|existing| Self::property_entry_bytes(&name, existing))
                .unwrap_or(0);
            object.properties.insert(name, value);
            old_entry_bytes
        };
        self.apply_object_component_delta(global_object, old_entry_bytes, new_entry_bytes)?;
        Ok(())
    }

    pub(super) fn infer_closure_name(&mut self, value: &Value, name: &str) -> MustardResult<()> {
        let Value::Closure(closure) = value else {
            return Ok(());
        };
        let needs_name = self
            .closures
            .get(*closure)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .name
            .is_none();
        if !needs_name {
            return Ok(());
        }
        let old_name_bytes = self
            .closures
            .get(*closure)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .name
            .as_ref()
            .map_or(0, String::len);
        let new_name_bytes = name.len();
        self.closures
            .get_mut(*closure)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .name = Some(name.to_string());
        self.apply_closure_component_delta(*closure, old_name_bytes, new_name_bytes)?;
        Ok(())
    }

    pub(super) fn new_env(&mut self, parent: Option<EnvKey>) -> MustardResult<EnvKey> {
        self.insert_env(parent)
    }

    pub(super) fn define_global(
        &mut self,
        name: String,
        value: Value,
        _mutable: bool,
    ) -> MustardResult<()> {
        self.infer_closure_name(&value, &name)?;
        self.set_global_property_value(name, value)
    }

    pub(super) fn define_global_binding(
        &mut self,
        name: String,
        value: Value,
        mutable: bool,
    ) -> MustardResult<()> {
        let binding_name = name.clone();
        let binding_bytes = Self::binding_entry_bytes(&name);
        let cell = self.insert_cell(value, mutable, true)?;
        self.envs
            .get_mut(self.globals)
            .ok_or_else(|| MustardError::runtime("missing globals environment"))?
            .bindings
            .insert(name, cell);
        self.apply_env_component_delta(self.globals, 0, binding_bytes)?;
        let value = self
            .cells
            .get(cell)
            .ok_or_else(|| MustardError::runtime("binding cell missing"))?
            .value
            .clone();
        self.set_global_property_value(binding_name, value)?;
        Ok(())
    }

    pub(super) fn declare_name(
        &mut self,
        env: EnvKey,
        name: String,
        mutable: bool,
    ) -> MustardResult<()> {
        if self
            .envs
            .get(env)
            .ok_or_else(|| MustardError::runtime("environment missing"))?
            .bindings
            .contains_key(&name)
        {
            return Ok(());
        }
        let cell = self.insert_cell(Value::Undefined, mutable, false)?;
        let binding_bytes = Self::binding_entry_bytes(&name);
        let is_ephemeral = is_ephemeral_internal_binding(&name);
        let env_data = self
            .envs
            .get_mut(env)
            .ok_or_else(|| MustardError::runtime("environment missing"))?;
        env_data.bindings.insert(name, cell);
        if is_ephemeral {
            env_data.ephemeral_binding_count += 1;
        }
        self.apply_env_component_delta(env, 0, binding_bytes)?;
        Ok(())
    }

    fn env_at_depth(&self, env: EnvKey, depth: usize) -> MustardResult<EnvKey> {
        let mut current = Some(env);
        for _ in 0..depth {
            current = current
                .and_then(|key| self.envs.get(key))
                .and_then(|env| env.parent);
        }
        current.ok_or_else(|| {
            MustardError::runtime(format!(
                "environment missing while resolving lexical slot at depth {depth}"
            ))
        })
    }

    /// Hot-path slot resolution: returns the resolved environment plus the bound
    /// cell without cloning the binding name. Names are only needed for rare
    /// diagnostics, so callers fetch them lazily via [`Self::slot_binding_name`].
    ///
    /// When the resolved environment contains no ephemeral internal temporaries
    /// (`\0mustard_*` bindings created by destructuring/compound-assignment
    /// lowering), the binding order in `bindings` matches the compiler's slot
    /// numbering exactly, so we can index in O(1). Otherwise we fall back to the
    /// filtered scan, which is identical in result to the fast path whenever no
    /// ephemeral bindings are present.
    fn resolve_slot_cell(
        &self,
        env: EnvKey,
        depth: usize,
        slot: usize,
    ) -> MustardResult<(EnvKey, CellKey)> {
        let resolved = self.env_at_depth(env, depth)?;
        let env_data = self
            .envs
            .get(resolved)
            .ok_or_else(|| MustardError::runtime("environment missing"))?;
        let cell = if env_data.ephemeral_binding_count == 0 {
            env_data.bindings.get_index(slot).map(|(_, cell)| *cell)
        } else {
            env_data
                .bindings
                .iter()
                .filter(|(name, _)| !is_ephemeral_internal_binding(name))
                .nth(slot)
                .map(|(_, cell)| *cell)
        };
        let cell = cell.ok_or_else(|| {
            MustardError::runtime(format!(
                "binding slot {slot} missing in environment at depth {depth}"
            ))
        })?;
        Ok((resolved, cell))
    }

    /// Resolve only the name bound at a `(depth, slot)` for diagnostics. This is
    /// off the hot path and may allocate.
    fn slot_binding_name(&self, env: EnvKey, depth: usize, slot: usize) -> String {
        self.env_at_depth(env, depth)
            .ok()
            .and_then(|resolved| self.envs.get(resolved))
            .and_then(|env_data| {
                env_data
                    .bindings
                    .iter()
                    .filter(|(name, _)| !is_ephemeral_internal_binding(name))
                    .nth(slot)
                    .map(|(name, _)| name.clone())
            })
            .unwrap_or_else(|| format!("<slot {slot}>"))
    }

    /// The string-index ASCII cache (`string_ascii_cache`) is keyed by the
    /// `(ptr, len)` of live string data. A store that neither removes a string
    /// from a cell nor introduces one cannot invalidate any cached entry (no
    /// string buffer is freed or reused), so only clear when a string actually
    /// enters or leaves the cell. Storing a number/bool into a numeric counter
    /// in a hot loop therefore no longer nukes the whole cache each iteration.
    fn invalidate_string_index_cache_on_store(
        &mut self,
        old_was_string: bool,
        new_is_string: bool,
    ) {
        if old_was_string || new_is_string {
            self.string_ascii_cache.clear();
        }
    }

    pub(super) fn lookup_slot(
        &self,
        env: EnvKey,
        depth: usize,
        slot: usize,
    ) -> MustardResult<Value> {
        let (_, cell_key) = self.resolve_slot_cell(env, depth, slot)?;
        let cell = self
            .cells
            .get(cell_key)
            .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
        if !cell.initialized {
            let name = self.slot_binding_name(env, depth, slot);
            return Err(MustardError::runtime(format!(
                "ReferenceError: `{name}` accessed before initialization"
            )));
        }
        Ok(cell.value.clone())
    }

    pub(super) fn lookup_slot_string_property(
        &mut self,
        env: EnvKey,
        depth: usize,
        slot: usize,
        key: &str,
    ) -> MustardResult<Option<Value>> {
        let (_, cell_key) = self.resolve_slot_cell(env, depth, slot)?;
        let string_cache_key = {
            let cell = self
                .cells
                .get(cell_key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
            if !cell.initialized {
                let name = self.slot_binding_name(env, depth, slot);
                return Err(MustardError::runtime(format!(
                    "ReferenceError: `{name}` accessed before initialization"
                )));
            }
            match &cell.value {
                Value::String(value) => (value.as_ptr() as usize, value.len()),
                _ => return Ok(None),
            }
        };

        let is_ascii = match self.string_ascii_cache.get(&string_cache_key) {
            Some(is_ascii) => *is_ascii,
            None => {
                let value = match &self
                    .cells
                    .get(cell_key)
                    .ok_or_else(|| MustardError::runtime("binding cell missing"))?
                    .value
                {
                    Value::String(value) => value,
                    _ => return Ok(None),
                };
                let is_ascii = value.is_ascii();
                self.string_ascii_cache.insert(string_cache_key, is_ascii);
                is_ascii
            }
        };

        let value = match &self
            .cells
            .get(cell_key)
            .ok_or_else(|| MustardError::runtime("binding cell missing"))?
            .value
        {
            Value::String(value) => value,
            _ => return Ok(None),
        };

        if key == "length" {
            return Ok(Some(Value::Number(if is_ascii {
                value.len() as f64
            } else {
                value.chars().count() as f64
            })));
        }

        let Some(index) = array_index_from_property_key(key) else {
            return Ok(None);
        };
        let value = if is_ascii {
            value
                .as_bytes()
                .get(index)
                .map(|byte| Value::String(char::from(*byte).to_string()))
        } else {
            string_index_property_value(value, index)
        };
        Ok(Some(value.unwrap_or(Value::Undefined)))
    }

    pub(super) fn assign_slot(
        &mut self,
        env: EnvKey,
        depth: usize,
        slot: usize,
        value: Value,
    ) -> MustardResult<()> {
        let (resolved_env, cell_key) = self.resolve_slot_cell(env, depth, slot)?;
        if matches!(value, Value::Closure(_)) {
            let name = self.slot_binding_name(env, depth, slot);
            self.infer_closure_name(&value, &name)?;
        }
        let new_value_bytes = Self::cell_value_bytes(&value);
        let new_is_string = matches!(value, Value::String(_));
        let old_value_bytes;
        let old_was_string;
        {
            let cell = self
                .cells
                .get_mut(cell_key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
            if !cell.initialized {
                let name = self.slot_binding_name(env, depth, slot);
                return Err(MustardError::runtime(format!(
                    "ReferenceError: `{name}` accessed before initialization"
                )));
            }
            if !cell.mutable {
                let name = self.slot_binding_name(env, depth, slot);
                return Err(MustardError::runtime(format!(
                    "TypeError: assignment to constant variable `{name}`"
                )));
            }
            old_value_bytes = Self::cell_value_bytes(&cell.value);
            old_was_string = matches!(cell.value, Value::String(_));
            cell.value = value;
        }
        self.apply_cell_component_delta(cell_key, old_value_bytes, new_value_bytes)?;
        self.invalidate_string_index_cache_on_store(old_was_string, new_is_string);
        // A slot binding only needs to be mirrored into the global object when it
        // actually resolves into the globals environment. Lexical slots never
        // resolve to globals (free names compile to `LoadGlobal`/`StoreGlobal`),
        // so this branch is effectively only taken for global-scope bindings and
        // we avoid resolving the name string on the common local-assignment path.
        if resolved_env == self.globals {
            let name = self.slot_binding_name(env, depth, slot);
            if self
                .envs
                .get(self.globals)
                .and_then(|globals| globals.bindings.get(name.as_str()))
                .is_some_and(|bound| *bound == cell_key)
            {
                let value = self
                    .cells
                    .get(cell_key)
                    .ok_or_else(|| MustardError::runtime("binding cell missing"))?
                    .value
                    .clone();
                self.set_global_property_value(name, value)?;
            }
        }
        Ok(())
    }

    pub(super) fn lookup_name(&self, env: EnvKey, name: &str) -> MustardResult<Value> {
        let Some(cell) = self.find_cell(env, name) else {
            return self.lookup_global_name(name);
        };
        let cell = self
            .cells
            .get(cell)
            .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
        if !cell.initialized {
            return Err(MustardError::runtime(format!(
                "ReferenceError: `{name}` accessed before initialization"
            )));
        }
        Ok(cell.value.clone())
    }

    pub(super) fn lookup_global_name(&self, name: &str) -> MustardResult<Value> {
        if let Some(cell) = self.global_binding_cell(name) {
            let cell = self
                .cells
                .get(cell)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
            if !cell.initialized {
                return Err(MustardError::runtime(format!(
                    "ReferenceError: `{name}` accessed before initialization"
                )));
            }
            return Ok(cell.value.clone());
        }
        if let Some(value) = self.global_property_value(name) {
            return Ok(value);
        }
        Err(MustardError::Message {
            kind: DiagnosticKind::Runtime,
            message: format!("ReferenceError: `{name}` is not defined"),
            span: None,
            traceback: Vec::new(),
        })
    }

    pub(super) fn assign_name(
        &mut self,
        env: EnvKey,
        name: &str,
        value: Value,
    ) -> MustardResult<()> {
        let Some(cell_key) = self.find_cell(env, name) else {
            return self.assign_global_name(name, value);
        };
        self.infer_closure_name(&value, name)?;
        let new_value_bytes = Self::cell_value_bytes(&value);
        let old_value_bytes;
        {
            let cell = self
                .cells
                .get_mut(cell_key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
            if !cell.initialized {
                return Err(MustardError::runtime(format!(
                    "ReferenceError: `{name}` accessed before initialization"
                )));
            }
            if !cell.mutable {
                return Err(MustardError::runtime(format!(
                    "TypeError: assignment to constant variable `{name}`"
                )));
            }
            old_value_bytes = Self::cell_value_bytes(&cell.value);
            cell.value = value;
        }
        self.apply_cell_component_delta(cell_key, old_value_bytes, new_value_bytes)?;
        if self
            .envs
            .get(self.globals)
            .and_then(|globals| globals.bindings.get(name))
            .is_some_and(|bound| *bound == cell_key)
        {
            let value = self
                .cells
                .get(cell_key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?
                .value
                .clone();
            self.set_global_property_value(name.to_string(), value)?;
        }
        Ok(())
    }

    pub(super) fn assign_global_name(&mut self, name: &str, value: Value) -> MustardResult<()> {
        if let Some(cell_key) = self.global_binding_cell(name) {
            self.infer_closure_name(&value, name)?;
            let new_value_bytes = Self::cell_value_bytes(&value);
            let new_is_string = matches!(value, Value::String(_));
            let old_value_bytes;
            let old_was_string;
            {
                let cell = self
                    .cells
                    .get_mut(cell_key)
                    .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
                if !cell.initialized {
                    return Err(MustardError::runtime(format!(
                        "ReferenceError: `{name}` accessed before initialization"
                    )));
                }
                if !cell.mutable {
                    return Err(MustardError::runtime(format!(
                        "TypeError: assignment to constant variable `{name}`"
                    )));
                }
                old_value_bytes = Self::cell_value_bytes(&cell.value);
                old_was_string = matches!(cell.value, Value::String(_));
                cell.value = value;
            }
            self.apply_cell_component_delta(cell_key, old_value_bytes, new_value_bytes)?;
            self.invalidate_string_index_cache_on_store(old_was_string, new_is_string);
            let value = self
                .cells
                .get(cell_key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?
                .value
                .clone();
            return self.set_global_property_value(name.to_string(), value);
        }
        if self.global_property_value(name).is_some() {
            self.infer_closure_name(&value, name)?;
            return self.set_global_property_value(name.to_string(), value);
        }
        Err(MustardError::runtime(format!(
            "ReferenceError: `{name}` is not defined"
        )))
    }

    pub(super) fn initialize_name_in_env(
        &mut self,
        env: EnvKey,
        name: &str,
        value: Value,
    ) -> MustardResult<()> {
        self.infer_closure_name(&value, name)?;
        let cell_key = self
            .envs
            .get(env)
            .and_then(|env| env.bindings.get(name).copied())
            .ok_or_else(|| {
                MustardError::runtime(format!("binding `{name}` missing in current scope"))
            })?;
        let mut was_initialized = false;
        let new_value_bytes = Self::cell_value_bytes(&value);
        let new_is_string = matches!(value, Value::String(_));
        let old_value_bytes;
        let old_was_string;
        {
            let cell = self
                .cells
                .get_mut(cell_key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?;
            if cell.initialized {
                if !cell.mutable {
                    return Err(MustardError::runtime(format!(
                        "TypeError: binding `{name}` was already initialized"
                    )));
                }
                old_value_bytes = Self::cell_value_bytes(&cell.value);
                old_was_string = matches!(cell.value, Value::String(_));
                cell.value = value;
                was_initialized = true;
            } else {
                old_value_bytes = Self::cell_value_bytes(&cell.value);
                old_was_string = matches!(cell.value, Value::String(_));
                cell.value = value;
                cell.initialized = true;
            }
        }
        self.apply_cell_component_delta(cell_key, old_value_bytes, new_value_bytes)?;
        self.invalidate_string_index_cache_on_store(old_was_string, new_is_string);
        if env == self.globals {
            let value = self
                .cells
                .get(cell_key)
                .ok_or_else(|| MustardError::runtime("binding cell missing"))?
                .value
                .clone();
            self.set_global_property_value(name.to_string(), value)?;
        }
        if was_initialized {
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn find_cell(&self, env: EnvKey, name: &str) -> Option<CellKey> {
        let mut current = Some(env);
        while let Some(key) = current {
            let env = self.envs.get(key)?;
            if let Some(cell) = env.bindings.get(name) {
                return Some(*cell);
            }
            current = env.parent;
        }
        None
    }

    pub(super) fn initialize_pattern(
        &mut self,
        env: EnvKey,
        pattern: &Pattern,
        value: Value,
    ) -> MustardResult<()> {
        match pattern {
            Pattern::Identifier { name, .. } => self.initialize_name_in_env(env, name, value),
            Pattern::Default {
                target,
                default_value,
                ..
            } => {
                let value = if matches!(value, Value::Undefined) {
                    let bytecode = BytecodeProgram {
                        functions: vec![FunctionPrototype {
                            name: None,
                            length: 0,
                            display_source: String::new(),
                            params: Vec::new(),
                            param_binding_names: Vec::new(),
                            rest: None,
                            rest_binding_names: Vec::new(),
                            code: Vec::new(),
                            is_async: false,
                            is_arrow: false,
                            span: SourceSpan::new(0, 0),
                        }],
                        root: 0,
                    };
                    drop(bytecode);
                    return Err(MustardError::runtime(format!(
                        "default pattern initialization at runtime requires compiled evaluation support: {:?}",
                        default_value
                    )));
                } else {
                    value
                };
                self.initialize_pattern(env, target, value)
            }
            Pattern::Array { elements, rest, .. } => {
                let items = self.to_array_items(value)?;
                for (index, pattern) in elements.iter().enumerate() {
                    if let Some(pattern) = pattern {
                        self.initialize_pattern(
                            env,
                            pattern,
                            items.get(index).cloned().unwrap_or(Value::Undefined),
                        )?;
                    }
                }
                if let Some(rest) = rest {
                    let array = self.insert_array(
                        items.into_iter().skip(elements.len()).collect(),
                        IndexMap::new(),
                    )?;
                    self.initialize_pattern(env, rest, Value::Array(array))?;
                }
                Ok(())
            }
            Pattern::Object {
                properties, rest, ..
            } => {
                let mut seen = HashSet::new();
                for property in properties {
                    let key = property_name_to_key(&property.key);
                    let prop_value = self.get_property_static(value.clone(), &key, false)?;
                    seen.insert(key);
                    self.initialize_pattern(env, &property.value, prop_value)?;
                }
                if let Some(rest_pattern) = rest {
                    let mut rest_object = IndexMap::new();
                    match value {
                        Value::Object(object) => {
                            if let Some(object) = self.objects.get(object) {
                                for (key, value) in &object.properties {
                                    if !seen.contains(key) {
                                        rest_object.insert(key.clone(), value.clone());
                                    }
                                }
                            }
                        }
                        Value::Null | Value::Undefined => {
                            return Err(MustardError::runtime(
                                "cannot destructure object pattern from nullish value",
                            ));
                        }
                        _ => {}
                    }
                    let rest = self.insert_object(rest_object, ObjectKind::Plain)?;
                    self.initialize_pattern(env, rest_pattern, Value::Object(rest))?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn capability_value(&self, name: &str) -> Option<Value> {
        match self.global_property_value(name)? {
            Value::HostFunction(capability) => Some(Value::HostFunction(capability)),
            _ => None,
        }
    }
}
