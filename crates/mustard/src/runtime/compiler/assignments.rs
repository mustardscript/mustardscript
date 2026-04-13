use super::super::{bytecode::Instruction, format_number_key};
use super::{Compiler, context::CompileContext};
use crate::{
    diagnostic::MustardResult,
    ir::{AssignOp, AssignTarget, Expr, MemberProperty, Pattern, PropertyName, UpdateOp},
    span::SourceSpan,
};

use super::bindings::assign_op_to_binary;

impl Compiler {
    fn declare_internal_binding(&mut self, context: &mut CompileContext, prefix: &str) -> String {
        let name = self.fresh_internal_name(context, prefix);
        self.emit_declare_name(context, name.clone(), true);
        name
    }

    fn store_internal_binding(
        &mut self,
        context: &mut CompileContext,
        name: &str,
        span: SourceSpan,
    ) {
        context
            .code
            .push(Instruction::InitializePattern(Pattern::Identifier {
                span,
                name: name.to_string(),
            }));
    }

    pub(super) fn compile_pattern_binding(
        &mut self,
        context: &mut CompileContext,
        pattern: &Pattern,
    ) -> MustardResult<()> {
        match pattern {
            Pattern::Identifier { .. } => {
                context
                    .code
                    .push(Instruction::InitializePattern(pattern.clone()));
            }
            Pattern::Array {
                span,
                elements,
                rest,
            } => {
                let source = self.declare_internal_binding(context, "pattern_array");
                self.store_internal_binding(context, &source, *span);
                for (index, element) in elements.iter().enumerate() {
                    if let Some(element) = element {
                        self.emit_load_name(context, &source);
                        context.code.push(Instruction::PatternArrayIndex(index));
                        self.compile_pattern_binding(context, element)?;
                    }
                }
                if let Some(rest) = rest {
                    self.emit_load_name(context, &source);
                    context
                        .code
                        .push(Instruction::PatternArrayRest(elements.len()));
                    self.compile_pattern_binding(context, rest)?;
                }
            }
            Pattern::Object {
                span,
                properties,
                rest,
            } => {
                let source = self.declare_internal_binding(context, "pattern_object");
                self.store_internal_binding(context, &source, *span);
                for property in properties {
                    self.emit_load_name(context, &source);
                    match &property.key {
                        PropertyName::Identifier(name) | PropertyName::String(name) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: name.clone(),
                                optional: false,
                            });
                        }
                        PropertyName::Number(number) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: format_number_key(*number),
                                optional: false,
                            });
                        }
                    }
                    self.compile_pattern_binding(context, &property.value)?;
                }
                if let Some(rest) = rest {
                    let excluded = properties
                        .iter()
                        .map(|property| match &property.key {
                            PropertyName::Identifier(name) | PropertyName::String(name) => {
                                name.clone()
                            }
                            PropertyName::Number(number) => format_number_key(*number),
                        })
                        .collect();
                    self.emit_load_name(context, &source);
                    context.code.push(Instruction::PatternObjectRest(excluded));
                    self.compile_pattern_binding(context, rest)?;
                }
            }
            Pattern::Default {
                span,
                target,
                default_value,
            } => {
                let source = self.declare_internal_binding(context, "pattern_default");
                self.store_internal_binding(context, &source, *span);
                self.emit_load_name(context, &source);
                context.code.push(Instruction::PushUndefined);
                context
                    .code
                    .push(Instruction::Binary(crate::ir::BinaryOp::StrictEq));
                let use_source = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                self.compile_expr(context, default_value)?;
                let end = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let use_source_ip = context.code.len();
                self.patch_jump(context, use_source, use_source_ip);
                context.code.push(Instruction::Pop);
                self.emit_load_name(context, &source);
                let end_ip = context.code.len();
                self.patch_jump(context, end, end_ip);
                self.compile_pattern_binding(context, target)?;
            }
        }
        Ok(())
    }

    fn compile_assign_target_pattern(
        &mut self,
        context: &mut CompileContext,
        target: &AssignTarget,
    ) -> MustardResult<()> {
        match target {
            AssignTarget::Identifier { .. } | AssignTarget::Member { .. } => {
                let source = self.declare_internal_binding(context, "assign_value");
                self.store_internal_binding(context, &source, SourceSpan::new(0, 0));
                self.compile_assignment(
                    context,
                    target,
                    AssignOp::Assign,
                    &Expr::Identifier {
                        span: SourceSpan::new(0, 0),
                        name: source,
                    },
                )?;
                context.code.push(Instruction::Pop);
            }
            AssignTarget::Default {
                span,
                target,
                default_value,
            } => {
                let source = self.declare_internal_binding(context, "assign_default");
                self.store_internal_binding(context, &source, *span);
                self.emit_load_name(context, &source);
                context.code.push(Instruction::PushUndefined);
                context
                    .code
                    .push(Instruction::Binary(crate::ir::BinaryOp::StrictEq));
                let use_source = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                self.compile_expr(context, default_value)?;
                let end = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let use_source_ip = context.code.len();
                self.patch_jump(context, use_source, use_source_ip);
                context.code.push(Instruction::Pop);
                self.emit_load_name(context, &source);
                let end_ip = context.code.len();
                self.patch_jump(context, end, end_ip);
                self.compile_assign_target_pattern(context, target)?;
            }
            AssignTarget::Array {
                span,
                elements,
                rest,
            } => {
                let source = self.declare_internal_binding(context, "assign_array");
                self.store_internal_binding(context, &source, *span);
                for (index, element) in elements.iter().enumerate() {
                    if let Some(element) = element {
                        self.emit_load_name(context, &source);
                        context.code.push(Instruction::PatternArrayIndex(index));
                        self.compile_assign_target_pattern(context, element)?;
                    }
                }
                if let Some(rest) = rest {
                    self.emit_load_name(context, &source);
                    context
                        .code
                        .push(Instruction::PatternArrayRest(elements.len()));
                    self.compile_assign_target_pattern(context, rest)?;
                }
            }
            AssignTarget::Object {
                span,
                properties,
                rest,
            } => {
                let source = self.declare_internal_binding(context, "assign_object");
                self.store_internal_binding(context, &source, *span);
                for property in properties {
                    self.emit_load_name(context, &source);
                    match &property.key {
                        PropertyName::Identifier(name) | PropertyName::String(name) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: name.clone(),
                                optional: false,
                            });
                        }
                        PropertyName::Number(number) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: format_number_key(*number),
                                optional: false,
                            });
                        }
                    }
                    self.compile_assign_target_pattern(context, &property.value)?;
                }
                if let Some(rest) = rest {
                    let excluded = properties
                        .iter()
                        .map(|property| match &property.key {
                            PropertyName::Identifier(name) | PropertyName::String(name) => {
                                name.clone()
                            }
                            PropertyName::Number(number) => format_number_key(*number),
                        })
                        .collect();
                    self.emit_load_name(context, &source);
                    context.code.push(Instruction::PatternObjectRest(excluded));
                    self.compile_assign_target_pattern(context, rest)?;
                }
            }
        }
        Ok(())
    }

    fn compile_short_circuit_assignment_identifier(
        &mut self,
        context: &mut CompileContext,
        name: &str,
        value: &Expr,
        jump_if_eval: Instruction,
    ) -> MustardResult<()> {
        self.emit_load_name(context, name);
        context.code.push(Instruction::Dup);
        let rhs_jump = self.emit_jump(context, jump_if_eval);
        context.code.push(Instruction::Pop);
        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
        let rhs_ip = context.code.len();
        self.patch_jump(context, rhs_jump, rhs_ip);
        context.code.push(Instruction::Pop);
        context.code.push(Instruction::Pop);
        self.compile_expr(context, value)?;
        self.emit_store_name(context, name);
        let end_ip = context.code.len();
        self.patch_jump(context, end_jump, end_ip);
        Ok(())
    }

    fn compile_short_circuit_assignment_static(
        &mut self,
        context: &mut CompileContext,
        object: &Expr,
        name: String,
        optional: bool,
        value: &Expr,
        jump_if_eval: Instruction,
    ) -> MustardResult<()> {
        let object_binding = self.fresh_internal_name(context, "assign_obj");
        self.emit_declare_name(context, object_binding.clone(), false);
        self.compile_expr(context, object)?;
        context
            .code
            .push(Instruction::InitializePattern(Pattern::Identifier {
                span: SourceSpan::new(0, 0),
                name: object_binding.clone(),
            }));
        self.emit_load_name(context, &object_binding);
        context.code.push(Instruction::GetPropStatic {
            name: name.clone(),
            optional,
        });
        context.code.push(Instruction::Dup);
        let rhs_jump = self.emit_jump(context, jump_if_eval);
        context.code.push(Instruction::Pop);
        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
        let rhs_ip = context.code.len();
        self.patch_jump(context, rhs_jump, rhs_ip);
        context.code.push(Instruction::Pop);
        context.code.push(Instruction::Pop);
        self.emit_load_name(context, &object_binding);
        self.compile_expr(context, value)?;
        context.code.push(Instruction::SetPropStatic { name });
        let end_ip = context.code.len();
        self.patch_jump(context, end_jump, end_ip);
        Ok(())
    }

    fn compile_short_circuit_assignment_computed(
        &mut self,
        context: &mut CompileContext,
        object: &Expr,
        property: &Expr,
        optional: bool,
        value: &Expr,
        jump_if_eval: Instruction,
    ) -> MustardResult<()> {
        let object_binding = self.fresh_internal_name(context, "assign_obj");
        let key_binding = self.fresh_internal_name(context, "assign_key");
        for name in [&object_binding, &key_binding] {
            self.emit_declare_name(context, name.clone(), false);
        }
        self.compile_expr(context, object)?;
        context
            .code
            .push(Instruction::InitializePattern(Pattern::Identifier {
                span: SourceSpan::new(0, 0),
                name: object_binding.clone(),
            }));
        self.compile_expr(context, property)?;
        context
            .code
            .push(Instruction::InitializePattern(Pattern::Identifier {
                span: SourceSpan::new(0, 0),
                name: key_binding.clone(),
            }));
        self.emit_load_name(context, &object_binding);
        self.emit_load_name(context, &key_binding);
        context.code.push(Instruction::GetPropComputed { optional });
        context.code.push(Instruction::Dup);
        let rhs_jump = self.emit_jump(context, jump_if_eval);
        context.code.push(Instruction::Pop);
        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
        let rhs_ip = context.code.len();
        self.patch_jump(context, rhs_jump, rhs_ip);
        context.code.push(Instruction::Pop);
        context.code.push(Instruction::Pop);
        self.emit_load_name(context, &object_binding);
        self.emit_load_name(context, &key_binding);
        self.compile_expr(context, value)?;
        context.code.push(Instruction::SetPropComputed);
        let end_ip = context.code.len();
        self.patch_jump(context, end_jump, end_ip);
        Ok(())
    }

    pub(super) fn compile_assignment(
        &mut self,
        context: &mut CompileContext,
        target: &AssignTarget,
        operator: AssignOp,
        value: &Expr,
    ) -> MustardResult<()> {
        if matches!(
            target,
            AssignTarget::Array { .. } | AssignTarget::Object { .. } | AssignTarget::Default { .. }
        ) {
            self.compile_expr(context, value)?;
            let result = self.declare_internal_binding(context, "assign_result");
            self.store_internal_binding(context, &result, SourceSpan::new(0, 0));
            self.emit_load_name(context, &result);
            self.compile_assign_target_pattern(
                context,
                match target {
                    AssignTarget::Array { .. }
                    | AssignTarget::Object { .. }
                    | AssignTarget::Default { .. } => target,
                    _ => unreachable!(),
                },
            )?;
            self.emit_load_name(context, &result);
            return Ok(());
        }
        match target {
            AssignTarget::Identifier { name, .. } => {
                if operator == AssignOp::Assign {
                    self.compile_expr(context, value)?;
                    self.emit_store_name(context, name);
                } else if operator == AssignOp::OrAssign {
                    self.compile_short_circuit_assignment_identifier(
                        context,
                        name,
                        value,
                        Instruction::JumpIfFalse(usize::MAX),
                    )?;
                } else if operator == AssignOp::AndAssign {
                    self.compile_short_circuit_assignment_identifier(
                        context,
                        name,
                        value,
                        Instruction::JumpIfTrue(usize::MAX),
                    )?;
                } else if operator == AssignOp::NullishAssign {
                    self.compile_short_circuit_assignment_identifier(
                        context,
                        name,
                        value,
                        Instruction::JumpIfNullish(usize::MAX),
                    )?;
                } else {
                    self.emit_load_name(context, name);
                    self.compile_expr(context, value)?;
                    context
                        .code
                        .push(Instruction::Binary(assign_op_to_binary(operator)?));
                    self.emit_store_name(context, name);
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
                    } else if operator == AssignOp::OrAssign {
                        self.compile_short_circuit_assignment_static(
                            context,
                            object,
                            name.clone(),
                            *optional,
                            value,
                            Instruction::JumpIfFalse(usize::MAX),
                        )?;
                    } else if operator == AssignOp::AndAssign {
                        self.compile_short_circuit_assignment_static(
                            context,
                            object,
                            name.clone(),
                            *optional,
                            value,
                            Instruction::JumpIfTrue(usize::MAX),
                        )?;
                    } else if operator == AssignOp::NullishAssign {
                        self.compile_short_circuit_assignment_static(
                            context,
                            object,
                            name.clone(),
                            *optional,
                            value,
                            Instruction::JumpIfNullish(usize::MAX),
                        )?;
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
                    } else if operator == AssignOp::OrAssign {
                        self.compile_short_circuit_assignment_static(
                            context,
                            object,
                            name,
                            *optional,
                            value,
                            Instruction::JumpIfFalse(usize::MAX),
                        )?;
                    } else if operator == AssignOp::AndAssign {
                        self.compile_short_circuit_assignment_static(
                            context,
                            object,
                            name,
                            *optional,
                            value,
                            Instruction::JumpIfTrue(usize::MAX),
                        )?;
                    } else if operator == AssignOp::NullishAssign {
                        self.compile_short_circuit_assignment_static(
                            context,
                            object,
                            name,
                            *optional,
                            value,
                            Instruction::JumpIfNullish(usize::MAX),
                        )?;
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
                    } else if operator == AssignOp::OrAssign {
                        self.compile_short_circuit_assignment_computed(
                            context,
                            object,
                            expr,
                            *optional,
                            value,
                            Instruction::JumpIfFalse(usize::MAX),
                        )?;
                    } else if operator == AssignOp::AndAssign {
                        self.compile_short_circuit_assignment_computed(
                            context,
                            object,
                            expr,
                            *optional,
                            value,
                            Instruction::JumpIfTrue(usize::MAX),
                        )?;
                    } else if operator == AssignOp::NullishAssign {
                        self.compile_short_circuit_assignment_computed(
                            context,
                            object,
                            expr,
                            *optional,
                            value,
                            Instruction::JumpIfNullish(usize::MAX),
                        )?;
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
            AssignTarget::Array { .. }
            | AssignTarget::Object { .. }
            | AssignTarget::Default { .. } => unreachable!(),
        }
        Ok(())
    }

    pub(super) fn compile_update(
        &mut self,
        context: &mut CompileContext,
        target: &AssignTarget,
        operator: UpdateOp,
        prefix: bool,
    ) -> MustardResult<()> {
        match target {
            AssignTarget::Identifier { name, .. } => {
                self.emit_load_name(context, name);
                if !prefix {
                    context.code.push(Instruction::Dup);
                }
                context.code.push(Instruction::Update(operator));
                self.emit_store_name(context, name);
                if !prefix {
                    context.code.push(Instruction::Pop);
                }
            }
            AssignTarget::Member {
                object,
                property,
                optional,
                ..
            } => {
                let object_binding = self.declare_internal_binding(context, "update_obj");
                self.compile_expr(context, object)?;
                self.store_internal_binding(context, &object_binding, SourceSpan::new(0, 0));
                let key_binding = if let MemberProperty::Computed(property) = property {
                    let key_binding = self.declare_internal_binding(context, "update_key");
                    self.compile_expr(context, property)?;
                    self.store_internal_binding(context, &key_binding, SourceSpan::new(0, 0));
                    Some(key_binding)
                } else {
                    None
                };
                self.emit_load_name(context, &object_binding);
                match property {
                    MemberProperty::Static(PropertyName::Identifier(name))
                    | MemberProperty::Static(PropertyName::String(name)) => {
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                    }
                    MemberProperty::Static(PropertyName::Number(number)) => {
                        context.code.push(Instruction::GetPropStatic {
                            name: format_number_key(*number),
                            optional: *optional,
                        });
                    }
                    MemberProperty::Computed(_) => {
                        self.emit_load_name(
                            context,
                            key_binding
                                .clone()
                                .expect("computed update key missing")
                                .as_str(),
                        );
                        context.code.push(Instruction::GetPropComputed {
                            optional: *optional,
                        });
                    }
                }
                if !prefix {
                    context.code.push(Instruction::Dup);
                }
                context.code.push(Instruction::Update(operator));
                let value_binding = self.declare_internal_binding(context, "update_value");
                self.store_internal_binding(context, &value_binding, SourceSpan::new(0, 0));
                self.emit_load_name(context, &object_binding);
                match property {
                    MemberProperty::Static(PropertyName::Identifier(name))
                    | MemberProperty::Static(PropertyName::String(name)) => {
                        self.emit_load_name(context, &value_binding);
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                    }
                    MemberProperty::Static(PropertyName::Number(number)) => {
                        self.emit_load_name(context, &value_binding);
                        context.code.push(Instruction::SetPropStatic {
                            name: format_number_key(*number),
                        });
                    }
                    MemberProperty::Computed(_) => {
                        self.emit_load_name(
                            context,
                            key_binding.expect("computed update key missing").as_str(),
                        );
                        self.emit_load_name(context, &value_binding);
                        context.code.push(Instruction::SetPropComputed);
                    }
                }
                if !prefix {
                    context.code.push(Instruction::Pop);
                }
            }
            AssignTarget::Array { .. }
            | AssignTarget::Object { .. }
            | AssignTarget::Default { .. } => unreachable!(),
        }
        Ok(())
    }
}
