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
            BindingPattern::AssignmentPattern(pattern) => {
                self.unsupported(
                    "default destructuring is not supported in v1",
                    Some(pattern.span.into()),
                );
                None
            }
        }
    }

    pub(super) fn validate_function_params(&mut self, params: &FormalParameters<'a>) -> bool {
        let mut valid = true;
        for param in &params.items {
            if param.initializer.is_some() {
                self.unsupported(
                    "default parameters are not supported in v1",
                    Some(param.span.into()),
                );
                valid = false;
            }
            if !self.validate_param_pattern(&param.pattern) {
                valid = false;
            }
        }
        if let Some(rest) = &params.rest
            && !self.validate_param_pattern(&rest.rest.argument)
        {
            valid = false;
        }
        valid
    }

    pub(super) fn validate_param_pattern(&mut self, pattern: &BindingPattern<'a>) -> bool {
        match pattern {
            BindingPattern::BindingIdentifier(_) => true,
            BindingPattern::ObjectPattern(pattern) => {
                pattern
                    .properties
                    .iter()
                    .all(|property| self.validate_param_pattern(&property.value))
                    && pattern
                        .rest
                        .as_ref()
                        .is_none_or(|rest| self.validate_param_pattern(&rest.argument))
            }
            BindingPattern::ArrayPattern(pattern) => {
                pattern
                    .elements
                    .iter()
                    .flatten()
                    .all(|element| self.validate_param_pattern(element))
                    && pattern
                        .rest
                        .as_ref()
                        .is_none_or(|rest| self.validate_param_pattern(&rest.argument))
            }
            BindingPattern::AssignmentPattern(pattern) => {
                self.unsupported(
                    "default parameters are not supported in v1",
                    Some(pattern.span.into()),
                );
                false
            }
        }
    }

    pub(super) fn lower_function_params(
        &mut self,
        params: &FormalParameters<'a>,
    ) -> Option<(Vec<Pattern>, Option<Pattern>)> {
        let mut lowered = Vec::with_capacity(params.items.len());
        for param in &params.items {
            lowered.push(self.lower_pattern(&param.pattern)?);
        }
        let rest = params
            .rest
            .as_ref()
            .and_then(|rest| self.lower_pattern(&rest.rest.argument));
        if params.rest.is_some() && rest.is_none() {
            return None;
        }
        Some((lowered, rest))
    }
}
