use super::super::{bytecode::Instruction, format_number_key};
use super::{Compiler, context::CompileContext};
use crate::{
    diagnostic::MustardResult,
    ir::{
        ArrayElement, BinaryOp, CallArgument, Expr, MemberProperty, ObjectProperty,
        ObjectPropertyKey, PropertyName,
    },
    span::SourceSpan,
};

impl Compiler {
    pub(super) fn compile_expr(
        &mut self,
        context: &mut CompileContext,
        expression: &Expr,
    ) -> MustardResult<()> {
        match expression {
            Expr::Undefined { .. } => context.code.push(Instruction::PushUndefined),
            Expr::Null { .. } => context.code.push(Instruction::PushNull),
            Expr::Bool { value, .. } => context.code.push(Instruction::PushBool(*value)),
            Expr::Number { value, .. } => context.code.push(Instruction::PushNumber(*value)),
            Expr::BigInt { value, .. } => context.code.push(Instruction::PushBigInt(value.clone())),
            Expr::String { value, .. } => context.code.push(Instruction::PushString(value.clone())),
            Expr::RegExp { pattern, flags, .. } => context.code.push(Instruction::PushRegExp {
                pattern: pattern.clone(),
                flags: flags.clone(),
            }),
            Expr::Identifier { name, .. } => self.emit_load_name(context, name),
            Expr::This { .. } => self.emit_load_name(context, "this"),
            Expr::Array { elements, .. } => {
                if elements
                    .iter()
                    .all(|element| matches!(element, ArrayElement::Value(_)))
                {
                    for element in elements {
                        let ArrayElement::Value(element) = element else {
                            unreachable!("dense array fast-path should only see value entries");
                        };
                        self.compile_expr(context, element)?;
                    }
                    context.code.push(Instruction::MakeArray {
                        count: elements.len(),
                    });
                } else {
                    context.code.push(Instruction::MakeArray { count: 0 });
                    for element in elements {
                        match element {
                            ArrayElement::Value(element) => {
                                self.compile_expr(context, element)?;
                                context.code.push(Instruction::ArrayPush);
                            }
                            ArrayElement::Hole { .. } => {
                                context.code.push(Instruction::ArrayPushHole);
                            }
                            ArrayElement::Spread { value, .. } => {
                                self.compile_expr(context, value)?;
                                context.code.push(Instruction::ArrayExtend);
                            }
                        }
                    }
                }
            }
            Expr::Object { properties, .. } => {
                let static_keys = properties
                    .iter()
                    .map(|property| match property {
                        ObjectProperty::Property {
                            key: ObjectPropertyKey::Static(name),
                            ..
                        } => Some(name.clone()),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();

                if let Some(keys) = static_keys {
                    for property in properties {
                        let ObjectProperty::Property { value, .. } = property else {
                            unreachable!("static-key object fast-path should only see properties");
                        };
                        self.compile_expr(context, value)?;
                    }
                    context.code.push(Instruction::MakeObject { keys });
                } else {
                    context
                        .code
                        .push(Instruction::MakeObject { keys: Vec::new() });
                    for property in properties {
                        match property {
                            ObjectProperty::Property { key, value, .. } => {
                                context.code.push(Instruction::Dup);
                                match key {
                                    ObjectPropertyKey::Static(PropertyName::Identifier(name))
                                    | ObjectPropertyKey::Static(PropertyName::String(name)) => {
                                        self.compile_expr(context, value)?;
                                        context.code.push(Instruction::SetPropStatic {
                                            name: name.clone(),
                                        });
                                    }
                                    ObjectPropertyKey::Static(PropertyName::Number(number)) => {
                                        self.compile_expr(context, value)?;
                                        context.code.push(Instruction::SetPropStatic {
                                            name: format_number_key(*number),
                                        });
                                    }
                                    ObjectPropertyKey::Computed(expression) => {
                                        self.compile_expr(context, expression)?;
                                        self.compile_expr(context, value)?;
                                        context.code.push(Instruction::SetPropComputed);
                                    }
                                }
                                context.code.push(Instruction::Pop);
                            }
                            ObjectProperty::Spread { value, .. } => {
                                self.compile_expr(context, value)?;
                                context.code.push(Instruction::CopyDataProperties);
                            }
                        }
                    }
                }
            }
            Expr::Function(function) => {
                context.code.push(Instruction::MakeClosure {
                    function_id: self.compile_function(context, function)?,
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
            Expr::Sequence { expressions, .. } => {
                if expressions.is_empty() {
                    context.code.push(Instruction::PushUndefined);
                } else {
                    for (index, expression) in expressions.iter().enumerate() {
                        self.compile_expr(context, expression)?;
                        if index + 1 != expressions.len() {
                            context.code.push(Instruction::Pop);
                        }
                    }
                }
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
            } => self.compile_assignment(context, target.as_ref(), *operator, value)?,
            Expr::Update {
                target,
                operator,
                prefix,
                ..
            } => self.compile_update(context, target.as_ref(), *operator, *prefix)?,
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
                span,
                callee,
                arguments,
                optional,
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
                if arguments
                    .iter()
                    .all(|argument| matches!(argument, CallArgument::Value(_)))
                {
                    for argument in arguments {
                        let CallArgument::Value(argument) = argument else {
                            unreachable!(
                                "non-spread call fast-path should only see value arguments"
                            );
                        };
                        self.compile_expr(context, argument)?;
                    }
                    context.code.push(Instruction::Call {
                        argc: arguments.len(),
                        with_this,
                        optional: *optional,
                        span: *span,
                    });
                } else {
                    context.code.push(Instruction::MakeArray { count: 0 });
                    for argument in arguments {
                        match argument {
                            CallArgument::Value(argument) => {
                                self.compile_expr(context, argument)?;
                                context.code.push(Instruction::ArrayPush);
                            }
                            CallArgument::Spread { value, .. } => {
                                self.compile_expr(context, value)?;
                                context.code.push(Instruction::ArrayExtend);
                            }
                        }
                    }
                    context.code.push(Instruction::CallWithArray {
                        with_this,
                        optional: *optional,
                        span: *span,
                    });
                }
            }
            Expr::New {
                callee, arguments, ..
            } => {
                self.compile_expr(context, callee)?;
                if arguments
                    .iter()
                    .all(|argument| matches!(argument, CallArgument::Value(_)))
                {
                    for argument in arguments {
                        let CallArgument::Value(argument) = argument else {
                            unreachable!(
                                "non-spread constructor fast-path should only see value arguments"
                            );
                        };
                        self.compile_expr(context, argument)?;
                    }
                    context.code.push(Instruction::Construct {
                        argc: arguments.len(),
                    });
                } else {
                    context.code.push(Instruction::MakeArray { count: 0 });
                    for argument in arguments {
                        match argument {
                            CallArgument::Value(argument) => {
                                self.compile_expr(context, argument)?;
                                context.code.push(Instruction::ArrayPush);
                            }
                            CallArgument::Spread { value, .. } => {
                                self.compile_expr(context, value)?;
                                context.code.push(Instruction::ArrayExtend);
                            }
                        }
                    }
                    context.code.push(Instruction::ConstructWithArray);
                }
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
}
