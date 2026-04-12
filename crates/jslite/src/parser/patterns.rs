use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_pattern(&mut self, pattern: &BindingPattern<'a>) -> Option<Pattern> {
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

    pub(super) fn lower_function_params(
        &mut self,
        params: &FormalParameters<'a>,
    ) -> Option<(Vec<Pattern>, Option<Pattern>, Vec<Stmt>, usize)> {
        let mut lowered = Vec::with_capacity(params.items.len());
        let mut param_init = Vec::new();
        let mut function_length = 0usize;
        let mut counted_default = false;
        for param in &params.items {
            if !counted_default {
                let has_default = param.initializer.is_some()
                    || matches!(param.pattern, BindingPattern::AssignmentPattern(_));
                if has_default {
                    counted_default = true;
                } else {
                    function_length += 1;
                }
            }
            let temp_name = self.fresh_internal_name("param");
            lowered.push(Pattern::Identifier {
                span: param.span.into(),
                name: temp_name.clone(),
            });
            let mut pattern = self.lower_pattern(&param.pattern)?;
            if let Some(initializer) = &param.initializer {
                pattern = Pattern::Default {
                    span: param.span.into(),
                    target: Box::new(pattern),
                    default_value: self.lower_expr(initializer)?,
                };
            }
            param_init.push(Stmt::VariableDecl {
                span: param.span.into(),
                kind: BindingKind::Let,
                declarators: vec![Declarator {
                    span: param.span.into(),
                    pattern,
                    initializer: Some(Expr::Identifier {
                        span: param.span.into(),
                        name: temp_name,
                    }),
                }],
            });
        }
        let rest = if let Some(rest) = &params.rest {
            let temp_name = self.fresh_internal_name("rest");
            param_init.push(Stmt::VariableDecl {
                span: rest.span.into(),
                kind: BindingKind::Let,
                declarators: vec![Declarator {
                    span: rest.span.into(),
                    pattern: self.lower_pattern(&rest.rest.argument)?,
                    initializer: Some(Expr::Identifier {
                        span: rest.span.into(),
                        name: temp_name.clone(),
                    }),
                }],
            });
            Some(Pattern::Identifier {
                span: rest.span.into(),
                name: temp_name,
            })
        } else {
            None
        };
        Some((lowered, rest, param_init, function_length))
    }
}
