use super::*;

impl<'a> Lowerer<'a> {
    fn declare_name_in_current_scope(&mut self, name: &str, span: SourceSpan) {
        if let Some(scope) = self.scopes.last_mut()
            && !scope.insert(name.to_string())
        {
            self.unsupported(
                format!("SyntaxError: Identifier `{name}` has already been declared"),
                Some(span),
            );
        }
    }

    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn is_bound(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    pub(super) fn predeclare_block(&mut self, statements: &[Statement<'a>]) {
        for statement in statements {
            self.predeclare_stmt(statement);
        }
    }

    pub(super) fn predeclare_stmt(&mut self, statement: &Statement<'a>) {
        match statement {
            Statement::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    self.declare_name_in_current_scope(id.name.as_str(), id.span.into());
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

    pub(super) fn collect_pattern_bindings(&mut self, pattern: &BindingPattern<'a>) {
        match pattern {
            BindingPattern::BindingIdentifier(identifier) => self
                .declare_name_in_current_scope(identifier.name.as_str(), identifier.span.into()),
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

    pub(super) fn collect_ir_pattern_bindings(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Identifier { name, span } => self.declare_name_in_current_scope(name, *span),
            Pattern::Object {
                properties, rest, ..
            } => {
                for property in properties {
                    self.collect_ir_pattern_bindings(&property.value);
                }
                if let Some(rest) = rest {
                    self.collect_ir_pattern_bindings(rest);
                }
            }
            Pattern::Array { elements, rest, .. } => {
                for element in elements.iter().flatten() {
                    self.collect_ir_pattern_bindings(element);
                }
                if let Some(rest) = rest {
                    self.collect_ir_pattern_bindings(rest);
                }
            }
            Pattern::Default { target, .. } => self.collect_ir_pattern_bindings(target),
        }
    }
}
