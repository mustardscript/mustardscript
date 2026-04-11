use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::{GetSpan, SourceType};

use crate::{
    diagnostic::{Diagnostic, JsliteError, JsliteResult},
    ir::*,
    span::SourceSpan,
};

const FORBIDDEN_AMBIENT_GLOBALS: &[&str] = &[
    "eval",
    "process",
    "module",
    "exports",
    "global",
    "require",
    "Function",
    "setTimeout",
    "setInterval",
    "queueMicrotask",
    "fetch",
];

pub fn compile(source: &str) -> JsliteResult<CompiledProgram> {
    let allocator = Allocator::default();
    let parser = Parser::new(&allocator, source, SourceType::default().with_script(true))
        .with_options(ParseOptions {
            allow_return_outside_function: false,
            ..ParseOptions::default()
        });
    let parsed = parser.parse();
    let mut diagnostics = Vec::new();
    diagnostics.extend(
        parsed
            .errors
            .into_iter()
            .map(|error| Diagnostic::parse(error.to_string(), None)),
    );
    if parsed.panicked {
        return Err(JsliteError::Diagnostics(diagnostics));
    }

    let mut lowerer = Lowerer::new(source);
    let script = lowerer.lower_program(&parsed.program);
    diagnostics.extend(lowerer.diagnostics);
    if !diagnostics.is_empty() {
        return Err(JsliteError::Diagnostics(diagnostics));
    }
    Ok(CompiledProgram {
        source: source.to_string(),
        script,
    })
}

struct Lowerer<'a> {
    diagnostics: Vec<Diagnostic>,
    _source: &'a str,
    scopes: Vec<HashSet<String>>,
}

impl<'a> Lowerer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            diagnostics: Vec::new(),
            _source: source,
            scopes: vec![HashSet::new()],
        }
    }

    fn lower_program(&mut self, program: &Program<'a>) -> Script {
        self.predeclare_block(&program.body);
        let body = program
            .body
            .iter()
            .filter_map(|statement| self.lower_stmt(statement))
            .collect();
        Script {
            span: program.span.into(),
            body,
        }
    }

    fn lower_block_stmt(&mut self, block: &BlockStatement<'a>) -> Stmt {
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

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind_name(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_bound(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn predeclare_block(&mut self, statements: &[Statement<'a>]) {
        for statement in statements {
            self.predeclare_stmt(statement);
        }
    }

    fn predeclare_stmt(&mut self, statement: &Statement<'a>) {
        match statement {
            Statement::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    self.bind_name(id.name.as_str());
                }
            }
            Statement::VariableDeclaration(decl) => {
                if decl.kind == VariableDeclarationKind::Var
                    || decl.kind == VariableDeclarationKind::Using
                    || decl.kind == VariableDeclarationKind::AwaitUsing
                {
                    self.unsupported("only let and const are supported", Some(decl.span.into()));
                    return;
                }
                for declarator in &decl.declarations {
                    self.collect_pattern_bindings(&declarator.id);
                }
            }
            _ => {}
        }
    }

    fn collect_pattern_bindings(&mut self, pattern: &BindingPattern<'a>) {
        match pattern {
            BindingPattern::BindingIdentifier(identifier) => {
                self.bind_name(identifier.name.as_str())
            }
            BindingPattern::ObjectPattern(pattern) => {
                for property in &pattern.properties {
                    self.collect_pattern_bindings(&property.value);
                }
                if let Some(rest) = &pattern.rest {
                    self.collect_pattern_bindings(&rest.argument);
                }
            }
            BindingPattern::ArrayPattern(pattern) => {
                for element in pattern.elements.iter().flatten() {
                    self.collect_pattern_bindings(element);
                }
                if let Some(rest) = &pattern.rest {
                    self.collect_pattern_bindings(&rest.argument);
                }
            }
            BindingPattern::AssignmentPattern(pattern) => {
                self.collect_pattern_bindings(&pattern.left);
            }
        }
    }

    fn lower_stmt(&mut self, statement: &Statement<'a>) -> Option<Stmt> {
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
            Statement::ForInStatement(statement) => {
                self.unsupported(
                    "for...in is not supported in v1",
                    Some(statement.span.into()),
                );
                None
            }
            Statement::ForOfStatement(statement) => {
                self.unsupported(
                    "for...of is not supported in v1",
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

    fn lower_binding_kind(
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

    fn lower_declarator(&mut self, declarator: &VariableDeclarator<'a>) -> Option<Declarator> {
        Some(Declarator {
            span: declarator.span.into(),
            pattern: self.lower_pattern(&declarator.id)?,
            initializer: declarator
                .init
                .as_ref()
                .and_then(|expr| self.lower_expr(expr)),
        })
    }

    fn lower_pattern(&mut self, pattern: &BindingPattern<'a>) -> Option<Pattern> {
        match pattern {
            BindingPattern::BindingIdentifier(identifier) => Some(Pattern::Identifier {
                span: identifier.span.into(),
                name: identifier.name.as_str().to_string(),
            }),
            BindingPattern::ObjectPattern(pattern) => Some(Pattern::Object {
                span: pattern.span.into(),
                properties: pattern
                    .properties
                    .iter()
                    .filter_map(|property| {
                        Some(ObjectPatternProperty {
                            span: property.span.into(),
                            key: self.lower_property_name(&property.key)?,
                            value: self.lower_pattern(&property.value)?,
                        })
                    })
                    .collect(),
                rest: pattern
                    .rest
                    .as_ref()
                    .and_then(|rest| self.lower_pattern(&rest.argument))
                    .map(Box::new),
            }),
            BindingPattern::ArrayPattern(pattern) => Some(Pattern::Array {
                span: pattern.span.into(),
                elements: pattern
                    .elements
                    .iter()
                    .map(|element| {
                        element
                            .as_ref()
                            .and_then(|pattern| self.lower_pattern(pattern))
                    })
                    .collect(),
                rest: pattern
                    .rest
                    .as_ref()
                    .and_then(|rest| self.lower_pattern(&rest.argument))
                    .map(Box::new),
            }),
            BindingPattern::AssignmentPattern(pattern) => Some(Pattern::Default {
                span: pattern.span.into(),
                target: Box::new(self.lower_pattern(&pattern.left)?),
                default_value: self.lower_expr(&pattern.right)?,
            }),
        }
    }

    fn lower_function(&mut self, function: &Function<'a>, is_arrow: bool) -> Option<FunctionExpr> {
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
        self.push_scope();
        for param in &function.params.items {
            self.collect_pattern_bindings(&param.pattern);
        }
        if let Some(rest) = &function.params.rest {
            self.collect_pattern_bindings(&rest.rest.argument);
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
            params: function
                .params
                .items
                .iter()
                .filter_map(|param| {
                    if param.initializer.is_some() {
                        Some(Pattern::Default {
                            span: param.span.into(),
                            target: Box::new(self.lower_pattern(&param.pattern)?),
                            default_value: self.lower_expr(param.initializer.as_deref()?)?,
                        })
                    } else {
                        self.lower_pattern(&param.pattern)
                    }
                })
                .collect(),
            body: lowered,
            is_async: function.r#async,
            is_arrow,
        })
    }

    fn lower_arrow_function(
        &mut self,
        function: &ArrowFunctionExpression<'a>,
    ) -> Option<FunctionExpr> {
        self.push_scope();
        for param in &function.params.items {
            self.collect_pattern_bindings(&param.pattern);
        }
        if let Some(rest) = &function.params.rest {
            self.collect_pattern_bindings(&rest.rest.argument);
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
            params: function
                .params
                .items
                .iter()
                .filter_map(|param| {
                    if param.initializer.is_some() {
                        Some(Pattern::Default {
                            span: param.span.into(),
                            target: Box::new(self.lower_pattern(&param.pattern)?),
                            default_value: self.lower_expr(param.initializer.as_deref()?)?,
                        })
                    } else {
                        self.lower_pattern(&param.pattern)
                    }
                })
                .collect(),
            body,
            is_async: function.r#async,
            is_arrow: true,
        })
    }

    fn lower_for_init_expr(&mut self, init: &ForStatementInit<'a>) -> Option<Expr> {
        match init {
            ForStatementInit::VariableDeclaration(_) => None,
            expression => self.lower_expr(expression.to_expression()),
        }
    }

    fn lower_expr(&mut self, expression: &Expression<'a>) -> Option<Expr> {
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
            Expression::RegExpLiteral(expression) => {
                self.unsupported(
                    "RegExp literals are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
            Expression::SequenceExpression(expression) => {
                self.unsupported(
                    "sequence expressions are not supported in v1",
                    Some(expression.span.into()),
                );
                None
            }
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
            Expression::BigIntLiteral(expression) => {
                self.unsupported(
                    "bigint is not supported in v1",
                    Some(expression.span.into()),
                );
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

    fn lower_call_args(&mut self, args: &[Argument<'a>]) -> Option<Vec<Expr>> {
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

    fn lower_property_name(&mut self, key: &PropertyKey<'a>) -> Option<PropertyName> {
        match key {
            PropertyKey::StaticIdentifier(identifier) => Some(PropertyName::Identifier(
                identifier.name.as_str().to_string(),
            )),
            PropertyKey::StringLiteral(literal) => {
                Some(PropertyName::String(literal.value.as_str().to_string()))
            }
            PropertyKey::NumericLiteral(literal) => Some(PropertyName::Number(literal.value)),
            _ => {
                self.unsupported("unsupported property key in v1", Some(key.span().into()));
                None
            }
        }
    }

    fn lower_assignment_target(&mut self, target: &AssignmentTarget<'a>) -> Option<AssignTarget> {
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

    fn lower_unary_op(&mut self, op: UnaryOperator, span: oxc_span::Span) -> Option<UnaryOp> {
        match op {
            UnaryOperator::UnaryPlus => Some(UnaryOp::Plus),
            UnaryOperator::UnaryNegation => Some(UnaryOp::Minus),
            UnaryOperator::LogicalNot => Some(UnaryOp::Not),
            UnaryOperator::Typeof => Some(UnaryOp::Typeof),
            UnaryOperator::Void => Some(UnaryOp::Void),
            UnaryOperator::Delete => {
                self.unsupported("delete is not supported in v1", Some(span.into()));
                None
            }
            _ => {
                self.unsupported("unsupported unary operator in v1", Some(span.into()));
                None
            }
        }
    }

    fn lower_binary_op(&mut self, op: BinaryOperator, span: oxc_span::Span) -> Option<BinaryOp> {
        match op {
            BinaryOperator::Addition => Some(BinaryOp::Add),
            BinaryOperator::Subtraction => Some(BinaryOp::Sub),
            BinaryOperator::Multiplication => Some(BinaryOp::Mul),
            BinaryOperator::Division => Some(BinaryOp::Div),
            BinaryOperator::Remainder => Some(BinaryOp::Rem),
            BinaryOperator::Equality => Some(BinaryOp::Eq),
            BinaryOperator::Inequality => Some(BinaryOp::NotEq),
            BinaryOperator::StrictEquality => Some(BinaryOp::StrictEq),
            BinaryOperator::StrictInequality => Some(BinaryOp::StrictNotEq),
            BinaryOperator::LessThan => Some(BinaryOp::LessThan),
            BinaryOperator::LessEqualThan => Some(BinaryOp::LessThanEq),
            BinaryOperator::GreaterThan => Some(BinaryOp::GreaterThan),
            BinaryOperator::GreaterEqualThan => Some(BinaryOp::GreaterThanEq),
            _ => {
                self.unsupported("unsupported binary operator in v1", Some(span.into()));
                None
            }
        }
    }

    fn lower_logical_op(
        &mut self,
        op: LogicalOperator,
        _span: oxc_span::Span,
    ) -> Option<LogicalOp> {
        match op {
            LogicalOperator::And => Some(LogicalOp::And),
            LogicalOperator::Or => Some(LogicalOp::Or),
            LogicalOperator::Coalesce => Some(LogicalOp::NullishCoalesce),
        }
    }

    fn lower_assign_op(
        &mut self,
        op: AssignmentOperator,
        span: oxc_span::Span,
    ) -> Option<AssignOp> {
        match op {
            AssignmentOperator::Assign => Some(AssignOp::Assign),
            AssignmentOperator::Addition => Some(AssignOp::AddAssign),
            AssignmentOperator::Subtraction => Some(AssignOp::SubAssign),
            AssignmentOperator::Multiplication => Some(AssignOp::MulAssign),
            AssignmentOperator::Division => Some(AssignOp::DivAssign),
            AssignmentOperator::LogicalNullish => Some(AssignOp::NullishAssign),
            _ => {
                self.unsupported("unsupported assignment operator in v1", Some(span.into()));
                None
            }
        }
    }

    fn unsupported(&mut self, message: impl Into<String>, span: Option<SourceSpan>) {
        self.diagnostics
            .push(Diagnostic::validation(message.into(), span));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_function_and_if() {
        let program = compile(
            r#"
            const add = (a, b = 1) => {
              if (a > b) {
                return a + b;
              }
              return a ?? b;
            };
            "#,
        )
        .expect("program should compile");

        assert_eq!(program.script.body.len(), 1);
        match &program.script.body[0] {
            Stmt::VariableDecl { declarators, .. } => {
                assert_eq!(declarators.len(), 1);
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn rejects_forbidden_free_require() {
        let error = compile("require('fs');").expect_err("should reject forbidden global");
        let text = error.to_string();
        assert!(text.contains("forbidden ambient global `require`"));
    }

    #[test]
    fn rejects_free_eval() {
        let error = compile("eval('1 + 1');").expect_err("should reject eval");
        let text = error.to_string();
        assert!(text.contains("forbidden ambient global `eval`"));
        assert!(text.contains("[0..4]"));
    }

    #[test]
    fn rejects_free_function_constructor() {
        let error = compile("new Function('return 1;');").expect_err("should reject Function");
        let text = error.to_string();
        assert!(text.contains("forbidden ambient global `Function`"));
        assert!(text.contains("[4..12]"));
    }

    #[test]
    fn allows_shadowed_require() {
        compile("const require = () => 1; require();").expect("shadowed require should compile");
    }

    #[test]
    fn allows_shadowed_function_identifier() {
        compile("const Function = (value) => value; Function(1);")
            .expect("shadowed Function should compile");
    }

    #[test]
    fn rejects_module_syntax() {
        let error = compile("export const x = 1;").expect_err("module syntax should fail");
        assert!(error.to_string().contains("module syntax"));
    }

    #[test]
    fn rejects_delete_operator() {
        let error = compile("delete record.value;").expect_err("delete should fail");
        let text = error.to_string();
        assert!(text.contains("delete is not supported in v1"));
        assert!(text.contains("[0..19]"));
    }
}
