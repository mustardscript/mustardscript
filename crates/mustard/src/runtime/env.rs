use super::*;

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

    fn set_global_property_value(&mut self, name: String, value: Value) -> MustardResult<()> {
        let Some(global_object) = self.global_object_key() else {
            return Ok(());
        };
        self.objects
            .get_mut(global_object)
            .ok_or_else(|| MustardError::runtime("global object missing"))?
            .properties
            .insert(name, value);
        self.refresh_object_accounting(global_object)?;
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
        self.closures
            .get_mut(*closure)
            .ok_or_else(|| MustardError::runtime("closure missing"))?
            .name = Some(name.to_string());
        self.refresh_closure_accounting(*closure)?;
        Ok(())
    }

    pub(super) fn new_env(&mut self, parent: Option<EnvKey>) -> MustardResult<EnvKey> {
        self.insert_env(parent)
    }

    pub(super) fn define_global(
        &mut self,
        name: String,
        value: Value,
        mutable: bool,
    ) -> MustardResult<()> {
        let binding_name = name.clone();
        let cell = self.insert_cell(value, mutable, true)?;
        self.envs
            .get_mut(self.globals)
            .ok_or_else(|| MustardError::runtime("missing globals environment"))?
            .bindings
            .insert(name, cell);
        self.refresh_env_accounting(self.globals)?;
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
        self.envs
            .get_mut(env)
            .ok_or_else(|| MustardError::runtime("environment missing"))?
            .bindings
            .insert(name, cell);
        self.refresh_env_accounting(env)?;
        Ok(())
    }

    pub(super) fn lookup_name(&self, env: EnvKey, name: &str) -> MustardResult<Value> {
        let Some(cell) = self.find_cell(env, name) else {
            if let Some(value) = self.global_property_value(name) {
                return Ok(value);
            }
            return Err(MustardError::Message {
                kind: DiagnosticKind::Runtime,
                message: format!("ReferenceError: `{name}` is not defined"),
                span: None,
                traceback: Vec::new(),
            });
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

    pub(super) fn assign_name(
        &mut self,
        env: EnvKey,
        name: &str,
        value: Value,
    ) -> MustardResult<()> {
        self.infer_closure_name(&value, name)?;
        let Some(cell_key) = self.find_cell(env, name) else {
            if self.global_property_value(name).is_some() {
                return self.set_global_property_value(name.to_string(), value);
            }
            return Err(MustardError::runtime(format!(
                "ReferenceError: `{name}` is not defined"
            )));
        };
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
            cell.value = value;
        }
        self.refresh_cell_accounting(cell_key)?;
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
                cell.value = value;
                was_initialized = true;
            } else {
                cell.value = value;
                cell.initialized = true;
            }
        }
        self.refresh_cell_accounting(cell_key)?;
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
                            rest: None,
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
                    let prop_value =
                        self.get_property(value.clone(), Value::String(key.clone()), false)?;
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
        let cell = self.find_cell(self.globals, name)?;
        let cell = self.cells.get(cell)?;
        if !cell.initialized {
            return None;
        }
        match &cell.value {
            Value::HostFunction(_) => Some(cell.value.clone()),
            _ => None,
        }
    }
}
