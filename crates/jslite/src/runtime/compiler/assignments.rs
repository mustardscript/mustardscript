use super::super::{bytecode::Instruction, format_number_key};
use super::{Compiler, context::CompileContext};
use crate::{
    diagnostic::JsliteResult,
    ir::{AssignOp, AssignTarget, Expr, MemberProperty, Pattern, PropertyName},
    span::SourceSpan,
};

use super::bindings::assign_op_to_binary;

impl Compiler {
    pub(super) fn compile_assignment(
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
}
