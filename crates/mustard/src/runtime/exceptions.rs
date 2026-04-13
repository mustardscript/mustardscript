use super::*;

impl Runtime {
    pub(super) fn traceback_frames(&self) -> Vec<TraceFrame> {
        self.frames
            .iter()
            .rev()
            .filter_map(|frame| {
                self.program
                    .functions
                    .get(frame.function_id)
                    .map(|function| TraceFrame {
                        function_name: function.name.clone(),
                        span: function.span,
                    })
            })
            .collect()
    }

    pub(super) fn annotate_runtime_error(&self, error: MustardError) -> MustardError {
        error.with_traceback(self.traceback_frames())
    }

    pub(super) fn traceback_snapshots(&self) -> Vec<TraceFrameSnapshot> {
        self.traceback_frames()
            .into_iter()
            .map(|frame| TraceFrameSnapshot {
                function_name: frame.function_name,
                span: frame.span,
            })
            .collect()
    }

    pub(super) fn compose_traceback(&self, origin: &[TraceFrameSnapshot]) -> Vec<TraceFrame> {
        let mut frames = self.traceback_frames();
        for frame in origin {
            let candidate = TraceFrame {
                function_name: frame.function_name.clone(),
                span: frame.span,
            };
            if !frames.iter().any(|existing| {
                existing.function_name == candidate.function_name && existing.span == candidate.span
            }) {
                frames.push(candidate);
            }
        }
        frames
    }

    pub(super) fn runtime_error_to_promise_rejection(
        &mut self,
        error: MustardError,
    ) -> MustardResult<PromiseRejection> {
        match error {
            MustardError::Message {
                kind: DiagnosticKind::Runtime,
                message,
                span,
                traceback,
            } => Ok(PromiseRejection {
                value: self.value_from_runtime_message(&message)?,
                span,
                traceback: if traceback.is_empty() {
                    self.traceback_snapshots()
                } else {
                    traceback
                        .into_iter()
                        .map(|frame| TraceFrameSnapshot {
                            function_name: frame.function_name,
                            span: frame.span,
                        })
                        .collect()
                },
            }),
            other => Err(other),
        }
    }

    pub(super) fn reject_promise_from_error(
        &mut self,
        target: PromiseKey,
        error: MustardError,
    ) -> MustardResult<()> {
        let rejection = self.runtime_error_to_promise_rejection(error)?;
        self.reject_promise(target, rejection)
    }

    pub(super) fn root_error_from_rejection(
        &self,
        rejection: PromiseRejection,
    ) -> MustardResult<MustardError> {
        Ok(MustardError::Message {
            kind: DiagnosticKind::Runtime,
            message: self.render_exception(&rejection.value)?,
            span: rejection.span,
            traceback: self.compose_traceback(&rejection.traceback),
        })
    }

    pub(super) fn handle_runtime_fault(
        &mut self,
        error: MustardError,
    ) -> MustardResult<StepAction> {
        match error {
            MustardError::Message {
                kind: DiagnosticKind::Runtime,
                message,
                span,
                ..
            } => {
                if message == INTERNAL_CALLBACK_THROW_MARKER {
                    let rejection = self.pending_internal_exception.take().ok_or_else(|| {
                        MustardError::runtime("missing internal callback exception state")
                    })?;
                    return self.raise_exception_with_origin(
                        rejection.value,
                        rejection.span,
                        Some(rejection.traceback),
                    );
                }
                let value = self.value_from_runtime_message(&message)?;
                self.raise_exception(value, span)
            }
            other => Err(other),
        }
    }

    pub(super) fn store_completion(
        &mut self,
        frame_index: usize,
        completion: CompletionRecord,
    ) -> MustardResult<()> {
        let completion_index = self.frames[frame_index]
            .active_finally
            .last()
            .map(|active| active.completion_index);
        if let Some(completion_index) = completion_index {
            if completion_index >= self.frames[frame_index].pending_completions.len() {
                return Err(MustardError::runtime(
                    "active finally references missing completion",
                ));
            }
            self.frames[frame_index].pending_completions[completion_index] = completion;
        } else {
            self.frames[frame_index]
                .pending_completions
                .push(completion);
        }
        Ok(())
    }

    pub(super) fn restore_handler_state(
        &mut self,
        frame_index: usize,
        handler: &ExceptionHandler,
    ) -> MustardResult<()> {
        let frame = &mut self.frames[frame_index];
        frame.env = handler.env;
        frame.scope_stack.truncate(handler.scope_stack_len);
        frame.stack.truncate(handler.stack_len);
        Ok(())
    }

    pub(super) fn raise_exception(
        &mut self,
        value: Value,
        span: Option<SourceSpan>,
    ) -> MustardResult<StepAction> {
        self.raise_exception_with_origin(value, span, None)
    }

    pub(super) fn raise_exception_with_origin(
        &mut self,
        value: Value,
        span: Option<SourceSpan>,
        origin_traceback: Option<Vec<TraceFrameSnapshot>>,
    ) -> MustardResult<StepAction> {
        let traceback = match origin_traceback.as_ref() {
            Some(origin) => self.compose_traceback(origin),
            None => self.traceback_frames(),
        };
        let thrown = value;

        loop {
            let Some(frame_index) = self.frames.len().checked_sub(1) else {
                return Err(MustardError::runtime("vm lost all frames"));
            };

            if let Some(handler_index) = self.frames[frame_index]
                .handlers
                .iter()
                .rposition(|handler| handler.catch.is_some() || handler.finally.is_some())
            {
                let handler = self.frames[frame_index].handlers[handler_index].clone();
                self.frames[frame_index].handlers.truncate(handler_index);
                self.restore_handler_state(frame_index, &handler)?;

                if let Some(catch_ip) = handler.catch {
                    self.frames[frame_index].pending_exception = Some(thrown);
                    self.frames[frame_index].ip = catch_ip;
                    return Ok(StepAction::Continue);
                }

                if let Some(finally_ip) = handler.finally {
                    self.frames[frame_index]
                        .pending_completions
                        .push(CompletionRecord::Throw(thrown));
                    self.frames[frame_index].ip = finally_ip;
                    return Ok(StepAction::Continue);
                }
            }

            if let Some(active) = self.frames[frame_index].active_finally.last().cloned() {
                if active.completion_index >= self.frames[frame_index].pending_completions.len() {
                    return Err(MustardError::runtime(
                        "active finally references missing completion",
                    ));
                }
                self.frames[frame_index].pending_completions[active.completion_index] =
                    CompletionRecord::Throw(thrown);
                self.frames[frame_index].ip = active.exit;
                return Ok(StepAction::Continue);
            }

            if let Some(async_promise) = self.frames[frame_index].async_promise {
                self.frames.pop();
                self.reject_promise(
                    async_promise,
                    PromiseRejection {
                        value: thrown,
                        span,
                        traceback: traceback
                            .iter()
                            .map(|frame| TraceFrameSnapshot {
                                function_name: frame.function_name.clone(),
                                span: frame.span,
                            })
                            .collect(),
                    },
                )?;
                return Ok(StepAction::Continue);
            }

            if self.frames[frame_index].callback_capture {
                self.frames.pop();
                self.pending_internal_exception = Some(PromiseRejection {
                    value: thrown,
                    span,
                    traceback: traceback
                        .iter()
                        .map(|frame| TraceFrameSnapshot {
                            function_name: frame.function_name.clone(),
                            span: frame.span,
                        })
                        .collect(),
                });
                return Ok(StepAction::Continue);
            }

            if self.frames.len() == 1 {
                let message = self.render_exception(&thrown)?;
                return Err(MustardError::Message {
                    kind: DiagnosticKind::Runtime,
                    message,
                    span,
                    traceback,
                });
            }

            self.frames.pop();
        }
    }

    pub(super) fn resume_completion(
        &mut self,
        completion: CompletionRecord,
    ) -> MustardResult<StepAction> {
        match completion {
            CompletionRecord::Throw(value) => self.raise_exception(value, None),
            CompletionRecord::Jump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => self.resume_nonthrow_completion(
                target_handler_depth,
                target_scope_depth,
                CompletionRecord::Jump {
                    target,
                    target_handler_depth,
                    target_scope_depth,
                },
            ),
            CompletionRecord::Return(value) => {
                self.resume_nonthrow_completion(0, 0, CompletionRecord::Return(value))
            }
        }
    }

    pub(super) fn resume_nonthrow_completion(
        &mut self,
        target_handler_depth: usize,
        target_scope_depth: usize,
        completion: CompletionRecord,
    ) -> MustardResult<StepAction> {
        let frame_index = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| MustardError::runtime("vm lost all frames"))?;
        let current_depth = self.frames[frame_index].handlers.len();
        if target_handler_depth > current_depth {
            return Err(MustardError::runtime(
                "completion targets missing handler depth",
            ));
        }
        if target_scope_depth > self.frames[frame_index].scope_stack.len() {
            return Err(MustardError::runtime(
                "completion targets missing scope depth",
            ));
        }

        let restore_state = if target_handler_depth < current_depth {
            self.frames[frame_index]
                .handlers
                .get(target_handler_depth)
                .cloned()
        } else {
            None
        };

        if let Some(handler_index) = (target_handler_depth..current_depth)
            .rev()
            .find(|index| self.frames[frame_index].handlers[*index].finally.is_some())
        {
            let handler = self.frames[frame_index].handlers[handler_index].clone();
            self.frames[frame_index].handlers.truncate(handler_index);
            self.restore_handler_state(frame_index, &handler)?;
            self.frames[frame_index]
                .pending_completions
                .push(completion);
            self.frames[frame_index].ip = handler
                .finally
                .ok_or_else(|| MustardError::runtime("missing finally target"))?;
            return Ok(StepAction::Continue);
        }

        if let Some(handler) = restore_state.as_ref() {
            self.restore_handler_state(frame_index, handler)?;
        }
        self.frames[frame_index]
            .handlers
            .truncate(target_handler_depth);

        match completion {
            CompletionRecord::Jump { target, .. } => {
                if self.frames[frame_index].scope_stack.len() < target_scope_depth {
                    return Err(MustardError::runtime(
                        "completion targets missing scope depth",
                    ));
                }
                while self.frames[frame_index].scope_stack.len() > target_scope_depth {
                    let restored = self.frames[frame_index]
                        .scope_stack
                        .pop()
                        .ok_or_else(|| MustardError::runtime("scope stack underflow"))?;
                    self.frames[frame_index].env = restored;
                }
                self.frames[frame_index].ip = target;
                Ok(StepAction::Continue)
            }
            CompletionRecord::Return(value) => self.complete_return(value),
            CompletionRecord::Throw(_) => unreachable!(),
        }
    }

    pub(super) fn complete_return(&mut self, value: Value) -> MustardResult<StepAction> {
        let frame = self
            .frames
            .pop()
            .ok_or_else(|| MustardError::runtime("vm lost all frames"))?;
        if frame.callback_capture {
            self.pending_sync_callback_result = Some(value);
            return Ok(StepAction::Continue);
        }
        if let Some(async_promise) = frame.async_promise {
            self.resolve_promise(async_promise, value)?;
            return Ok(StepAction::Continue);
        }
        if let Some(parent) = self.frames.last_mut() {
            parent.stack.push(value);
            Ok(StepAction::Continue)
        } else {
            self.root_result = Some(value);
            Ok(StepAction::Continue)
        }
    }
}
