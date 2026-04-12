use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_function(
        &mut self,
        function: &Function<'a>,
        is_arrow: bool,
    ) -> Option<FunctionExpr> {
        if function.generator {
            self.unsupported(
                "generators are not supported in v1",
                Some(function.span.into()),
            );
            return None;
        }
        let Some(body) = function.body.as_ref() else {
            self.unsupported(
                "functions without bodies are not supported",
                Some(function.span.into()),
            );
            return None;
        };
        if !self.validate_function_params(&function.params) {
            return None;
        }
        let (params, rest) = self.lower_function_params(&function.params)?;
        self.push_scope();
        for pattern in &params {
            self.collect_ir_pattern_bindings(pattern);
        }
        if let Some(rest) = &rest {
            self.collect_ir_pattern_bindings(rest);
        }
        self.predeclare_block(&body.statements);
        let lowered = body
            .statements
            .iter()
            .filter_map(|statement| self.lower_stmt(statement))
            .collect();
        self.pop_scope();
        Some(FunctionExpr {
            span: function.span.into(),
            name: function.id.as_ref().map(|id| id.name.as_str().to_string()),
            params,
            rest,
            body: lowered,
            is_async: function.r#async,
            is_arrow,
        })
    }

    pub(super) fn lower_arrow_function(
        &mut self,
        function: &ArrowFunctionExpression<'a>,
    ) -> Option<FunctionExpr> {
        if !self.validate_function_params(&function.params) {
            return None;
        }
        let (params, rest) = self.lower_function_params(&function.params)?;
        self.push_scope();
        for pattern in &params {
            self.collect_ir_pattern_bindings(pattern);
        }
        if let Some(rest) = &rest {
            self.collect_ir_pattern_bindings(rest);
        }
        self.predeclare_block(&function.body.statements);
        let body = if function.expression {
            if function.body.statements.len() == 1 {
                match &function.body.statements[0] {
                    Statement::ExpressionStatement(statement) => vec![Stmt::Return {
                        span: statement.span.into(),
                        value: Some(self.lower_expr(&statement.expression)?),
                    }],
                    statement => vec![self.lower_stmt(statement)?],
                }
            } else {
                function
                    .body
                    .statements
                    .iter()
                    .filter_map(|statement| self.lower_stmt(statement))
                    .collect()
            }
        } else {
            function
                .body
                .statements
                .iter()
                .filter_map(|statement| self.lower_stmt(statement))
                .collect()
        };
        self.pop_scope();
        Some(FunctionExpr {
            span: function.span.into(),
            name: None,
            params,
            rest,
            body,
            is_async: function.r#async,
            is_arrow: true,
        })
    }

    pub(super) fn lower_for_init_expr(&mut self, init: &ForStatementInit<'a>) -> Option<Expr> {
        match init {
            ForStatementInit::VariableDeclaration(_) => None,
            expression => self.lower_expr(expression.to_expression()),
        }
    }

    pub(super) fn lower_expr(&mut self, expression: &Expression<'a>) -> Option<Expr> {
        match expression {
            Expression::BooleanLiteral(literal) => Some(Expr::Bool {
                span: literal.span.into(),
                value: literal.value,
            }),
            Expression::NullLiteral(literal) => Some(Expr::Null {
                span: literal.span.into(),
            }),
            Expression::NumericLiteral(literal) => Some(Expr::Number {
                span: literal.span.into(),
                value: literal.value,
            }),
            Expression::BigIntLiteral(literal) => Some(Expr::BigInt {
                span: literal.span.into(),
                value: literal.value.as_str().to_string(),
            }),
            Expression::StringLiteral(literal) => Some(Expr::String {
                span: literal.span.into(),
                value: literal.value.as_str().to_string(),
            }),
            Expression::TemplateLiteral(literal) => Some(Expr::Template {
                span: literal.span.into(),
                quasis: literal
                    .quasis
                    .iter()
                    .map(|quasi| {
                        quasi
                            .value
                            .cooked
                            .as_ref()
                            .unwrap_or(&quasi.value.raw)
                            .as_str()
                            .to_string()
                    })
                    .collect(),
                expressions: literal
                    .expressions
                    .iter()
                    .filter_map(|expr| self.lower_expr(expr))
                    .collect(),
            }),
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                if !self.is_bound(name) && FORBIDDEN_AMBIENT_GLOBALS.contains(&name) {
                    self.unsupported(
                        format!("free reference to forbidden ambient global `{name}`"),
                        Some(identifier.span.into()),
                    );
                    return None;
                }
                if name == "undefined" {
                    return Some(Expr::Undefined {
                        span: identifier.span.into(),
                    });
                }
                Some(Expr::Identifier {
                    span: identifier.span.into(),
                    name: name.to_string(),
                })
            }
            Expression::ThisExpression(this) => Some(Expr::This {
                span: this.span.into(),
            }),
            Expression::ArrayExpression(array) => {
                let mut elements = Vec::with_capacity(array.elements.len());
                for element in &array.elements {
                    match element {
                        ArrayExpressionElement::SpreadElement(spread) => {
                            self.unsupported(
                                "array spread is not supported in v1",
                                Some(spread.span.into()),
                            );
                            return None;
                        }
                        ArrayExpressionElement::Elision(elision) => {
                            self.unsupported(
                                "array holes are not supported in v1",
                                Some(elision.span.into()),
                            );
                            return None;
                        }
                        element => elements.push(self.lower_expr(element.to_expression())?),
                    }
                }
                Some(Expr::Array {
                    span: array.span.into(),
                    elements,
                })
            }
            Expression::ObjectExpression(object) => Some(Expr::Object {
                span: object.span.into(),
                properties: object
                    .properties
                    .iter()
                    .filter_map(|property| match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            if property.method {
                                self.unsupported(
                                    "object literal methods are not supported in v1",
                                    Some(property.span.into()),
                                );
                                return None;
                            }
                            Some(crate::ir::ObjectProperty {
                                span: property.span.into(),
                                key: self.lower_property_name(&property.key)?,
                                value: self.lower_expr(&property.value)?,
                            })
                        }
                        ObjectPropertyKind::SpreadProperty(property) => {
                            self.unsupported(
                                "object spread is not supported in v1",
                                Some(property.span.into()),
                            );
                            None
                        }
                    })
                    .collect(),
            }),
            Expression::ArrowFunctionExpression(function) => Some(Expr::Function(Box::new(
                self.lower_arrow_function(function)?,
            ))),
            Expression::FunctionExpression(function) => Some(Expr::Function(Box::new(
                self.lower_function(function, false)?,
            ))),
            Expression::UnaryExpression(expression) => Some(Expr::Unary {
                span: expression.span.into(),
                operator: self.lower_unary_op(expression.operator, expression.span)?,
                argument: Box::new(self.lower_expr(&expression.argument)?),
            }),
            Expression::BinaryExpression(expression) => Some(Expr::Binary {
                span: expression.span.into(),
                operator: self.lower_binary_op(expression.operator, expression.span)?,
                left: Box::new(self.lower_expr(&expression.left)?),
                right: Box::new(self.lower_expr(&expression.right)?),
            }),
            Expression::SequenceExpression(expression) => {
                let mut expressions = Vec::with_capacity(expression.expressions.len());
                for entry in &expression.expressions {
                    expressions.push(self.lower_expr(entry)?);
                }
                Some(Expr::Sequence {
                    span: expression.span.into(),
                    expressions,
                })
            }
            Expression::LogicalExpression(expression) => Some(Expr::Logical {
                span: expression.span.into(),
                operator: self.lower_logical_op(expression.operator, expression.span)?,
                left: Box::new(self.lower_expr(&expression.left)?),
                right: Box::new(self.lower_expr(&expression.right)?),
            }),
            Expression::ConditionalExpression(expression) => Some(Expr::Conditional {
                span: expression.span.into(),
                test: Box::new(self.lower_expr(&expression.test)?),
                consequent: Box::new(self.lower_expr(&expression.consequent)?),
                alternate: Box::new(self.lower_expr(&expression.alternate)?),
            }),
            Expression::AssignmentExpression(expression) => Some(Expr::Assignment {
                span: expression.span.into(),
                target: self.lower_assignment_target(&expression.left)?,
                operator: self.lower_assign_op(expression.operator, expression.span)?,
                value: Box::new(self.lower_expr(&expression.right)?),
            }),
            Expression::CallExpression(expression) => Some(Expr::Call {
                span: expression.span.into(),
                callee: Box::new(self.lower_expr(&expression.callee)?),
                arguments: self.lower_call_args(&expression.arguments)?,
                optional: expression.optional,
            }),
            Expression::ChainExpression(expression) => match &expression.expression {
                ChainElement::CallExpression(call) => Some(Expr::Call {
                    span: call.span.into(),
                    callee: Box::new(self.lower_expr(&call.callee)?),
                    arguments: self.lower_call_args(&call.arguments)?,
                    optional: true,
                }),
                ChainElement::ComputedMemberExpression(member) => Some(Expr::Member {
                    span: member.span.into(),
                    object: Box::new(self.lower_expr(&member.object)?),
                    property: MemberProperty::Computed(Box::new(
                        self.lower_expr(&member.expression)?,
                    )),
                    optional: true,
                }),
                ChainElement::StaticMemberExpression(member) => Some(Expr::Member {
                    span: member.span.into(),
                    object: Box::new(self.lower_expr(&member.object)?),
                    property: MemberProperty::Static(PropertyName::Identifier(
                        member.property.name.as_str().to_string(),
                    )),
                    optional: true,
                }),
                ChainElement::PrivateFieldExpression(member) => {
                    self.unsupported(
                        "private fields are not supported in v1",
                        Some(member.span.into()),
                    );
                    None
                }
                ChainElement::TSNonNullExpression(expression) => {
                    self.lower_expr(&expression.expression)
                }
            },
            Expression::ComputedMemberExpression(member) => Some(Expr::Member {
                span: member.span.into(),
                object: Box::new(self.lower_expr(&member.object)?),
                property: MemberProperty::Computed(Box::new(self.lower_expr(&member.expression)?)),
                optional: member.optional,
            }),
            Expression::StaticMemberExpression(member) => Some(Expr::Member {
                span: member.span.into(),
                object: Box::new(self.lower_expr(&member.object)?),
                property: MemberProperty::Static(PropertyName::Identifier(
                    member.property.name.as_str().to_string(),
                )),
                optional: member.optional,
            }),
            Expression::AwaitExpression(expression) => Some(Expr::Await {
                span: expression.span.into(),
                value: Box::new(self.lower_expr(&expression.argument)?),
            }),
            Expression::NewExpression(expression) => Some(Expr::New {
                span: expression.span.into(),
                callee: Box::new(self.lower_expr(&expression.callee)?),
                arguments: self.lower_call_args(&expression.arguments)?,
            }),
            Expression::ParenthesizedExpression(expression) => {
                self.lower_expr(&expression.expression)
            }
            Expression::MetaProperty(property) => {
                self.unsupported(
                    "meta properties are not supported",
                    Some(property.span.into()),
                );
                None
            }
            Expression::ImportExpression(expression) => {
                self.unsupported(
                    "dynamic import() is not supported",
                    Some(expression.span.into()),
                );
                None
            }
            Expression::RegExpLiteral(expression) => Some(Expr::RegExp {
                span: expression.span.into(),
                pattern: expression.regex.pattern.text.as_str().to_string(),
                flags: expression.regex.flags.to_inline_string().to_string(),
            }),
            Expression::Super(expression) => {
                self.unsupported("super is not supported in v1", Some(expression.span.into()));
                None
            }
            Expression::PrivateFieldExpression(expression) => {
                self.unsupported(
                    "private fields are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
            Expression::UpdateExpression(expression) => {
                self.unsupported(
                    "update expressions are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
            Expression::YieldExpression(expression) => {
                self.unsupported("yield is not supported in v1", Some(expression.span.into()));
                None
            }
            Expression::TaggedTemplateExpression(expression) => {
                self.unsupported(
                    "tagged templates are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
            Expression::ClassExpression(expression) => {
                self.unsupported(
                    "classes are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
            Expression::JSXElement(_)
            | Expression::JSXFragment(_)
            | Expression::TSAsExpression(_)
            | Expression::TSSatisfiesExpression(_)
            | Expression::TSInstantiationExpression(_)
            | Expression::TSNonNullExpression(_)
            | Expression::TSTypeAssertion(_)
            | Expression::PrivateInExpression(_) => {
                self.unsupported(
                    "unsupported expression form in v1",
                    Some(expression.span().into()),
                );
                None
            }
            _ => {
                self.unsupported(
                    "unsupported expression form in v1",
                    Some(expression.span().into()),
                );
                None
            }
        }
    }

    pub(super) fn lower_call_args(&mut self, args: &[Argument<'a>]) -> Option<Vec<Expr>> {
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            match arg {
                Argument::SpreadElement(spread) => {
                    self.unsupported(
                        "spread arguments are not supported in v1",
                        Some(spread.span.into()),
                    );
                    return None;
                }
                expression => lowered.push(self.lower_expr(expression.to_expression())?),
            }
        }
        Some(lowered)
    }

    pub(super) fn lower_assignment_target(
        &mut self,
        target: &AssignmentTarget<'a>,
    ) -> Option<AssignTarget> {
        match target {
            AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
                Some(AssignTarget::Identifier {
                    span: identifier.span.into(),
                    name: identifier.name.as_str().to_string(),
                })
            }
            AssignmentTarget::ComputedMemberExpression(member) => Some(AssignTarget::Member {
                span: member.span.into(),
                object: Box::new(self.lower_expr(&member.object)?),
                property: MemberProperty::Computed(Box::new(self.lower_expr(&member.expression)?)),
                optional: member.optional,
            }),
            AssignmentTarget::StaticMemberExpression(member) => Some(AssignTarget::Member {
                span: member.span.into(),
                object: Box::new(self.lower_expr(&member.object)?),
                property: MemberProperty::Static(PropertyName::Identifier(
                    member.property.name.as_str().to_string(),
                )),
                optional: member.optional,
            }),
            AssignmentTarget::PrivateFieldExpression(expression) => {
                self.unsupported(
                    "private fields are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
            AssignmentTarget::ArrayAssignmentTarget(target) => {
                self.unsupported(
                    "destructuring assignment is not supported in v1",
                    Some(target.span.into()),
                );
                None
            }
            AssignmentTarget::ObjectAssignmentTarget(target) => {
                self.unsupported(
                    "destructuring assignment is not supported in v1",
                    Some(target.span.into()),
                );
                None
            }
            _ => {
                self.unsupported(
                    "unsupported assignment target in v1",
                    Some(target.span().into()),
                );
                None
            }
        }
    }

    pub(super) fn lower_for_of_assignment_target(
        &mut self,
        target: &ForStatementLeft<'a>,
    ) -> Option<AssignTarget> {
        match target {
            ForStatementLeft::AssignmentTargetIdentifier(identifier) => {
                Some(AssignTarget::Identifier {
                    span: identifier.span.into(),
                    name: identifier.name.as_str().to_string(),
                })
            }
            ForStatementLeft::ComputedMemberExpression(member) => Some(AssignTarget::Member {
                span: member.span.into(),
                object: Box::new(self.lower_expr(&member.object)?),
                property: MemberProperty::Computed(Box::new(self.lower_expr(&member.expression)?)),
                optional: member.optional,
            }),
            ForStatementLeft::StaticMemberExpression(member) => Some(AssignTarget::Member {
                span: member.span.into(),
                object: Box::new(self.lower_expr(&member.object)?),
                property: MemberProperty::Static(PropertyName::Identifier(
                    member.property.name.as_str().to_string(),
                )),
                optional: member.optional,
            }),
            ForStatementLeft::PrivateFieldExpression(expression) => {
                self.unsupported(
                    "private fields are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
            ForStatementLeft::ArrayAssignmentTarget(target) => {
                self.unsupported(
                    "destructuring assignment is not supported in v1",
                    Some(target.span.into()),
                );
                None
            }
            ForStatementLeft::ObjectAssignmentTarget(target) => {
                self.unsupported(
                    "destructuring assignment is not supported in v1",
                    Some(target.span.into()),
                );
                None
            }
            _ => {
                self.unsupported(
                    "unsupported assignment target in v1",
                    Some(target.span().into()),
                );
                None
            }
        }
    }
}
