use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_property_name(&mut self, key: &PropertyKey<'a>) -> Option<PropertyName> {
        match key {
            PropertyKey::StaticIdentifier(identifier) => Some(PropertyName::Identifier(
                identifier.name.as_str().to_string(),
            )),
            PropertyKey::StringLiteral(literal) if literal.lone_surrogates => {
                self.unsupported(
                    "lone surrogates are not supported by the Unicode string profile",
                    Some(literal.span.into()),
                );
                None
            }
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
        _span: oxc_span::Span,
    ) -> Option<UnaryOp> {
        match op {
            UnaryOperator::UnaryPlus => Some(UnaryOp::Plus),
            UnaryOperator::UnaryNegation => Some(UnaryOp::Minus),
            UnaryOperator::LogicalNot => Some(UnaryOp::Not),
            UnaryOperator::Typeof => Some(UnaryOp::Typeof),
            UnaryOperator::Void => Some(UnaryOp::Void),
            UnaryOperator::Delete => Some(UnaryOp::Delete),
            UnaryOperator::BitwiseNot => Some(UnaryOp::BitNot),
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
            BinaryOperator::In => Some(BinaryOp::In),
            BinaryOperator::Instanceof => Some(BinaryOp::Instanceof),
            BinaryOperator::Equality | BinaryOperator::Inequality => {
                self.unsupported(
                    "loose equality (== and !=) is not supported; use === or !== with explicit conversions or nullish checks",
                    Some(span.into()),
                );
                None
            }
            BinaryOperator::StrictEquality => Some(BinaryOp::StrictEq),
            BinaryOperator::StrictInequality => Some(BinaryOp::StrictNotEq),
            BinaryOperator::LessThan => Some(BinaryOp::LessThan),
            BinaryOperator::LessEqualThan => Some(BinaryOp::LessThanEq),
            BinaryOperator::GreaterThan => Some(BinaryOp::GreaterThan),
            BinaryOperator::GreaterEqualThan => Some(BinaryOp::GreaterThanEq),
            BinaryOperator::BitwiseAnd => Some(BinaryOp::BitAnd),
            BinaryOperator::BitwiseOR => Some(BinaryOp::BitOr),
            BinaryOperator::BitwiseXOR => Some(BinaryOp::BitXor),
            BinaryOperator::ShiftLeft => Some(BinaryOp::ShiftLeft),
            BinaryOperator::ShiftRight => Some(BinaryOp::ShiftRight),
            BinaryOperator::ShiftRightZeroFill => Some(BinaryOp::ShiftRightUnsigned),
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
        _span: oxc_span::Span,
    ) -> Option<AssignOp> {
        match op {
            AssignmentOperator::Assign => Some(AssignOp::Assign),
            AssignmentOperator::Addition => Some(AssignOp::AddAssign),
            AssignmentOperator::Subtraction => Some(AssignOp::SubAssign),
            AssignmentOperator::Multiplication => Some(AssignOp::MulAssign),
            AssignmentOperator::Division => Some(AssignOp::DivAssign),
            AssignmentOperator::Remainder => Some(AssignOp::RemAssign),
            AssignmentOperator::Exponential => Some(AssignOp::PowAssign),
            AssignmentOperator::LogicalOr => Some(AssignOp::OrAssign),
            AssignmentOperator::LogicalAnd => Some(AssignOp::AndAssign),
            AssignmentOperator::LogicalNullish => Some(AssignOp::NullishAssign),
            AssignmentOperator::BitwiseAnd => Some(AssignOp::BitAndAssign),
            AssignmentOperator::BitwiseOR => Some(AssignOp::BitOrAssign),
            AssignmentOperator::BitwiseXOR => Some(AssignOp::BitXorAssign),
            AssignmentOperator::ShiftLeft => Some(AssignOp::ShiftLeftAssign),
            AssignmentOperator::ShiftRight => Some(AssignOp::ShiftRightAssign),
            AssignmentOperator::ShiftRightZeroFill => Some(AssignOp::ShiftRightUnsignedAssign),
        }
    }
}
