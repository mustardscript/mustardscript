use super::super::bytecode::Instruction;
use super::{
    Compiler,
    context::{CompileContext, LoopContext},
    pattern_bindings,
};
use crate::{
    diagnostic::{MustardError, MustardResult},
    ir::{BinaryOp, BindingKind, Expr, ForInit, ForOfHead, Pattern, Stmt},
};

impl Compiler {
    pub(super) fn compile_stmt(
        &mut self,
        context: &mut CompileContext,
        statement: &Stmt,
    ) -> MustardResult<()> {
        match statement {
            Stmt::Block { body, .. } => {
                self.enter_env_scope(context);
                self.emit_block_prologue(context, body, false)?;
                for statement in body {
                    self.compile_stmt(context, statement)?;
                }
                self.exit_env_scope(context);
            }
            Stmt::VariableDecl { declarators, .. } => {
                for declarator in declarators {
                    let initializer_kind =
                        declarator.initializer.as_ref().and_then(|initializer| {
                            self.expr_known_collection_kind(context, initializer)
                        });
                    if let Some(initializer) = &declarator.initializer {
                        self.compile_expr(context, initializer)?;
                    } else {
                        context.code.push(Instruction::PushUndefined);
                    }
                    self.compile_pattern_binding(context, &declarator.pattern)?;
                    self.record_pattern_collection_kind(
                        context,
                        &declarator.pattern,
                        initializer_kind,
                    );
                }
            }
            Stmt::FunctionDecl { .. } => {}
            Stmt::Expression { expression, .. } => {
                self.compile_expr(context, expression)?;
                context.code.push(Instruction::Pop);
            }
            Stmt::If {
                test,
                consequent,
                alternate,
                ..
            } => {
                self.compile_expr(context, test)?;
                let jump_to_else = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                self.compile_stmt(context, consequent)?;
                let jump_to_end = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let else_ip = context.code.len();
                self.patch_jump(context, jump_to_else, else_ip);
                context.code.push(Instruction::Pop);
                if let Some(alternate) = alternate {
                    self.compile_stmt(context, alternate)?;
                }
                let end_ip = context.code.len();
                self.patch_jump(context, jump_to_end, end_ip);
            }
            Stmt::While { test, body, .. } => {
                let loop_start = context.code.len();
                self.compile_expr(context, test)?;
                let exit_jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                let continue_target = loop_ctx.continue_target.unwrap_or(loop_start);
                for jump in loop_ctx.continue_jumps {
                    self.patch_control_transfer(context, jump, continue_target);
                }
                context.code.push(Instruction::Jump(loop_start));
                let false_path_ip = context.code.len();
                context.code.push(Instruction::Pop);
                let loop_end = context.code.len();
                self.patch_jump(context, exit_jump, false_path_ip);
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
                }
            }
            Stmt::DoWhile { body, test, .. } => {
                let loop_start = context.code.len();
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                let continue_target = context.code.len();
                if let Some(loop_ctx) = context.loop_stack.last_mut() {
                    loop_ctx.continue_target = Some(continue_target);
                }
                self.compile_expr(context, test)?;
                let exit_jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                context.code.push(Instruction::Jump(loop_start));
                let false_path_ip = context.code.len();
                context.code.push(Instruction::Pop);
                let loop_end = context.code.len();
                self.patch_jump(context, exit_jump, false_path_ip);
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.continue_jumps {
                    self.patch_control_transfer(context, jump, continue_target);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
                }
            }
            Stmt::For {
                init,
                test,
                update,
                body,
                ..
            } => {
                self.enter_env_scope(context);
                if let Some(init) = init {
                    match init {
                        ForInit::VariableDecl {
                            kind: _,
                            declarators,
                        } => {
                            for declarator in declarators {
                                let initializer_kind =
                                    declarator.initializer.as_ref().and_then(|initializer| {
                                        self.expr_known_collection_kind(context, initializer)
                                    });
                                for (name, mutable) in pattern_bindings(&declarator.pattern) {
                                    self.emit_declare_name(context, name, mutable);
                                }
                                if let Some(initializer) = &declarator.initializer {
                                    self.compile_expr(context, initializer)?;
                                } else {
                                    context.code.push(Instruction::PushUndefined);
                                }
                                self.compile_pattern_binding(context, &declarator.pattern)?;
                                self.record_pattern_collection_kind(
                                    context,
                                    &declarator.pattern,
                                    initializer_kind,
                                );
                            }
                        }
                        ForInit::Expression(expression) => {
                            self.compile_expr(context, expression)?;
                            context.code.push(Instruction::Pop);
                        }
                    }
                }
                let loop_start = context.code.len();
                let exit_jump = if let Some(test) = test {
                    self.compile_expr(context, test)?;
                    let jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                    context.code.push(Instruction::Pop);
                    Some(jump)
                } else {
                    None
                };
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                let update_start = context.code.len();
                if let Some(loop_ctx) = context.loop_stack.last_mut() {
                    loop_ctx.continue_target = Some(update_start);
                }
                if let Some(update) = update {
                    self.compile_expr(context, update)?;
                    context.code.push(Instruction::Pop);
                }
                context.code.push(Instruction::Jump(loop_start));
                let false_path_ip = context.code.len();
                if exit_jump.is_some() {
                    context.code.push(Instruction::Pop);
                }
                let loop_end = context.code.len();
                if let Some(exit_jump) = exit_jump {
                    self.patch_jump(context, exit_jump, false_path_ip);
                }
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.continue_jumps {
                    self.patch_control_transfer(context, jump, update_start);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
                }
                self.exit_env_scope(context);
            }
            Stmt::ForOf {
                span,
                await_each,
                head,
                iterable,
                body,
            } => {
                self.enter_env_scope(context);
                let loop_scope_depth = context.scope_depth;
                let iterator_binding = self.fresh_internal_name(context, "iter");
                self.emit_declare_name(context, iterator_binding.clone(), false);
                self.compile_expr(context, iterable)?;
                context.code.push(Instruction::CreateIterator);
                context
                    .code
                    .push(Instruction::InitializePattern(Pattern::Identifier {
                        span: *span,
                        name: iterator_binding.clone(),
                    }));

                let loop_start = context.code.len();
                self.emit_load_name(context, &iterator_binding);
                context.code.push(Instruction::IteratorNext);
                let exit_jump = self.emit_jump(context, Instruction::JumpIfTrue(usize::MAX));
                context.code.push(Instruction::Pop);
                if *await_each {
                    context.code.push(Instruction::Await);
                }

                match head {
                    ForOfHead::Binding { kind, pattern } => {
                        self.enter_env_scope(context);
                        for (name, _) in pattern_bindings(pattern) {
                            self.emit_declare_name(context, name, *kind == BindingKind::Let);
                        }
                        self.compile_pattern_binding(context, pattern)?;
                        self.record_pattern_collection_kind(context, pattern, None);
                    }
                    ForOfHead::Assignment { target } => {
                        self.compile_assign_target_pattern(context, target)?;
                    }
                }
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
                    scope_depth: loop_scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                if matches!(head, ForOfHead::Binding { .. }) {
                    self.exit_env_scope(context);
                }
                let continue_target = context.code.len();
                context.code.push(Instruction::Jump(loop_start));

                let done_path_ip = context.code.len();
                context.code.push(Instruction::Pop);
                context.code.push(Instruction::Pop);
                let loop_end = context.code.len();
                self.patch_jump(context, exit_jump, done_path_ip);

                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.continue_jumps {
                    self.patch_control_transfer(context, jump, continue_target);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
                }

                self.exit_env_scope(context);
            }
            Stmt::ForIn {
                span,
                head,
                object,
                body,
            } => {
                self.compile_stmt(
                    context,
                    &Stmt::ForOf {
                        span: *span,
                        await_each: false,
                        head: head.clone(),
                        iterable: Expr::Call {
                            span: *span,
                            callee: Box::new(Expr::Member {
                                span: *span,
                                object: Box::new(Expr::Identifier {
                                    span: *span,
                                    name: "Object".to_string(),
                                }),
                                property: crate::ir::MemberProperty::Static(
                                    crate::ir::PropertyName::Identifier("keys".to_string()),
                                ),
                                optional: false,
                            }),
                            arguments: vec![crate::ir::CallArgument::Value(object.clone())],
                            optional: false,
                        },
                        body: body.clone(),
                    },
                )?;
            }
            Stmt::Break { span } => {
                let Some(loop_ctx) = context.loop_stack.last() else {
                    return Err(MustardError::runtime_at(
                        "`break` used outside of a loop",
                        *span,
                    ));
                };
                let patch =
                    self.emit_jump_transfer(context, loop_ctx.handler_depth, loop_ctx.scope_depth);
                context
                    .loop_stack
                    .last_mut()
                    .expect("loop context should still exist")
                    .break_jumps
                    .push(patch);
            }
            Stmt::Continue { span } => {
                let Some(loop_ctx) = context.loop_stack.last() else {
                    return Err(MustardError::runtime_at(
                        "`continue` used outside of a loop",
                        *span,
                    ));
                };
                let patch =
                    self.emit_jump_transfer(context, loop_ctx.handler_depth, loop_ctx.scope_depth);
                context
                    .loop_stack
                    .last_mut()
                    .expect("loop context should still exist")
                    .continue_jumps
                    .push(patch);
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.compile_expr(context, value)?;
                } else {
                    context.code.push(Instruction::PushUndefined);
                }
                self.emit_return(context);
            }
            Stmt::Throw { span, value } => {
                self.compile_expr(context, value)?;
                if let Some(active_finally) = context.active_finally.last() {
                    self.emit_scope_cleanup(context, active_finally.scope_depth);
                    context.code.push(Instruction::PushPendingThrow);
                    self.emit_jump_to_active_finally_exit(context);
                } else {
                    context.code.push(Instruction::Throw { span: *span });
                }
            }
            Stmt::Try {
                body,
                catch,
                finally,
                ..
            } => {
                self.compile_try(context, body, catch.as_ref(), finally.as_deref())?;
            }
            Stmt::Switch {
                discriminant,
                cases,
                ..
            } => {
                self.compile_expr(context, discriminant)?;
                let mut case_jumps = Vec::new();
                let mut default_case_index = None;
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                for (case_index, case) in cases.iter().enumerate() {
                    if let Some(test) = &case.test {
                        context.code.push(Instruction::Dup);
                        self.compile_expr(context, test)?;
                        context.code.push(Instruction::Binary(BinaryOp::StrictEq));
                        let miss_jump =
                            self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        case_jumps.push(self.emit_jump(context, Instruction::Jump(usize::MAX)));
                        let miss_ip = context.code.len();
                        self.patch_jump(context, miss_jump, miss_ip);
                        context.code.push(Instruction::Pop);
                    } else {
                        default_case_index = Some(case_index);
                    }
                }
                context.code.push(Instruction::Pop);
                let jump_past_cases = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let mut case_offsets = Vec::new();
                for case in cases {
                    let case_start = context.code.len();
                    case_offsets.push(case_start);
                    for statement in &case.consequent {
                        self.compile_stmt(context, statement)?;
                    }
                }
                let end_ip = context.code.len();
                let default_target = default_case_index
                    .and_then(|index| case_offsets.get(index).copied())
                    .unwrap_or(end_ip);
                self.patch_jump(context, jump_past_cases, default_target);
                for (jump, target) in case_jumps.into_iter().zip(case_offsets.iter().copied()) {
                    self.patch_jump(context, jump, target);
                }
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, end_ip);
                }
            }
            Stmt::Empty { .. } => {}
        }
        Ok(())
    }
}
