use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_block_stmt(&mut self, block: &BlockStatement<'a>) -> Stmt {
        self.push_scope();
        self.predeclare_block(&block.body);
        let body = block
            .body
            .iter()
            .filter_map(|statement| self.lower_stmt(statement))
            .collect();
        self.pop_scope();
        Stmt::Block {
            span: block.span.into(),
            body,
        }
    }

    pub(super) fn lower_stmt(&mut self, statement: &Statement<'a>) -> Option<Stmt> {
        match statement {
            Statement::BlockStatement(block) => Some(self.lower_block_stmt(block)),
            Statement::BreakStatement(statement) => Some(Stmt::Break {
                span: statement.span.into(),
            }),
            Statement::ContinueStatement(statement) => Some(Stmt::Continue {
                span: statement.span.into(),
            }),
            Statement::EmptyStatement(statement) => Some(Stmt::Empty {
                span: statement.span.into(),
            }),
            Statement::ExpressionStatement(statement) => Some(Stmt::Expression {
                span: statement.span.into(),
                expression: self.lower_expr(&statement.expression)?,
            }),
            Statement::ForStatement(statement) => {
                let init = match &statement.init {
                    Some(ForStatementInit::VariableDeclaration(decl)) => {
                        Some(ForInit::VariableDecl {
                            kind: self.lower_binding_kind(decl.kind, decl.span)?,
                            declarators: decl
                                .declarations
                                .iter()
                                .filter_map(|declarator| self.lower_declarator(declarator))
                                .collect(),
                        })
                    }
                    Some(init) => Some(ForInit::Expression(self.lower_for_init_expr(init)?)),
                    None => None,
                };
                Some(Stmt::For {
                    span: statement.span.into(),
                    init,
                    test: statement
                        .test
                        .as_ref()
                        .and_then(|test| self.lower_expr(test)),
                    update: statement
                        .update
                        .as_ref()
                        .and_then(|expr| self.lower_expr(expr)),
                    body: Box::new(self.lower_stmt(&statement.body)?),
                })
            }
            Statement::ForOfStatement(statement) => {
                if statement.r#await {
                    self.unsupported(
                        "for await...of is not supported",
                        Some(statement.span.into()),
                    );
                    return None;
                }

                let head = self.lower_for_loop_head(&statement.left, "for...of")?;

                let iterable = self.lower_expr(&statement.right)?;
                let body = match &head {
                    crate::ir::ForOfHead::Binding { pattern, .. } => {
                        self.push_scope();
                        self.collect_ir_pattern_bindings(pattern);
                        let body = self.lower_stmt(&statement.body);
                        self.pop_scope();
                        Box::new(body?)
                    }
                    crate::ir::ForOfHead::Assignment { .. } => {
                        Box::new(self.lower_stmt(&statement.body)?)
                    }
                };

                Some(Stmt::ForOf {
                    span: statement.span.into(),
                    head,
                    iterable,
                    body,
                })
            }
            Statement::ForInStatement(statement) => {
                let head = self.lower_for_loop_head(&statement.left, "for...in")?;
                let iterable = self.lower_expr(&statement.right)?;
                let body = match &head {
                    crate::ir::ForOfHead::Binding { pattern, .. } => {
                        self.push_scope();
                        self.collect_ir_pattern_bindings(pattern);
                        let body = self.lower_stmt(&statement.body);
                        self.pop_scope();
                        Box::new(body?)
                    }
                    crate::ir::ForOfHead::Assignment { .. } => {
                        Box::new(self.lower_stmt(&statement.body)?)
                    }
                };

                Some(Stmt::ForIn {
                    span: statement.span.into(),
                    head,
                    object: iterable,
                    body,
                })
            }
            Statement::IfStatement(statement) => Some(Stmt::If {
                span: statement.span.into(),
                test: self.lower_expr(&statement.test)?,
                consequent: Box::new(self.lower_stmt(&statement.consequent)?),
                alternate: statement
                    .alternate
                    .as_ref()
                    .and_then(|alternate| self.lower_stmt(alternate))
                    .map(Box::new),
            }),
            Statement::ReturnStatement(statement) => Some(Stmt::Return {
                span: statement.span.into(),
                value: statement
                    .argument
                    .as_ref()
                    .and_then(|expr| self.lower_expr(expr)),
            }),
            Statement::SwitchStatement(statement) => Some(Stmt::Switch {
                span: statement.span.into(),
                discriminant: self.lower_expr(&statement.discriminant)?,
                cases: statement
                    .cases
                    .iter()
                    .map(|case| crate::ir::SwitchCase {
                        span: case.span.into(),
                        test: case.test.as_ref().and_then(|expr| self.lower_expr(expr)),
                        consequent: case
                            .consequent
                            .iter()
                            .filter_map(|statement| self.lower_stmt(statement))
                            .collect(),
                    })
                    .collect(),
            }),
            Statement::ThrowStatement(statement) => Some(Stmt::Throw {
                span: statement.span.into(),
                value: self.lower_expr(&statement.argument)?,
            }),
            Statement::TryStatement(statement) => Some(Stmt::Try {
                span: statement.span.into(),
                body: Box::new(self.lower_block_stmt(&statement.block)),
                catch: statement.handler.as_ref().map(|handler| {
                    self.push_scope();
                    if let Some(param) = &handler.param {
                        self.collect_pattern_bindings(&param.pattern);
                    }
                    self.predeclare_block(&handler.body.body);
                    let clause = crate::ir::CatchClause {
                        span: handler.span.into(),
                        parameter: handler
                            .param
                            .as_ref()
                            .and_then(|param| self.lower_pattern(&param.pattern)),
                        body: Box::new(self.lower_block_stmt(&handler.body)),
                    };
                    self.pop_scope();
                    clause
                }),
                finally: statement
                    .finalizer
                    .as_ref()
                    .map(|block| Box::new(self.lower_block_stmt(block))),
            }),
            Statement::VariableDeclaration(decl) => Some(Stmt::VariableDecl {
                span: decl.span.into(),
                kind: self.lower_binding_kind(decl.kind, decl.span)?,
                declarators: decl
                    .declarations
                    .iter()
                    .filter_map(|declarator| self.lower_declarator(declarator))
                    .collect(),
            }),
            Statement::WhileStatement(statement) => Some(Stmt::While {
                span: statement.span.into(),
                test: self.lower_expr(&statement.test)?,
                body: Box::new(self.lower_stmt(&statement.body)?),
            }),
            Statement::DoWhileStatement(statement) => Some(Stmt::DoWhile {
                span: statement.span.into(),
                body: Box::new(self.lower_stmt(&statement.body)?),
                test: self.lower_expr(&statement.test)?,
            }),
            Statement::FunctionDeclaration(function) => Some(Stmt::FunctionDecl {
                span: function.span.into(),
                function: self.lower_function(function, false)?,
            }),
            Statement::DebuggerStatement(statement) => {
                self.unsupported(
                    "debugger statements are not supported",
                    Some(statement.span.into()),
                );
                None
            }
            Statement::LabeledStatement(statement) => {
                self.unsupported(
                    "labeled statements are not supported in v1",
                    Some(statement.span.into()),
                );
                None
            }
            statement if statement.is_module_declaration() => {
                self.unsupported(
                    "module syntax is not supported",
                    Some(statement.span().into()),
                );
                None
            }
            Statement::WithStatement(statement) => {
                self.unsupported("with is not supported", Some(statement.span.into()));
                None
            }
            Statement::ClassDeclaration(class) => {
                self.unsupported("classes are not supported in v1", Some(class.span.into()));
                None
            }
            statement => {
                self.unsupported(
                    format!("unsupported statement form: {statement:?}"),
                    Some(statement.span().into()),
                );
                None
            }
        }
    }

    fn lower_for_loop_head(
        &mut self,
        left: &ForStatementLeft<'a>,
        loop_name: &str,
    ) -> Option<crate::ir::ForOfHead> {
        match left {
            ForStatementLeft::VariableDeclaration(decl) => {
                if decl.declarations.len() != 1 {
                    self.unsupported(
                        format!("{loop_name} currently requires exactly one let or const binding"),
                        Some(decl.span.into()),
                    );
                    return None;
                }
                let declarator = &decl.declarations[0];
                if declarator.init.is_some() {
                    self.unsupported(
                        format!("{loop_name} binding initializers are not supported"),
                        Some(declarator.span.into()),
                    );
                    return None;
                }
                Some(crate::ir::ForOfHead::Binding {
                    kind: self.lower_binding_kind(decl.kind, decl.span)?,
                    pattern: self.lower_pattern(&declarator.id)?,
                })
            }
            _ => Some(crate::ir::ForOfHead::Assignment {
                target: self.lower_for_of_assignment_target(left)?,
            }),
        }
    }

    pub(super) fn lower_binding_kind(
        &mut self,
        kind: VariableDeclarationKind,
        span: oxc_span::Span,
    ) -> Option<BindingKind> {
        match kind {
            VariableDeclarationKind::Let => Some(BindingKind::Let),
            VariableDeclarationKind::Const => Some(BindingKind::Const),
            _ => {
                self.unsupported("only let and const are supported", Some(span.into()));
                None
            }
        }
    }

    pub(super) fn lower_declarator(
        &mut self,
        declarator: &VariableDeclarator<'a>,
    ) -> Option<Declarator> {
        Some(Declarator {
            span: declarator.span.into(),
            pattern: self.lower_pattern(&declarator.id)?,
            initializer: declarator
                .init
                .as_ref()
                .and_then(|expr| self.lower_expr(expr)),
        })
    }
}
