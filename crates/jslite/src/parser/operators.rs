use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_property_name(&mut self, key: &PropertyKey<'a>) -> Option<PropertyName> {
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

    pub(super) fn lower_unary_op(
        &mut self,
        op: UnaryOperator,
        span: oxc_span::Span,
    ) -> Option<UnaryOp> {
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

    pub(super) fn lower_binary_op(
        &mut self,
        op: BinaryOperator,
        span: oxc_span::Span,
    ) -> Option<BinaryOp> {
        match op {
            BinaryOperator::Addition => Some(BinaryOp::Add),
            BinaryOperator::Subtraction => Some(BinaryOp::Sub),
            BinaryOperator::Multiplication => Some(BinaryOp::Mul),
            BinaryOperator::Division => Some(BinaryOp::Div),
            BinaryOperator::Remainder => Some(BinaryOp::Rem),
            BinaryOperator::Exponential => Some(BinaryOp::Pow),
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

    pub(super) fn lower_logical_op(
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

    pub(super) fn lower_assign_op(
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
}
