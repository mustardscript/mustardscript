use super::*;
use num_traits::FromPrimitive;

impl Runtime {
    /// IsLooselyEqual without invoking guest code. None means an object must be
    /// converted to a primitive by the VM, which can suspend and resume that call.
    pub(in crate::runtime) fn loose_equality(
        &mut self,
        left: &Value,
        right: &Value,
    ) -> MustardResult<Option<bool>> {
        let equal = match (left, right) {
            (Value::String(a), Value::String(b)) => {
                self.charge_native_helper_work(a.len().min(b.len()))?;
                a == b
            }
            (Value::BigInt(a), Value::BigInt(b)) => {
                self.charge_native_helper_work(bigint_comparison_work(a, b))?;
                a == b
            }
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Bool(value), other) | (other, Value::Bool(value)) => {
                return self.loose_equality(&Value::Number(f64::from(u8::from(*value))), other);
            }
            (Value::Number(number), Value::String(text))
            | (Value::String(text), Value::Number(number)) => {
                *number == self.coerce_number(Value::String(text.clone()))?
            }
            (Value::BigInt(integer), Value::String(text))
            | (Value::String(text), Value::BigInt(integer)) => {
                let Some(parsed) = self.parse_string_bigint(text)? else {
                    return Ok(Some(false));
                };
                self.charge_native_helper_work(bigint_comparison_work(integer, &parsed))?;
                *integer == parsed
            }
            (Value::BigInt(integer), Value::Number(number))
            | (Value::Number(number), Value::BigInt(integer)) => {
                // Converting the BigInt to f64 would incorrectly equate adjacent
                // integers beyond 2^53. Convert the exact integral f64 instead.
                if !number.is_finite() || number.fract() != 0.0 {
                    false
                } else {
                    let converted = BigInt::from_f64(*number).ok_or_else(|| {
                        MustardError::runtime("invalid finite integer during equality")
                    })?;
                    self.charge_native_helper_work(bigint_comparison_work(integer, &converted))?;
                    *integer == converted
                }
            }
            _ if left.is_primitive() != right.is_primitive() => return Ok(None),
            // All object representations have ECMAScript's Object type: identity
            // comparison never invokes coercion, even across different tags.
            _ => strict_equal(left, right),
        };
        Ok(Some(equal))
    }

    pub(in crate::runtime) fn start_loose_equality(
        &mut self,
        frame_index: usize,
        operator: BinaryOp,
        left: Value,
        right: Value,
    ) -> MustardResult<StepAction> {
        let negate = matches!(operator, BinaryOp::NotEq);
        if let Some(equal) = self.loose_equality(&left, &right)? {
            self.frames[frame_index]
                .stack
                .push(Value::Bool(equal != negate));
            return Ok(StepAction::Continue);
        }
        let (object, primitive) = if left.is_primitive() {
            (right, left)
        } else {
            (left, right)
        };
        let prefer_string = matches!(&object, Value::Object(id)
            if self.objects.get(*id).is_some_and(|o| matches!(o.kind, ObjectKind::Date(_))));
        self.frames[frame_index].pending_equality = Some(EqualityContinuation {
            primitive,
            negate,
            work: vec![CoercionWork::Primitive {
                object,
                prefer_string,
                next_method: 0,
                awaiting_result: false,
            }],
        });
        self.continue_loose_equality(frame_index)
    }

    pub(in crate::runtime) fn continue_loose_equality(
        &mut self,
        frame_index: usize,
    ) -> MustardResult<StepAction> {
        // Move (do not clone) accumulated string buffers. The explicit work stack
        // is bounded, rooted, and serializable, unlike a recursive Rust traversal.
        let mut state = self.frames[frame_index]
            .pending_equality
            .take()
            .ok_or_else(|| MustardError::runtime("missing equality continuation"))?;
        let roots: Vec<_> = state.work.iter().map(CoercionWork::root).collect();
        let action = self.with_temporary_roots(&roots, |runtime| {
            runtime.step_equality_coercion(frame_index, &mut state)
        })?;
        if state.work.len() > 256 {
            return Err(MustardError::runtime(
                "RangeError: string conversion nesting limit exceeded",
            ));
        }
        match action {
            CoercionAction::Complete(equal) => {
                self.frames[frame_index]
                    .stack
                    .push(Value::Bool(equal != state.negate));
                Ok(StepAction::Continue)
            }
            CoercionAction::Continue => {
                self.frames[frame_index].pending_equality = Some(state);
                Ok(StepAction::Continue)
            }
            CoercionAction::Call(callee, receiver, args) => {
                self.frames[frame_index].pending_equality = Some(state);
                // The method may delete or replace itself during execution.
                let mut call_roots = args.clone();
                call_roots.extend([callee.clone(), receiver.clone()]);
                let call = self.with_temporary_roots(&call_roots, |runtime| {
                    runtime.call_callable(callee, receiver, &args)
                })?;
                self.finish_call_step(frame_index, call)
            }
        }
    }

    fn step_equality_coercion(
        &mut self,
        frame_index: usize,
        state: &mut EqualityContinuation,
    ) -> MustardResult<CoercionAction> {
        let work = state
            .work
            .pop()
            .ok_or_else(|| MustardError::runtime("missing coercion work"))?;
        match work {
            CoercionWork::Primitive {
                object,
                prefer_string,
                mut next_method,
                awaiting_result,
            } => {
                if awaiting_result {
                    let result = self.frames[frame_index]
                        .stack
                        .pop()
                        .ok_or_else(|| MustardError::runtime("missing equality coercion result"))?;
                    if result.is_primitive() {
                        if state.work.is_empty() {
                            let equal = self
                                .loose_equality(&result, &state.primitive)?
                                .ok_or_else(|| {
                                    MustardError::runtime(
                                        "equality coercion did not produce a primitive",
                                    )
                                })?;
                            return Ok(CoercionAction::Complete(equal));
                        }
                        self.frames[frame_index].stack.push(result);
                        return Ok(CoercionAction::Continue);
                    }
                }
                let methods = if prefer_string {
                    ["toString", "valueOf"]
                } else {
                    ["valueOf", "toString"]
                };
                while let Some(method) = methods.get(usize::from(next_method)) {
                    next_method += 1;
                    let callee = self.get_property_static(object.clone(), method, false)?;
                    if !self.is_callable_value(&callee)? {
                        continue;
                    }
                    state.work.push(CoercionWork::Primitive {
                        object: object.clone(),
                        prefer_string,
                        next_method,
                        awaiting_result: true,
                    });
                    let (mut callee, receiver, mut args) =
                        self.resolve_coercion_method(callee, object)?;
                    // The default Array.toString delegates to the current join
                    // method. Walk a default join in VM state so element ToString
                    // hooks can mutate the array, throw, or suspend host work.
                    if matches!(
                        callee,
                        Value::BuiltinFunction(BuiltinFunction::ArrayToString)
                    ) {
                        callee = self.get_property_static(receiver.clone(), "join", false)?;
                        if !self.is_callable_value(&callee)? {
                            let value = self.call_object_to_string(receiver)?;
                            self.frames[frame_index].stack.push(value);
                            return Ok(CoercionAction::Continue);
                        }
                        args.clear();
                    }
                    if matches!(callee, Value::BuiltinFunction(BuiltinFunction::ArrayJoin)) {
                        if matches!(&receiver, Value::Object(id) if self.objects.get(*id).is_some_and(|o| matches!(o.kind, ObjectKind::FunctionPrototype(Value::BuiltinFunction(BuiltinFunction::ArrayCtor)))))
                        {
                            self.frames[frame_index]
                                .stack
                                .push(Value::String(String::new()));
                            return Ok(CoercionAction::Continue);
                        }
                        let array = self.array_receiver(receiver, "join")?;
                        let joining = |work: &CoercionWork| matches!(work, CoercionWork::ArrayJoin { array: active, .. } if *active == array);
                        if state.work.iter().any(joining)
                            || self.frames.iter().any(|frame| {
                                frame
                                    .pending_equality
                                    .as_ref()
                                    .is_some_and(|s| s.work.iter().any(joining))
                            })
                        {
                            self.frames[frame_index]
                                .stack
                                .push(Value::String(String::new()));
                        } else {
                            if state.work.len() >= 256 {
                                return Err(MustardError::runtime(
                                    "RangeError: array string conversion nesting limit exceeded",
                                ));
                            }
                            let length = self.array_length(array)?;
                            let separator_value = args.first().cloned().unwrap_or(Value::Undefined);
                            let separator = if matches!(separator_value, Value::Undefined) {
                                Some(",".into())
                            } else if separator_value.is_primitive() {
                                Some(self.to_string(separator_value.clone())?)
                            } else {
                                None
                            };
                            let coerce_separator = separator.is_none();
                            state.work.push(CoercionWork::ArrayJoin {
                                array,
                                length,
                                next_index: 0,
                                text: String::new(),
                                separator,
                                awaiting_element: false,
                            });
                            if coerce_separator {
                                self.push_equality_string_coercion(state, separator_value)?;
                            }
                        }
                        return Ok(CoercionAction::Continue);
                    }
                    if matches!(
                        callee,
                        Value::BuiltinFunction(BuiltinFunction::RegExpToString)
                    ) {
                        if receiver.is_primitive() {
                            return Err(MustardError::runtime(
                                "TypeError: RegExp.toString requires an object receiver",
                            ));
                        }
                        state.work.push(CoercionWork::RegExpString {
                            receiver,
                            source: None,
                            awaiting_result: false,
                        });
                        return Ok(CoercionAction::Continue);
                    }
                    return Ok(CoercionAction::Call(callee, receiver, args));
                }
                Err(MustardError::runtime(
                    "TypeError: cannot convert object to primitive value",
                ))
            }
            CoercionWork::ArrayJoin {
                array,
                length,
                mut next_index,
                mut text,
                mut separator,
                awaiting_element,
            } => {
                if separator.is_none() {
                    let value = self.frames[frame_index].stack.pop().ok_or_else(|| {
                        MustardError::runtime("missing join separator coercion result")
                    })?;
                    if !value.is_primitive() {
                        return Err(MustardError::runtime(
                            "invalid join separator coercion result",
                        ));
                    }
                    separator = Some(self.to_string(value)?);
                }
                if awaiting_element {
                    let value = self.frames[frame_index].stack.pop().ok_or_else(|| {
                        MustardError::runtime("missing array element coercion result")
                    })?;
                    if !value.is_primitive() {
                        return Err(MustardError::runtime(
                            "invalid array element coercion result",
                        ));
                    }
                    let part = self.to_string(value)?;
                    self.append_equality_join(state, &mut text, &part)?;
                }
                if next_index == length {
                    self.frames[frame_index].stack.push(Value::String(text));
                    return Ok(CoercionAction::Continue);
                }
                if next_index != 0 {
                    self.append_equality_join(
                        state,
                        &mut text,
                        separator.as_deref().unwrap_or(","),
                    )?;
                }
                let value = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| MustardError::runtime("array missing"))?
                    .elements
                    .get(next_index)
                    .cloned()
                    .flatten()
                    .unwrap_or(Value::Undefined);
                next_index += 1;
                let needs_coercion = !value.is_primitive();
                if !needs_coercion && !matches!(value, Value::Null | Value::Undefined) {
                    let part = self.to_string(value.clone())?;
                    self.append_equality_join(state, &mut text, &part)?;
                }
                state.work.push(CoercionWork::ArrayJoin {
                    array,
                    length,
                    next_index,
                    text,
                    separator,
                    awaiting_element: needs_coercion,
                });
                if needs_coercion {
                    self.push_equality_string_coercion(state, value)?;
                }
                Ok(CoercionAction::Continue)
            }
            CoercionWork::RegExpString {
                receiver,
                mut source,
                awaiting_result,
            } => {
                let value = if awaiting_result {
                    self.frames[frame_index].stack.pop().ok_or_else(|| {
                        MustardError::runtime("missing RegExp string coercion result")
                    })?
                } else {
                    self.get_property_static(
                        receiver.clone(),
                        if source.is_some() { "flags" } else { "source" },
                        false,
                    )?
                };
                if !value.is_primitive() {
                    state.work.push(CoercionWork::RegExpString {
                        receiver,
                        source,
                        awaiting_result: true,
                    });
                    self.push_equality_string_coercion(state, value)?;
                } else {
                    let part = self.to_string(value)?;
                    if let Some(pattern) = source {
                        let bytes = pattern.len().saturating_add(part.len()).saturating_add(2);
                        self.charge_native_helper_work(bytes)?;
                        self.ensure_heap_capacity(bytes)?;
                        self.frames[frame_index]
                            .stack
                            .push(Value::String(format!("/{pattern}/{part}")));
                    } else {
                        source = Some(part);
                        state.work.push(CoercionWork::RegExpString {
                            receiver,
                            source,
                            awaiting_result: false,
                        });
                    }
                }
                Ok(CoercionAction::Continue)
            }
        }
    }

    fn push_equality_string_coercion(
        &self,
        state: &mut EqualityContinuation,
        object: Value,
    ) -> MustardResult<()> {
        if state.work.len() >= 256 {
            return Err(MustardError::runtime(
                "RangeError: string conversion nesting limit exceeded",
            ));
        }
        state.work.push(CoercionWork::Primitive {
            object,
            prefer_string: true,
            next_method: 0,
            awaiting_result: false,
        });
        Ok(())
    }

    fn resolve_coercion_method(
        &mut self,
        mut callee: Value,
        mut receiver: Value,
    ) -> MustardResult<(Value, Value, Vec<Value>)> {
        let mut args = Vec::new();
        let mut depth = 0;
        while let Value::Object(id) = &callee {
            let Some(ObjectKind::BoundFunction(bound)) = self.objects.get(*id).map(|o| &o.kind)
            else {
                break;
            };
            let bound = bound.clone();
            depth += 1;
            if depth > self.limits.call_depth_limit {
                return Err(limit_error("call depth limit exceeded"));
            }
            self.charge_native_helper_work(bound.args.len().saturating_add(1))?;
            let mut combined = bound.args;
            combined.extend(args);
            args = combined;
            receiver = bound.this_value;
            callee = bound.target;
        }
        Ok((callee, receiver, args))
    }

    fn append_equality_join(
        &mut self,
        state: &EqualityContinuation,
        text: &mut String,
        part: &str,
    ) -> MustardResult<()> {
        self.charge_native_helper_work(part.len())?;
        let pending_bytes = state.work.iter().fold(0usize, |bytes, work| match work {
            CoercionWork::ArrayJoin { text, .. } => bytes.saturating_add(text.len()),
            _ => bytes,
        });
        self.ensure_heap_capacity(
            pending_bytes
                .saturating_add(text.len())
                .saturating_add(part.len()),
        )?;
        text.push_str(part);
        Ok(())
    }
}

enum CoercionAction {
    Continue,
    Complete(bool),
    Call(Value, Value, Vec<Value>),
}

fn bigint_comparison_work(left: &BigInt, right: &BigInt) -> usize {
    usize::try_from(left.bits().min(right.bits()).div_ceil(8)).unwrap_or(usize::MAX)
}
