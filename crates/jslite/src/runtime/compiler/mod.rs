mod bindings;

use std::collections::HashSet;

use bindings::{assign_op_to_binary, collect_block_bindings};

use crate::{
    diagnostic::{JsliteError, JsliteResult},
    ir::{
        AssignOp, AssignTarget, BinaryOp, BindingKind, CompiledProgram, Expr, ForInit,
        FunctionExpr, MemberProperty, Pattern, PropertyName, Stmt,
    },
    span::SourceSpan,
};

use super::{
    bytecode::{BytecodeProgram, FunctionPrototype, Instruction},
    format_number_key,
    validation::validate_bytecode_program,
};

pub(super) fn pattern_bindings(pattern: &Pattern) -> Vec<(String, bool)> {
    bindings::pattern_bindings(pattern)
}

pub fn lower_to_bytecode(program: &CompiledProgram) -> JsliteResult<BytecodeProgram> {
    let mut compiler = Compiler::default();
    let root = compiler.compile_root(&program.script.body, program.script.span)?;
    let program = BytecodeProgram {
        functions: compiler.functions,
        root,
    };
    validate_bytecode_program(&program)?;
    Ok(program)
}

#[derive(Debug, Default)]
struct Compiler {
    functions: Vec<FunctionPrototype>,
}

#[derive(Debug, Default)]
struct CompileContext {
    code: Vec<Instruction>,
    loop_stack: Vec<LoopContext>,
    active_handlers: Vec<ActiveHandlerContext>,
    active_finally: Vec<ActiveFinallyContext>,
    finally_regions: Vec<FinallyRegionContext>,
    scope_depth: usize,
    internal_name_counter: usize,
}

#[derive(Debug, Default)]
struct LoopContext {
    break_jumps: Vec<ControlTransferPatch>,
    continue_jumps: Vec<ControlTransferPatch>,
    continue_target: Option<usize>,
    handler_depth: usize,
    scope_depth: usize,
}

#[derive(Debug, Clone, Copy)]
struct ActiveHandlerContext {
    finally_region: Option<usize>,
    scope_depth: usize,
}

#[derive(Debug, Default)]
struct FinallyRegionContext {
    handler_sites: Vec<usize>,
    jump_sites: Vec<usize>,
}

#[derive(Debug, Default)]
struct ActiveFinallyContext {
    exit_patch_site: usize,
    jump_sites: Vec<usize>,
    scope_depth: usize,
}

#[derive(Debug, Clone, Copy)]
enum ControlTransferPatch {
    DirectJump(usize),
    PendingJump(usize),
}

impl Compiler {
    fn compile_root(&mut self, statements: &[Stmt], span: SourceSpan) -> JsliteResult<usize> {
        let mut context = CompileContext::default();
        self.emit_block_prologue(&mut context, statements)?;
        let mut produced_result = false;
        for (index, statement) in statements.iter().enumerate() {
            let is_last = index + 1 == statements.len();
            if is_last && let Stmt::Expression { expression, .. } = statement {
                self.compile_expr(&mut context, expression)?;
                produced_result = true;
                continue;
            }
            self.compile_stmt(&mut context, statement)?;
        }
        if !produced_result {
            context.code.push(Instruction::PushUndefined);
        }
        context.code.push(Instruction::Return);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name: None,
            params: Vec::new(),
            rest: None,
            code: context.code,
            is_async: false,
            is_arrow: false,
            span,
        });
        Ok(id)
    }

    fn compile_function(&mut self, function: &FunctionExpr) -> JsliteResult<usize> {
        self.compile_function_body(function)
    }

    fn compile_function_body(&mut self, function: &FunctionExpr) -> JsliteResult<usize> {
        let mut context = CompileContext::default();
        self.emit_block_prologue(&mut context, &function.body)?;
        for statement in &function.body {
            self.compile_stmt(&mut context, statement)?;
        }
        context.code.push(Instruction::PushUndefined);
        context.code.push(Instruction::Return);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name: function.name.clone(),
            params: function.params.clone(),
            rest: function.rest.clone(),
            code: context.code,
            is_async: function.is_async,
            is_arrow: function.is_arrow,
            span: function.span,
        });
        Ok(id)
    }

    fn emit_block_prologue(
        &mut self,
        context: &mut CompileContext,
        statements: &[Stmt],
    ) -> JsliteResult<()> {
        let mut declared = HashSet::new();
        let bindings = collect_block_bindings(statements);
        for (name, mutable) in bindings.lexicals {
            if declared.insert(name.clone()) {
                context
                    .code
                    .push(Instruction::DeclareName { name, mutable });
            }
        }
        for function in bindings.functions {
            if declared.insert(function.name.clone()) {
                context.code.push(Instruction::DeclareName {
                    name: function.name.clone(),
                    mutable: false,
                });
            }
            context.code.push(Instruction::MakeClosure {
                function_id: self.compile_function(&function.expr)?,
            });
            context
                .code
                .push(Instruction::InitializePattern(Pattern::Identifier {
                    span: function.expr.span,
                    name: function.name,
                }));
        }
        Ok(())
    }

    fn fresh_internal_name(&self, context: &mut CompileContext, prefix: &str) -> String {
        let name = format!("\0jslite_{prefix}_{}", context.internal_name_counter);
        context.internal_name_counter += 1;
        name
    }

    fn compile_stmt(&mut self, context: &mut CompileContext, statement: &Stmt) -> JsliteResult<()> {
        match statement {
            Stmt::Block { body, .. } => {
                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                self.emit_block_prologue(context, body)?;
                for statement in body {
                    self.compile_stmt(context, statement)?;
                }
                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
            }
            Stmt::VariableDecl { declarators, .. } => {
                for declarator in declarators {
                    if let Some(initializer) = &declarator.initializer {
                        self.compile_expr(context, initializer)?;
                    } else {
                        context.code.push(Instruction::PushUndefined);
                    }
                    context
                        .code
                        .push(Instruction::InitializePattern(declarator.pattern.clone()));
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
                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                if let Some(init) = init {
                    match init {
                        ForInit::VariableDecl {
                            kind: _,
                            declarators,
                        } => {
                            for declarator in declarators {
                                for (name, mutable) in pattern_bindings(&declarator.pattern) {
                                    context
                                        .code
                                        .push(Instruction::DeclareName { name, mutable });
                                }
                                if let Some(initializer) = &declarator.initializer {
                                    self.compile_expr(context, initializer)?;
                                } else {
                                    context.code.push(Instruction::PushUndefined);
                                }
                                context.code.push(Instruction::InitializePattern(
                                    declarator.pattern.clone(),
                                ));
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
                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
            }
            Stmt::ForOf {
                span,
                kind,
                pattern,
                iterable,
                body,
            } => {
                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                let loop_scope_depth = context.scope_depth;
                let iterator_binding = self.fresh_internal_name(context, "iter");
                context.code.push(Instruction::DeclareName {
                    name: iterator_binding.clone(),
                    mutable: false,
                });
                self.compile_expr(context, iterable)?;
                context.code.push(Instruction::CreateIterator);
                context
                    .code
                    .push(Instruction::InitializePattern(Pattern::Identifier {
                        span: *span,
                        name: iterator_binding.clone(),
                    }));

                let loop_start = context.code.len();
                context
                    .code
                    .push(Instruction::LoadName(iterator_binding.clone()));
                context.code.push(Instruction::IteratorNext);
                let exit_jump = self.emit_jump(context, Instruction::JumpIfTrue(usize::MAX));
                context.code.push(Instruction::Pop);

                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                for (name, _) in pattern_bindings(pattern) {
                    context.code.push(Instruction::DeclareName {
                        name,
                        mutable: *kind == BindingKind::Let,
                    });
                }
                context
                    .code
                    .push(Instruction::InitializePattern(pattern.clone()));
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
                    scope_depth: loop_scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
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

                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
            }
            Stmt::Break { span } => {
                let Some(loop_ctx) = context.loop_stack.last() else {
                    return Err(JsliteError::runtime_at(
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
                    return Err(JsliteError::runtime_at(
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

    fn compile_expr(
        &mut self,
        context: &mut CompileContext,
        expression: &Expr,
    ) -> JsliteResult<()> {
        match expression {
            Expr::Undefined { .. } => context.code.push(Instruction::PushUndefined),
            Expr::Null { .. } => context.code.push(Instruction::PushNull),
            Expr::Bool { value, .. } => context.code.push(Instruction::PushBool(*value)),
            Expr::Number { value, .. } => context.code.push(Instruction::PushNumber(*value)),
            Expr::String { value, .. } => context.code.push(Instruction::PushString(value.clone())),
            Expr::RegExp { pattern, flags, .. } => context.code.push(Instruction::PushRegExp {
                pattern: pattern.clone(),
                flags: flags.clone(),
            }),
            Expr::Identifier { name, .. } => context.code.push(Instruction::LoadName(name.clone())),
            Expr::This { .. } => context.code.push(Instruction::LoadName("this".to_string())),
            Expr::Array { elements, .. } => {
                for element in elements {
                    self.compile_expr(context, element)?;
                }
                context.code.push(Instruction::MakeArray {
                    count: elements.len(),
                });
            }
            Expr::Object { properties, .. } => {
                let mut keys = Vec::with_capacity(properties.len());
                for property in properties {
                    self.compile_expr(context, &property.value)?;
                    keys.push(property.key.clone());
                }
                context.code.push(Instruction::MakeObject { keys });
            }
            Expr::Function(function) => {
                context.code.push(Instruction::MakeClosure {
                    function_id: self.compile_function(function)?,
                });
            }
            Expr::Unary {
                operator, argument, ..
            } => {
                self.compile_expr(context, argument)?;
                context.code.push(Instruction::Unary(*operator));
            }
            Expr::Binary {
                operator,
                left,
                right,
                ..
            } => {
                self.compile_expr(context, left)?;
                self.compile_expr(context, right)?;
                context.code.push(Instruction::Binary(*operator));
            }
            Expr::Logical {
                operator,
                left,
                right,
                ..
            } => {
                self.compile_expr(context, left)?;
                match operator {
                    crate::ir::LogicalOp::And => {
                        let jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                        context.code.push(Instruction::Pop);
                        self.compile_expr(context, right)?;
                        let end_ip = context.code.len();
                        self.patch_jump(context, jump, end_ip);
                    }
                    crate::ir::LogicalOp::Or => {
                        let jump = self.emit_jump(context, Instruction::JumpIfTrue(usize::MAX));
                        context.code.push(Instruction::Pop);
                        self.compile_expr(context, right)?;
                        let end_ip = context.code.len();
                        self.patch_jump(context, jump, end_ip);
                    }
                    crate::ir::LogicalOp::NullishCoalesce => {
                        let jump = self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        let end = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs = context.code.len();
                        self.patch_jump(context, jump, rhs);
                        context.code.push(Instruction::Pop);
                        self.compile_expr(context, right)?;
                        let end_ip = context.code.len();
                        self.patch_jump(context, end, end_ip);
                    }
                }
            }
            Expr::Conditional {
                test,
                consequent,
                alternate,
                ..
            } => {
                self.compile_expr(context, test)?;
                let else_jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                self.compile_expr(context, consequent)?;
                let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let else_ip = context.code.len();
                self.patch_jump(context, else_jump, else_ip);
                context.code.push(Instruction::Pop);
                self.compile_expr(context, alternate)?;
                let end_ip = context.code.len();
                self.patch_jump(context, end_jump, end_ip);
            }
            Expr::Assignment {
                target,
                operator,
                value,
                ..
            } => self.compile_assignment(context, target, *operator, value)?,
            Expr::Member {
                object,
                property,
                optional,
                ..
            } => {
                self.compile_expr(context, object)?;
                match property {
                    MemberProperty::Static(PropertyName::Identifier(name))
                    | MemberProperty::Static(PropertyName::String(name)) => {
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        })
                    }
                    MemberProperty::Static(PropertyName::Number(number)) => {
                        context.code.push(Instruction::GetPropStatic {
                            name: format_number_key(*number),
                            optional: *optional,
                        })
                    }
                    MemberProperty::Computed(expression) => {
                        self.compile_expr(context, expression)?;
                        context.code.push(Instruction::GetPropComputed {
                            optional: *optional,
                        });
                    }
                }
            }
            Expr::Call {
                callee,
                arguments,
                optional,
                ..
            } => {
                let with_this = matches!(callee.as_ref(), Expr::Member { .. });
                if let Expr::Member {
                    object,
                    property,
                    optional: member_optional,
                    ..
                } = callee.as_ref()
                {
                    self.compile_expr(context, object)?;
                    context.code.push(Instruction::Dup);
                    match property {
                        MemberProperty::Static(PropertyName::Identifier(name))
                        | MemberProperty::Static(PropertyName::String(name)) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: name.clone(),
                                optional: *member_optional,
                            })
                        }
                        MemberProperty::Static(PropertyName::Number(number)) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: format_number_key(*number),
                                optional: *member_optional,
                            })
                        }
                        MemberProperty::Computed(expression) => {
                            self.compile_expr(context, expression)?;
                            context.code.push(Instruction::GetPropComputed {
                                optional: *member_optional,
                            });
                        }
                    }
                } else {
                    self.compile_expr(context, callee)?;
                }
                for argument in arguments {
                    self.compile_expr(context, argument)?;
                }
                context.code.push(Instruction::Call {
                    argc: arguments.len(),
                    with_this,
                    optional: *optional,
                });
            }
            Expr::New {
                callee, arguments, ..
            } => {
                self.compile_expr(context, callee)?;
                for argument in arguments {
                    self.compile_expr(context, argument)?;
                }
                context.code.push(Instruction::Construct {
                    argc: arguments.len(),
                });
            }
            Expr::Template {
                quasis,
                expressions,
                ..
            } => {
                let mut parts = Vec::new();
                for (index, quasi) in quasis.iter().enumerate() {
                    if !quasi.is_empty() {
                        parts.push(Expr::String {
                            span: SourceSpan::new(0, 0),
                            value: quasi.clone(),
                        });
                    }
                    if let Some(expression) = expressions.get(index) {
                        parts.push(expression.clone());
                    }
                }
                if parts.is_empty() {
                    context.code.push(Instruction::PushString(String::new()));
                } else {
                    self.compile_expr(context, &parts[0])?;
                    for part in parts.iter().skip(1) {
                        self.compile_expr(context, part)?;
                        context.code.push(Instruction::Binary(BinaryOp::Add));
                    }
                }
            }
            Expr::Await { value, .. } => {
                self.compile_expr(context, value)?;
                context.code.push(Instruction::Await);
            }
        }
        Ok(())
    }

    fn compile_assignment(
        &mut self,
        context: &mut CompileContext,
        target: &AssignTarget,
        operator: AssignOp,
        value: &Expr,
    ) -> JsliteResult<()> {
        match target {
            AssignTarget::Identifier { name, .. } => {
                if operator == AssignOp::Assign {
                    self.compile_expr(context, value)?;
                    context.code.push(Instruction::StoreName(name.clone()));
                } else if operator == AssignOp::NullishAssign {
                    context.code.push(Instruction::LoadName(name.clone()));
                    context.code.push(Instruction::Dup);
                    let rhs_jump = self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                    context.code.push(Instruction::Pop);
                    let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                    let rhs_ip = context.code.len();
                    self.patch_jump(context, rhs_jump, rhs_ip);
                    context.code.push(Instruction::Pop);
                    context.code.push(Instruction::Pop);
                    self.compile_expr(context, value)?;
                    context.code.push(Instruction::StoreName(name.clone()));
                    let end_ip = context.code.len();
                    self.patch_jump(context, end_jump, end_ip);
                } else {
                    context.code.push(Instruction::LoadName(name.clone()));
                    self.compile_expr(context, value)?;
                    context
                        .code
                        .push(Instruction::Binary(assign_op_to_binary(operator)?));
                    context.code.push(Instruction::StoreName(name.clone()));
                }
            }
            AssignTarget::Member {
                object,
                property,
                optional,
                ..
            } => match property {
                MemberProperty::Static(PropertyName::Identifier(name))
                | MemberProperty::Static(PropertyName::String(name)) => {
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                    } else if operator == AssignOp::NullishAssign {
                        let object_binding = self.fresh_internal_name(context, "assign_obj");
                        context.code.push(Instruction::DeclareName {
                            name: object_binding.clone(),
                            mutable: false,
                        });
                        self.compile_expr(context, object)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: object_binding.clone(),
                            }));
                        context
                            .code
                            .push(Instruction::LoadName(object_binding.clone()));
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        context.code.push(Instruction::Dup);
                        let rhs_jump =
                            self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        context.code.push(Instruction::Pop);
                        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs_ip = context.code.len();
                        self.patch_jump(context, rhs_jump, rhs_ip);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::LoadName(object_binding));
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                        let end_ip = context.code.len();
                        self.patch_jump(context, end_jump, end_ip);
                    } else {
                        self.compile_expr(context, object)?;
                        context.code.push(Instruction::Dup);
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::Binary(assign_op_to_binary(operator)?));
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                    }
                }
                MemberProperty::Static(PropertyName::Number(number)) => {
                    let name = format_number_key(*number);
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropStatic { name });
                    } else if operator == AssignOp::NullishAssign {
                        let object_binding = self.fresh_internal_name(context, "assign_obj");
                        context.code.push(Instruction::DeclareName {
                            name: object_binding.clone(),
                            mutable: false,
                        });
                        self.compile_expr(context, object)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: object_binding.clone(),
                            }));
                        context
                            .code
                            .push(Instruction::LoadName(object_binding.clone()));
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        context.code.push(Instruction::Dup);
                        let rhs_jump =
                            self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        context.code.push(Instruction::Pop);
                        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs_ip = context.code.len();
                        self.patch_jump(context, rhs_jump, rhs_ip);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::LoadName(object_binding));
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropStatic { name });
                        let end_ip = context.code.len();
                        self.patch_jump(context, end_jump, end_ip);
                    } else {
                        self.compile_expr(context, object)?;
                        context.code.push(Instruction::Dup);
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::Binary(assign_op_to_binary(operator)?));
                        context.code.push(Instruction::SetPropStatic { name });
                    }
                }
                MemberProperty::Computed(expr) => {
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, expr)?;
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropComputed);
                    } else if operator == AssignOp::NullishAssign {
                        let object_binding = self.fresh_internal_name(context, "assign_obj");
                        let key_binding = self.fresh_internal_name(context, "assign_key");
                        for name in [&object_binding, &key_binding] {
                            context.code.push(Instruction::DeclareName {
                                name: name.clone(),
                                mutable: false,
                            });
                        }
                        self.compile_expr(context, object)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: object_binding.clone(),
                            }));
                        self.compile_expr(context, expr)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: key_binding.clone(),
                            }));
                        context
                            .code
                            .push(Instruction::LoadName(object_binding.clone()));
                        context
                            .code
                            .push(Instruction::LoadName(key_binding.clone()));
                        context.code.push(Instruction::GetPropComputed {
                            optional: *optional,
                        });
                        context.code.push(Instruction::Dup);
                        let rhs_jump =
                            self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        context.code.push(Instruction::Pop);
                        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs_ip = context.code.len();
                        self.patch_jump(context, rhs_jump, rhs_ip);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::LoadName(object_binding));
                        context.code.push(Instruction::LoadName(key_binding));
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropComputed);
                        let end_ip = context.code.len();
                        self.patch_jump(context, end_jump, end_ip);
                    } else {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, expr)?;
                        context.code.push(Instruction::Dup2);
                        context.code.push(Instruction::GetPropComputed {
                            optional: *optional,
                        });
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::Binary(assign_op_to_binary(operator)?));
                        context.code.push(Instruction::SetPropComputed);
                    }
                }
            },
        }
        Ok(())
    }

    fn compile_try(
        &mut self,
        context: &mut CompileContext,
        body: &Stmt,
        catch: Option<&crate::ir::CatchClause>,
        finally: Option<&Stmt>,
    ) -> JsliteResult<()> {
        let finally_region = finally.map(|_| {
            context
                .finally_regions
                .push(FinallyRegionContext::default());
            context.finally_regions.len() - 1
        });

        let try_handler_site = context.code.len();
        context.code.push(Instruction::PushHandler {
            catch: catch.map(|_| usize::MAX),
            finally: finally_region.map(|_| usize::MAX),
        });
        if let Some(region) = finally_region {
            context.finally_regions[region]
                .handler_sites
                .push(try_handler_site);
        }

        context.active_handlers.push(ActiveHandlerContext {
            finally_region,
            scope_depth: context.scope_depth,
        });
        self.compile_stmt(context, body)?;
        context.active_handlers.pop();
        context.code.push(Instruction::PopHandler);

        let mut skip_catch_jump = None;
        let mut after_finally_patches = Vec::new();
        let outer_handler_depth = context.active_handlers.len();

        if let Some(region) = finally_region {
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth: outer_handler_depth,
                target_scope_depth: context.scope_depth,
            });
            after_finally_patches.push(patch);
            self.emit_jump_to_finally(context, region);
        } else if catch.is_some() {
            skip_catch_jump = Some(self.emit_jump(context, Instruction::Jump(usize::MAX)));
        }

        if let Some(catch_clause) = catch {
            self.patch_handler_catch(context, try_handler_site, context.code.len());

            if let Some(region) = finally_region {
                let catch_handler_site = context.code.len();
                context.code.push(Instruction::PushHandler {
                    catch: None,
                    finally: Some(usize::MAX),
                });
                context.finally_regions[region]
                    .handler_sites
                    .push(catch_handler_site);
                context.active_handlers.push(ActiveHandlerContext {
                    finally_region: Some(region),
                    scope_depth: context.scope_depth,
                });
            }

            context.code.push(Instruction::PushEnv);
            context.scope_depth += 1;
            if let Some(parameter) = &catch_clause.parameter {
                for (name, mutable) in pattern_bindings(parameter) {
                    context
                        .code
                        .push(Instruction::DeclareName { name, mutable });
                }
            }
            context.code.push(Instruction::BeginCatch);
            if let Some(parameter) = &catch_clause.parameter {
                context
                    .code
                    .push(Instruction::InitializePattern(parameter.clone()));
            } else {
                context.code.push(Instruction::Pop);
            }
            self.compile_stmt(context, catch_clause.body.as_ref())?;
            context.scope_depth -= 1;
            context.code.push(Instruction::PopEnv);

            if let Some(region) = finally_region {
                context.active_handlers.pop();
                context.code.push(Instruction::PopHandler);
                let patch = context.code.len();
                context.code.push(Instruction::PushPendingJump {
                    target: usize::MAX,
                    target_handler_depth: outer_handler_depth,
                    target_scope_depth: context.scope_depth,
                });
                after_finally_patches.push(patch);
                self.emit_jump_to_finally(context, region);
            }
        }

        if let Some(finally_stmt) = finally {
            let finally_ip = context.code.len();
            self.patch_finally_region(
                context,
                finally_region.expect("finally region should exist"),
                finally_ip,
            );
            let enter_finally = context.code.len();
            context
                .code
                .push(Instruction::EnterFinally { exit: usize::MAX });
            context.active_finally.push(ActiveFinallyContext {
                exit_patch_site: enter_finally,
                jump_sites: Vec::new(),
                scope_depth: context.scope_depth,
            });
            self.compile_stmt(context, finally_stmt)?;
            let continue_ip = context.code.len();
            let active_finally = context
                .active_finally
                .pop()
                .expect("finally context should exist");
            self.patch_finally_exit(context, active_finally, continue_ip);
            context.code.push(Instruction::ContinuePending);
            let after_finally = context.code.len();
            for patch in after_finally_patches {
                self.patch_pending_jump(context, patch, after_finally);
            }
            if let Some(skip_catch_jump) = skip_catch_jump {
                self.patch_jump(context, skip_catch_jump, after_finally);
            }
        } else if let Some(skip_catch_jump) = skip_catch_jump {
            let after_catch = context.code.len();
            self.patch_jump(context, skip_catch_jump, after_catch);
        }

        Ok(())
    }

    fn emit_return(&self, context: &mut CompileContext) {
        if let Some(active_finally) = context.active_finally.last() {
            self.emit_scope_cleanup(context, active_finally.scope_depth);
            context.code.push(Instruction::PushPendingReturn);
            self.emit_jump_to_active_finally_exit(context);
            return;
        }
        if let Some((handler_depth, region)) = self.nearest_finally_region(context, 0) {
            self.emit_scope_cleanup(context, context.active_handlers[handler_depth].scope_depth);
            self.emit_handler_cleanup(context, handler_depth);
            context.code.push(Instruction::PushPendingReturn);
            self.emit_jump_to_finally(context, region);
        } else {
            context.code.push(Instruction::Return);
        }
    }

    fn emit_jump_transfer(
        &self,
        context: &mut CompileContext,
        target_handler_depth: usize,
        target_scope_depth: usize,
    ) -> ControlTransferPatch {
        if let Some(active_finally) = context.active_finally.last() {
            self.emit_scope_cleanup(context, active_finally.scope_depth);
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth,
                target_scope_depth,
            });
            self.emit_jump_to_active_finally_exit(context);
            return ControlTransferPatch::PendingJump(patch);
        }
        if let Some((handler_depth, region)) =
            self.nearest_finally_region(context, target_handler_depth)
        {
            self.emit_scope_cleanup(context, context.active_handlers[handler_depth].scope_depth);
            self.emit_handler_cleanup(context, handler_depth);
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth,
                target_scope_depth,
            });
            self.emit_jump_to_finally(context, region);
            ControlTransferPatch::PendingJump(patch)
        } else {
            self.emit_scope_cleanup(context, target_scope_depth);
            self.emit_handler_cleanup(context, target_handler_depth);
            ControlTransferPatch::DirectJump(self.emit_jump(context, Instruction::Jump(usize::MAX)))
        }
    }

    fn emit_scope_cleanup(&self, context: &mut CompileContext, target_scope_depth: usize) {
        for _ in target_scope_depth..context.scope_depth {
            context.code.push(Instruction::PopEnv);
        }
    }

    fn emit_handler_cleanup(&self, context: &mut CompileContext, target_handler_depth: usize) {
        for _ in target_handler_depth..context.active_handlers.len() {
            context.code.push(Instruction::PopHandler);
        }
    }

    fn nearest_finally_region(
        &self,
        context: &CompileContext,
        target_handler_depth: usize,
    ) -> Option<(usize, usize)> {
        for handler_depth in (target_handler_depth..context.active_handlers.len()).rev() {
            if let Some(region) = context.active_handlers[handler_depth].finally_region {
                return Some((handler_depth, region));
            }
        }
        None
    }

    fn emit_jump_to_finally(&self, context: &mut CompileContext, region: usize) {
        let jump_site = self.emit_jump(context, Instruction::Jump(usize::MAX));
        context.finally_regions[region].jump_sites.push(jump_site);
    }

    fn emit_jump_to_active_finally_exit(&self, context: &mut CompileContext) {
        let jump_site = self.emit_jump(context, Instruction::Jump(usize::MAX));
        context
            .active_finally
            .last_mut()
            .expect("finally context should exist")
            .jump_sites
            .push(jump_site);
    }

    fn patch_handler_catch(&self, context: &mut CompileContext, index: usize, target: usize) {
        if let Instruction::PushHandler { catch, .. } = &mut context.code[index] {
            *catch = Some(target);
        }
    }

    fn patch_finally_region(&self, context: &mut CompileContext, region: usize, target: usize) {
        let handler_sites = context.finally_regions[region].handler_sites.clone();
        let jump_sites = context.finally_regions[region].jump_sites.clone();
        for site in handler_sites {
            if let Instruction::PushHandler { finally, .. } = &mut context.code[site] {
                *finally = Some(target);
            }
        }
        for site in jump_sites {
            self.patch_jump(context, site, target);
        }
    }

    fn patch_finally_exit(
        &self,
        context: &mut CompileContext,
        finally: ActiveFinallyContext,
        target: usize,
    ) {
        if let Instruction::EnterFinally { exit } = &mut context.code[finally.exit_patch_site] {
            *exit = target;
        }
        for jump_site in finally.jump_sites {
            self.patch_jump(context, jump_site, target);
        }
    }

    fn patch_pending_jump(&self, context: &mut CompileContext, index: usize, target: usize) {
        if let Instruction::PushPendingJump { target: jump, .. } = &mut context.code[index] {
            *jump = target;
        }
    }

    fn patch_control_transfer(
        &self,
        context: &mut CompileContext,
        patch: ControlTransferPatch,
        target: usize,
    ) {
        match patch {
            ControlTransferPatch::DirectJump(index) => self.patch_jump(context, index, target),
            ControlTransferPatch::PendingJump(index) => {
                self.patch_pending_jump(context, index, target)
            }
        }
    }

    fn emit_jump(&self, context: &mut CompileContext, instruction: Instruction) -> usize {
        let index = context.code.len();
        context.code.push(instruction);
        index
    }

    fn patch_jump(&self, context: &mut CompileContext, index: usize, target: usize) {
        match &mut context.code[index] {
            Instruction::Jump(address)
            | Instruction::JumpIfFalse(address)
            | Instruction::JumpIfTrue(address)
            | Instruction::JumpIfNullish(address) => *address = target,
            _ => {}
        }
    }
}
