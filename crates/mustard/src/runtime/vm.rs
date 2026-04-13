use super::*;
use num_bigint::BigInt;
use std::sync::Arc;

impl Runtime {
    pub(super) fn run_root(&mut self) -> MustardResult<ExecutionStep> {
        self.check_cancellation()?;
        self.collect_garbage()?;
        self.check_call_depth()?;
        let root_env = self.new_env(Some(self.globals))?;
        self.push_frame(
            self.program.root,
            root_env,
            &[],
            Value::Object(
                self.global_object_key()
                    .ok_or_else(|| MustardError::runtime("missing global object"))?,
            ),
            None,
        )?;
        self.run()
    }

    pub(super) fn step_active_frame(&mut self) -> MustardResult<StepAction> {
        let frame_index = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| MustardError::runtime("vm lost all frames"))?;
        let function_id = self.frames[frame_index].function_id;
        let ip = self.frames[frame_index].ip;
        let program = Arc::clone(&self.program);
        let instruction = program
            .functions
            .get(function_id)
            .and_then(|function| function.code.get(ip))
            .ok_or_else(|| MustardError::runtime("instruction pointer out of range"))?;
        self.frames[frame_index].ip += 1;
        self.bump_instruction_budget()?;
        self.collect_garbage_before_instruction(instruction)?;
        match instruction {
            Instruction::PushUndefined => {
                self.frames[frame_index].stack.push(Value::Undefined);
            }
            Instruction::PushNull => self.frames[frame_index].stack.push(Value::Null),
            Instruction::PushBool(value) => {
                self.frames[frame_index].stack.push(Value::Bool(*value))
            }
            Instruction::PushNumber(value) => {
                self.frames[frame_index].stack.push(Value::Number(*value))
            }
            Instruction::PushBigInt(value) => {
                let value = BigInt::parse_bytes(value.as_bytes(), 10)
                    .ok_or_else(|| MustardError::runtime("invalid BigInt literal in bytecode"))?;
                self.frames[frame_index].stack.push(Value::BigInt(value));
            }
            Instruction::PushString(value) => self.frames[frame_index]
                .stack
                .push(Value::String(value.clone())),
            Instruction::PushRegExp { pattern, flags } => {
                let value = self.make_regexp_value(pattern.clone(), flags.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::LoadSlot { depth, slot } => {
                let env = self.frames[frame_index].env;
                let value = self.lookup_slot(env, *depth, *slot)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::LoadName(name) => {
                let env = self.frames[frame_index].env;
                let value = self.lookup_name(env, name)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::LoadGlobalObject => {
                let value = Value::Object(
                    self.global_object_key()
                        .ok_or_else(|| MustardError::runtime("missing global object"))?,
                );
                self.frames[frame_index].stack.push(value);
            }
            Instruction::StoreName(name) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let env = self.frames[frame_index].env;
                self.assign_name(env, name, value.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::StoreSlot { depth, slot } => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let env = self.frames[frame_index].env;
                self.assign_slot(env, *depth, *slot, value.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::InitializePattern(pattern) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let env = self.frames[frame_index].env;
                self.initialize_pattern(env, pattern, value)?;
            }
            Instruction::PushEnv => {
                let current_env = self.frames[frame_index].env;
                let env = self.new_env(Some(current_env))?;
                self.frames[frame_index].scope_stack.push(current_env);
                self.frames[frame_index].env = env;
            }
            Instruction::PopEnv => {
                let restored = self.frames[frame_index]
                    .scope_stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("scope stack underflow"))?;
                self.frames[frame_index].env = restored;
            }
            Instruction::DeclareName { name, mutable } => {
                let env = self.frames[frame_index].env;
                self.declare_name(env, name.clone(), *mutable)?;
            }
            Instruction::MakeClosure { function_id } => {
                let env = self.frames[frame_index].env;
                let this_value = self.lookup_name(env, "this").unwrap_or(Value::Undefined);
                let closure = self.insert_closure(*function_id, env, this_value)?;
                self.frames[frame_index].stack.push(Value::Closure(closure));
            }
            Instruction::MakeArray { count } => {
                let values = pop_many(&mut self.frames[frame_index].stack, *count)?;
                let array = self.insert_array(values, IndexMap::new())?;
                self.frames[frame_index].stack.push(Value::Array(array));
            }
            Instruction::ArrayPush => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let target = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let Value::Array(array) = target else {
                    return Err(MustardError::runtime(
                        "array builder target is not an array",
                    ));
                };
                self.arrays
                    .get_mut(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .push(Some(value));
                self.refresh_array_accounting(array)?;
                self.frames[frame_index].stack.push(Value::Array(array));
            }
            Instruction::ArrayPushHole => {
                let target = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let Value::Array(array) = target else {
                    return Err(MustardError::runtime(
                        "array builder target is not an array",
                    ));
                };
                self.arrays
                    .get_mut(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .push(None);
                self.refresh_array_accounting(array)?;
                self.frames[frame_index].stack.push(Value::Array(array));
            }
            Instruction::ArrayExtend => {
                let iterable = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let target = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let target = self.expand_iterable_into_array(target, iterable)?;
                self.frames[frame_index].stack.push(target);
            }
            Instruction::MakeObject { keys } => {
                let values = pop_many(&mut self.frames[frame_index].stack, keys.len())?;
                let mut properties = IndexMap::new();
                for (key, value) in keys.iter().zip(values.into_iter()) {
                    properties.insert(property_name_to_key(key), value);
                }
                let object = self.insert_object(properties, ObjectKind::Plain)?;
                self.frames[frame_index].stack.push(Value::Object(object));
            }
            Instruction::CopyDataProperties => {
                let source = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let target = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                self.copy_data_properties(target.clone(), source)?;
                self.frames[frame_index].stack.push(target);
            }
            Instruction::CreateIterator => {
                let iterable = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let iterator = self.create_iterator(iterable)?;
                self.frames[frame_index].stack.push(iterator);
            }
            Instruction::IteratorNext => {
                let iterator = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let (value, done) = self.iterator_next(iterator)?;
                self.frames[frame_index].stack.push(value);
                self.frames[frame_index].stack.push(Value::Bool(done));
            }
            Instruction::GetPropStatic { name, optional } => {
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let value = self.get_property(object, Value::String(name.clone()), *optional)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::GetPropComputed { optional } => {
                let property = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let value = self.get_property(object, property, *optional)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::SetPropStatic { name } => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                self.set_property(object, Value::String(name.clone()), value.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::SetPropComputed => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let property = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let object = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                self.set_property(object, property, value.clone())?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Unary(operator) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let result = self.apply_unary(*operator, value)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::Binary(operator) => {
                let right = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let left = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let result = self.apply_binary(*operator, left, right)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::Update(operator) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let result = self.apply_update(*operator, value)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::PatternArrayIndex(index) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let result = self.pattern_array_index(value, *index)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::PatternArrayRest(start) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let result = self.pattern_array_rest(value, *start)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::PatternObjectRest(excluded) => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let result = self.pattern_object_rest(value, excluded)?;
                self.frames[frame_index].stack.push(result);
            }
            Instruction::Pop => {
                self.frames[frame_index].stack.pop();
            }
            Instruction::Dup => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Dup2 => {
                let len = self.frames[frame_index].stack.len();
                if len < 2 {
                    return Err(MustardError::runtime("stack underflow"));
                }
                let a = self.frames[frame_index].stack[len - 2].clone();
                let b = self.frames[frame_index].stack[len - 1].clone();
                self.frames[frame_index].stack.push(a);
                self.frames[frame_index].stack.push(b);
            }
            Instruction::PushHandler { catch, finally } => {
                let frame = &mut self.frames[frame_index];
                frame.handlers.push(ExceptionHandler {
                    catch: *catch,
                    finally: *finally,
                    env: frame.env,
                    scope_stack_len: frame.scope_stack.len(),
                    stack_len: frame.stack.len(),
                });
            }
            Instruction::PopHandler => {
                self.frames[frame_index]
                    .handlers
                    .pop()
                    .ok_or_else(|| MustardError::runtime("handler stack underflow"))?;
            }
            Instruction::EnterFinally { exit } => {
                let completion_index = self.frames[frame_index]
                    .pending_completions
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| MustardError::runtime("missing pending completion"))?;
                self.frames[frame_index]
                    .active_finally
                    .push(ActiveFinallyState {
                        completion_index,
                        exit: *exit,
                    });
            }
            Instruction::BeginCatch => {
                let value = self.frames[frame_index]
                    .pending_exception
                    .take()
                    .ok_or_else(|| MustardError::runtime("missing pending exception"))?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Throw { span } => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                return self.raise_exception(value, Some(*span));
            }
            Instruction::PushPendingJump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => {
                self.store_completion(
                    frame_index,
                    CompletionRecord::Jump {
                        target: *target,
                        target_handler_depth: *target_handler_depth,
                        target_scope_depth: *target_scope_depth,
                    },
                )?;
            }
            Instruction::PushPendingReturn => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                self.store_completion(frame_index, CompletionRecord::Return(value))?;
            }
            Instruction::PushPendingThrow => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                self.store_completion(frame_index, CompletionRecord::Throw(value))?;
            }
            Instruction::ContinuePending => {
                let marker = self.frames[frame_index]
                    .active_finally
                    .pop()
                    .ok_or_else(|| MustardError::runtime("missing active finally state"))?;
                if marker.completion_index >= self.frames[frame_index].pending_completions.len() {
                    return Err(MustardError::runtime(
                        "active finally references missing completion",
                    ));
                }
                let completion = self.frames[frame_index]
                    .pending_completions
                    .remove(marker.completion_index);
                return self.resume_completion(completion);
            }
            Instruction::Jump(target) => self.frames[frame_index].ip = *target,
            Instruction::JumpIfFalse(target) => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                if !is_truthy(&value) {
                    self.frames[frame_index].ip = *target;
                }
            }
            Instruction::JumpIfTrue(target) => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                if is_truthy(&value) {
                    self.frames[frame_index].ip = *target;
                }
            }
            Instruction::JumpIfNullish(target) => {
                let value = self.frames[frame_index]
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                if matches!(value, Value::Null | Value::Undefined) {
                    self.frames[frame_index].ip = *target;
                }
            }
            Instruction::Call {
                argc,
                with_this,
                optional,
            } => {
                let args = pop_many(&mut self.frames[frame_index].stack, *argc)?;
                let callee = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let this_value = if *with_this {
                    self.frames[frame_index]
                        .stack
                        .pop()
                        .ok_or_else(|| MustardError::runtime("stack underflow"))?
                } else {
                    Value::Undefined
                };
                if *optional && matches!(callee, Value::Undefined | Value::Null) {
                    self.frames[frame_index].stack.push(Value::Undefined);
                    return Ok(StepAction::Continue);
                }
                match self.call_callable(callee, this_value, &args)? {
                    RunState::Completed(value) => {
                        self.frames[frame_index].stack.push(value);
                    }
                    RunState::PushedFrame => {}
                    RunState::StartedAsync(value) => {
                        self.frames[frame_index].stack.push(value);
                    }
                    RunState::Suspended {
                        capability,
                        args,
                        resume_behavior,
                    } => {
                        self.pending_resume_behavior = resume_behavior;
                        self.suspended_host_call = Some(PendingHostCall {
                            capability: capability.clone(),
                            args: args.clone(),
                            promise: None,
                            resume_behavior,
                            traceback: self.traceback_snapshots(),
                        });
                        self.snapshot_nonce = next_snapshot_nonce();
                        return Ok(StepAction::Return(ExecutionStep::Suspended(Box::new(
                            Suspension {
                                capability,
                                args,
                                snapshot: ExecutionSnapshot::capture(self),
                            },
                        ))));
                    }
                }
            }
            Instruction::Await => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                self.suspend_async_await(value)?;
            }
            Instruction::CallWithArray {
                with_this,
                optional,
            } => {
                let args = self.pop_argument_array(frame_index)?;
                let callee = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let this_value = if *with_this {
                    self.frames[frame_index]
                        .stack
                        .pop()
                        .ok_or_else(|| MustardError::runtime("stack underflow"))?
                } else {
                    Value::Undefined
                };
                if *optional && matches!(callee, Value::Undefined | Value::Null) {
                    self.frames[frame_index].stack.push(Value::Undefined);
                    return Ok(StepAction::Continue);
                }
                match self.call_callable(callee, this_value, &args)? {
                    RunState::Completed(value) => {
                        self.frames[frame_index].stack.push(value);
                    }
                    RunState::PushedFrame => {}
                    RunState::StartedAsync(value) => {
                        self.frames[frame_index].stack.push(value);
                    }
                    RunState::Suspended {
                        capability,
                        args,
                        resume_behavior,
                    } => {
                        self.pending_resume_behavior = resume_behavior;
                        self.suspended_host_call = Some(PendingHostCall {
                            capability: capability.clone(),
                            args: args.clone(),
                            promise: None,
                            resume_behavior,
                            traceback: self.traceback_snapshots(),
                        });
                        self.snapshot_nonce = next_snapshot_nonce();
                        return Ok(StepAction::Return(ExecutionStep::Suspended(Box::new(
                            Suspension {
                                capability,
                                args,
                                snapshot: ExecutionSnapshot::capture(self),
                            },
                        ))));
                    }
                }
            }
            Instruction::Construct { argc } => {
                let args = pop_many(&mut self.frames[frame_index].stack, *argc)?;
                let callee = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let value = self.construct(callee, &args)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::ConstructWithArray => {
                let args = self.pop_argument_array(frame_index)?;
                let callee = self.frames[frame_index]
                    .stack
                    .pop()
                    .ok_or_else(|| MustardError::runtime("stack underflow"))?;
                let value = self.construct(callee, &args)?;
                self.frames[frame_index].stack.push(value);
            }
            Instruction::Return => {
                let value = self.frames[frame_index]
                    .stack
                    .pop()
                    .unwrap_or(Value::Undefined);
                return self.complete_return(value);
            }
        }
        Ok(StepAction::Continue)
    }

    pub(super) fn run(&mut self) -> MustardResult<ExecutionStep> {
        loop {
            self.check_cancellation()
                .map_err(|error| self.annotate_runtime_error(error))?;
            if self.frames.is_empty() {
                match self.process_idle_state() {
                    Ok(Some(step)) => return Ok(step),
                    Ok(None) => continue,
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                }
            }
            let action = match self.step_active_frame() {
                Ok(action) => action,
                Err(error) => match self.handle_runtime_fault(error) {
                    Ok(action) => action,
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                },
            };

            match action {
                StepAction::Continue => {}
                StepAction::Return(step) => return Ok(step),
            }
        }
    }

    pub(super) fn run_until_frame_depth(
        &mut self,
        target_depth: usize,
        host_suspension_message: &str,
    ) -> MustardResult<()> {
        while self.frames.len() > target_depth {
            self.check_cancellation()?;
            let action = match self.step_active_frame() {
                Ok(action) => action,
                Err(error) => self.handle_runtime_fault(error)?,
            };
            match action {
                StepAction::Continue => {}
                StepAction::Return(ExecutionStep::Suspended(_)) => {
                    return Err(MustardError::runtime(host_suspension_message));
                }
                StepAction::Return(ExecutionStep::Completed(_)) => {
                    return Err(MustardError::runtime(
                        "nested callback execution unexpectedly completed the program",
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_call_depth(&self) -> MustardResult<()> {
        if self.frames.len() >= self.limits.call_depth_limit {
            return Err(limit_error("call depth limit exceeded"));
        }
        Ok(())
    }

    pub(super) fn push_frame(
        &mut self,
        function_id: usize,
        env: EnvKey,
        args: &[Value],
        this_value: Value,
        async_promise: Option<PromiseKey>,
    ) -> MustardResult<()> {
        self.check_call_depth()?;
        let program = Arc::clone(&self.program);
        let function = program
            .functions
            .get(function_id)
            .ok_or_else(|| MustardError::runtime("function not found"))?;
        let this_cell = self.insert_cell(this_value, true, true)?;
        self.envs
            .get_mut(env)
            .ok_or_else(|| MustardError::runtime("environment missing"))?
            .bindings
            .insert("this".to_string(), this_cell);
        self.refresh_env_accounting(env)?;
        for binding_names in &function.param_binding_names {
            for name in binding_names {
                self.declare_name(env, name.clone(), true)?;
            }
        }
        for (index, pattern) in function.params.iter().enumerate() {
            let arg = args.get(index).cloned().unwrap_or(Value::Undefined);
            self.initialize_pattern(env, pattern, arg)?;
        }
        if let Some(rest) = &function.rest {
            for name in &function.rest_binding_names {
                self.declare_name(env, name.clone(), true)?;
            }
            let rest_array = self.insert_array(
                args.iter().skip(function.params.len()).cloned().collect(),
                IndexMap::new(),
            )?;
            self.initialize_pattern(env, rest, Value::Array(rest_array))?;
        }
        self.frames.push(Frame {
            function_id,
            ip: 0,
            env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            pending_completions: Vec::new(),
            active_finally: Vec::new(),
            async_promise,
            callback_capture: false,
        });
        Ok(())
    }

    pub(super) fn call_callable(
        &mut self,
        callee: Value,
        this_value: Value,
        args: &[Value],
    ) -> MustardResult<RunState> {
        match callee {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| MustardError::runtime("closure not found"))?;
                let env = self.new_env(Some(closure.env))?;
                let (is_async, is_arrow) = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .map(|function| (function.is_async, function.is_arrow))
                    .ok_or_else(|| MustardError::runtime("function not found"))?;
                let frame_this = if is_arrow {
                    closure.this_value.clone()
                } else {
                    this_value
                };
                if is_async {
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    self.push_frame(closure.function_id, env, args, frame_this, Some(promise))?;
                    Ok(RunState::StartedAsync(Value::Promise(promise)))
                } else {
                    self.push_frame(closure.function_id, env, args, frame_this, None)?;
                    Ok(RunState::PushedFrame)
                }
            }
            Value::BuiltinFunction(function) => match function {
                BuiltinFunction::FunctionCall => self.call_function_call(this_value, args),
                BuiltinFunction::FunctionApply => self.call_function_apply(this_value, args),
                BuiltinFunction::FunctionBind => Ok(RunState::Completed(
                    self.call_function_bind(this_value, args)?,
                )),
                _ => Ok(RunState::Completed(
                    self.call_builtin(function, this_value, args)?,
                )),
            },
            Value::Object(object)
                if self
                    .objects
                    .get(object)
                    .is_some_and(|object| matches!(object.kind, ObjectKind::BoundFunction(_))) =>
            {
                let bound = match &self
                    .objects
                    .get(object)
                    .ok_or_else(|| MustardError::runtime("object missing"))?
                    .kind
                {
                    ObjectKind::BoundFunction(bound) => bound.clone(),
                    _ => unreachable!(),
                };
                let mut combined = bound.args;
                combined.extend(args.iter().cloned());
                self.call_callable(bound.target, bound.this_value, &combined)
            }
            Value::HostFunction(capability) => {
                let resume_behavior = resume_behavior_for_capability(&capability);
                let args = args
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<MustardResult<Vec<_>>>()?;
                if self.current_async_boundary_index().is_some() {
                    let outstanding = self.pending_host_calls.len()
                        + usize::from(self.suspended_host_call.is_some());
                    if outstanding >= self.limits.max_outstanding_host_calls {
                        return Err(limit_error("outstanding host-call limit exhausted"));
                    }
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    self.pending_host_calls.push_back(PendingHostCall {
                        capability,
                        args,
                        promise: Some(promise),
                        resume_behavior,
                        traceback: self.traceback_snapshots(),
                    });
                    Ok(RunState::Completed(Value::Promise(promise)))
                } else {
                    Ok(RunState::Suspended {
                        resume_behavior,
                        capability,
                        args,
                    })
                }
            }
            _ => Err(MustardError::runtime("value is not callable")),
        }
    }

    fn call_function_call(&mut self, target: Value, args: &[Value]) -> MustardResult<RunState> {
        if !self.is_callable_value(&target)? {
            return Err(MustardError::runtime(
                "TypeError: Function.prototype.call called on incompatible receiver",
            ));
        }
        let this_arg = args.first().cloned().unwrap_or(Value::Undefined);
        self.call_callable(target, this_arg, &args[1..])
    }

    fn call_function_apply(&mut self, target: Value, args: &[Value]) -> MustardResult<RunState> {
        if !self.is_callable_value(&target)? {
            return Err(MustardError::runtime(
                "TypeError: Function.prototype.apply called on incompatible receiver",
            ));
        }
        let this_arg = args.first().cloned().unwrap_or(Value::Undefined);
        let arg_list = match args.get(1).cloned().unwrap_or(Value::Undefined) {
            Value::Undefined | Value::Null => Vec::new(),
            Value::Array(array) => self.array_argument_values(Value::Array(array))?,
            _ => {
                return Err(MustardError::runtime(
                    "TypeError: Function.prototype.apply expects an array or undefined argument list in the supported surface",
                ));
            }
        };
        self.call_callable(target, this_arg, &arg_list)
    }

    fn call_function_bind(&mut self, target: Value, args: &[Value]) -> MustardResult<Value> {
        if !self.is_callable_value(&target)? {
            return Err(MustardError::runtime(
                "TypeError: Function.prototype.bind called on incompatible receiver",
            ));
        }
        let bound_this = args.first().cloned().unwrap_or(Value::Undefined);
        let bound_args = args.iter().skip(1).cloned().collect();
        Ok(Value::Object(self.insert_object(
            IndexMap::new(),
            ObjectKind::BoundFunction(BoundFunctionData {
                target,
                this_value: bound_this,
                args: bound_args,
            }),
        )?))
    }

    fn is_callable_value(&self, value: &Value) -> MustardResult<bool> {
        Ok(match value {
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => true,
            Value::Object(object) => matches!(
                self.objects
                    .get(*object)
                    .ok_or_else(|| MustardError::runtime("object missing"))?
                    .kind,
                ObjectKind::BoundFunction(_)
            ),
            _ => false,
        })
    }

    pub(super) fn construct(&mut self, callee: Value, args: &[Value]) -> MustardResult<Value> {
        match callee {
            Value::BuiltinFunction(
                BuiltinFunction::FunctionCtor
                | BuiltinFunction::ArrayCtor
                | BuiltinFunction::DateCtor
                | BuiltinFunction::ObjectCtor
                | BuiltinFunction::MapCtor
                | BuiltinFunction::SetCtor
                | BuiltinFunction::PromiseCtor
                | BuiltinFunction::RegExpCtor
                | BuiltinFunction::ErrorCtor
                | BuiltinFunction::TypeErrorCtor
                | BuiltinFunction::ReferenceErrorCtor
                | BuiltinFunction::RangeErrorCtor
                | BuiltinFunction::SyntaxErrorCtor
                | BuiltinFunction::NumberCtor
                | BuiltinFunction::StringCtor
                | BuiltinFunction::BooleanCtor
                | BuiltinFunction::IntlDateTimeFormatCtor
                | BuiltinFunction::IntlNumberFormatCtor,
            ) => match callee {
                Value::BuiltinFunction(BuiltinFunction::FunctionCtor) => {
                    Err(MustardError::runtime(
                        "TypeError: Function constructor is unavailable in the supported surface",
                    ))
                }
                Value::BuiltinFunction(BuiltinFunction::MapCtor) => self.construct_map(args),
                Value::BuiltinFunction(BuiltinFunction::SetCtor) => self.construct_set(args),
                Value::BuiltinFunction(BuiltinFunction::DateCtor) => self.construct_date(args),
                Value::BuiltinFunction(BuiltinFunction::PromiseCtor) => {
                    self.construct_promise(args)
                }
                Value::BuiltinFunction(BuiltinFunction::RegExpCtor) => self.construct_regexp(args),
                Value::BuiltinFunction(BuiltinFunction::NumberCtor) => self.construct_number(args),
                Value::BuiltinFunction(BuiltinFunction::StringCtor) => self.construct_string(args),
                Value::BuiltinFunction(BuiltinFunction::BooleanCtor) => {
                    self.construct_boolean(args)
                }
                Value::BuiltinFunction(BuiltinFunction::IntlDateTimeFormatCtor) => {
                    self.construct_intl_date_time_format(args)
                }
                Value::BuiltinFunction(BuiltinFunction::IntlNumberFormatCtor) => {
                    self.construct_intl_number_format(args)
                }
                Value::BuiltinFunction(kind) => self.call_builtin(kind, Value::Undefined, args),
                _ => unreachable!(),
            },
            _ => Err(MustardError::runtime(
                "only conservative built-in constructors are supported in v1",
            )),
        }
    }

    fn pop_argument_array(&mut self, frame_index: usize) -> MustardResult<Vec<Value>> {
        let args = self.frames[frame_index]
            .stack
            .pop()
            .ok_or_else(|| MustardError::runtime("stack underflow"))?;
        self.array_argument_values(args)
    }

    fn array_argument_values(&self, value: Value) -> MustardResult<Vec<Value>> {
        let Value::Array(array) = value else {
            return Err(MustardError::runtime(
                "argument builder target is not an array",
            ));
        };
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| MustardError::runtime("array missing"))?
            .elements;
        Ok(elements
            .iter()
            .map(|value| value.clone().unwrap_or(Value::Undefined))
            .collect())
    }

    fn expand_iterable_into_array(
        &mut self,
        target: Value,
        iterable: Value,
    ) -> MustardResult<Value> {
        let Value::Array(array) = target else {
            return Err(MustardError::runtime(
                "array builder target is not an array",
            ));
        };
        self.with_temporary_roots(&[Value::Array(array), iterable.clone()], |runtime| {
            let iterator = runtime.create_iterator(iterable.clone())?;
            runtime.with_temporary_roots(
                &[Value::Array(array), iterable, iterator.clone()],
                |runtime| {
                    loop {
                        runtime.charge_native_helper_work(1)?;
                        let (value, done) = runtime.iterator_next(iterator.clone())?;
                        if done {
                            break;
                        }
                        runtime
                            .arrays
                            .get_mut(array)
                            .ok_or_else(|| MustardError::runtime("array missing"))?
                            .elements
                            .push(Some(value));
                        runtime.refresh_array_accounting(array)?;
                    }
                    Ok(Value::Array(array))
                },
            )
        })
    }

    fn pattern_array_index(&self, value: Value, index: usize) -> MustardResult<Value> {
        let items = self.to_array_items(value)?;
        Ok(items.get(index).cloned().unwrap_or(Value::Undefined))
    }

    fn pattern_array_rest(&mut self, value: Value, start: usize) -> MustardResult<Value> {
        let items = self.to_array_items(value)?;
        let array = self.insert_array(items.into_iter().skip(start).collect(), IndexMap::new())?;
        Ok(Value::Array(array))
    }

    fn pattern_object_rest(&mut self, value: Value, excluded: &[String]) -> MustardResult<Value> {
        let excluded: std::collections::HashSet<_> = excluded.iter().cloned().collect();
        let mut rest_object = IndexMap::new();
        match value {
            Value::Object(object) => {
                if let Some(object) = self.objects.get(object) {
                    for (key, value) in &object.properties {
                        if !excluded.contains(key) {
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
        Ok(Value::Object(
            self.insert_object(rest_object, ObjectKind::Plain)?,
        ))
    }
}
