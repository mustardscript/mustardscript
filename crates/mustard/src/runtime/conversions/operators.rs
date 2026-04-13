use super::*;
use crate::ir::UpdateOp;

impl Runtime {
    pub(in crate::runtime) fn apply_update(
        &mut self,
        operator: UpdateOp,
        value: Value,
    ) -> MustardResult<Value> {
        let delta = match operator {
            UpdateOp::Increment => 1.0,
            UpdateOp::Decrement => -1.0,
        };
        match value {
            Value::BigInt(value) => Ok(Value::BigInt(match operator {
                UpdateOp::Increment => value + BigInt::from(1u8),
                UpdateOp::Decrement => value - BigInt::from(1u8),
            })),
            other => Ok(Value::Number(self.to_number(other)? + delta)),
        }
    }

    pub(in crate::runtime) fn apply_unary(
        &mut self,
        operator: UnaryOp,
        value: Value,
    ) -> MustardResult<Value> {
        match operator {
            UnaryOp::Plus => match value {
                Value::BigInt(_) => Err(MustardError::runtime(
                    "TypeError: unary plus is not supported for BigInt values",
                )),
                other => Ok(Value::Number(self.to_number(other)?)),
            },
            UnaryOp::Minus => match value {
                Value::BigInt(value) => Ok(Value::BigInt(-value)),
                other => Ok(Value::Number(-self.to_number(other)?)),
            },
            UnaryOp::Not => Ok(Value::Bool(!is_truthy(&value))),
            UnaryOp::Typeof => Ok(Value::String(
                match value {
                    Value::Undefined => "undefined",
                    Value::Null => "object",
                    Value::Bool(_) => "boolean",
                    Value::Number(_) => "number",
                    Value::BigInt(_) => "bigint",
                    Value::String(_) => "string",
                    Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => {
                        "function"
                    }
                    Value::Object(object) => {
                        if self.objects.get(object).is_some_and(|object| {
                            matches!(object.kind, ObjectKind::BoundFunction(_))
                        }) {
                            "function"
                        } else {
                            "object"
                        }
                    }
                    Value::Array(_)
                    | Value::Map(_)
                    | Value::Set(_)
                    | Value::Iterator(_)
                    | Value::Promise(_) => "object",
                }
                .to_string(),
            )),
            UnaryOp::Void => Ok(Value::Undefined),
        }
    }

    pub(in crate::runtime) fn apply_binary(
        &mut self,
        operator: BinaryOp,
        left: Value,
        right: Value,
    ) -> MustardResult<Value> {
        match operator {
            BinaryOp::Add => {
                if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                    Ok(Value::String(format!(
                        "{}{}",
                        self.to_string(left)?,
                        self.to_string(right)?
                    )))
                } else if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) {
                    self.apply_bigint_binary(BinaryOp::Add, left, right)
                } else {
                    Ok(Value::Number(
                        self.to_number(left)? + self.to_number(right)?,
                    ))
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem | BinaryOp::Pow
                if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) =>
            {
                self.apply_bigint_binary(operator, left, right)
            }
            BinaryOp::Sub => Ok(Value::Number(
                self.to_number(left)? - self.to_number(right)?,
            )),
            BinaryOp::Mul => Ok(Value::Number(
                self.to_number(left)? * self.to_number(right)?,
            )),
            BinaryOp::Div => Ok(Value::Number(
                self.to_number(left)? / self.to_number(right)?,
            )),
            BinaryOp::Rem => Ok(Value::Number(
                self.to_number(left)? % self.to_number(right)?,
            )),
            BinaryOp::Pow => Ok(Value::Number(
                self.to_number(left)?.powf(self.to_number(right)?),
            )),
            BinaryOp::In => Ok(Value::Bool(
                self.has_property_in_supported_surface(right, left)?,
            )),
            BinaryOp::Instanceof => {
                Ok(Value::Bool(self.instanceof_supported_surface(left, right)?))
            }
            BinaryOp::Eq | BinaryOp::StrictEq => Ok(Value::Bool(strict_equal(&left, &right))),
            BinaryOp::NotEq | BinaryOp::StrictNotEq => {
                Ok(Value::Bool(!strict_equal(&left, &right)))
            }
            BinaryOp::LessThan
            | BinaryOp::LessThanEq
            | BinaryOp::GreaterThan
            | BinaryOp::GreaterThanEq
                if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) =>
            {
                let (left, right) = bigint_operands(left, right, mixed_bigint_comparison_error)?;
                Ok(Value::Bool(match operator {
                    BinaryOp::LessThan => left < right,
                    BinaryOp::LessThanEq => left <= right,
                    BinaryOp::GreaterThan => left > right,
                    BinaryOp::GreaterThanEq => left >= right,
                    _ => unreachable!(),
                }))
            }
            BinaryOp::LessThan
            | BinaryOp::LessThanEq
            | BinaryOp::GreaterThan
            | BinaryOp::GreaterThanEq
                if matches!((&left, &right), (Value::String(_), Value::String(_))) =>
            {
                let (Value::String(left), Value::String(right)) = (left, right) else {
                    unreachable!()
                };
                Ok(Value::Bool(match operator {
                    BinaryOp::LessThan => left < right,
                    BinaryOp::LessThanEq => left <= right,
                    BinaryOp::GreaterThan => left > right,
                    BinaryOp::GreaterThanEq => left >= right,
                    _ => unreachable!(),
                }))
            }
            BinaryOp::LessThan => Ok(Value::Bool(self.to_number(left)? < self.to_number(right)?)),
            BinaryOp::LessThanEq => {
                Ok(Value::Bool(self.to_number(left)? <= self.to_number(right)?))
            }
            BinaryOp::GreaterThan => {
                Ok(Value::Bool(self.to_number(left)? > self.to_number(right)?))
            }
            BinaryOp::GreaterThanEq => {
                Ok(Value::Bool(self.to_number(left)? >= self.to_number(right)?))
            }
        }
    }
}

impl Runtime {
    fn instanceof_supported_surface(&self, left: Value, right: Value) -> MustardResult<bool> {
        match right {
            Value::BuiltinFunction(function) => match function {
                BuiltinFunction::FunctionCtor => Ok(match left {
                    Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => true,
                    Value::Object(object) => self
                        .objects
                        .get(object)
                        .is_some_and(|object| matches!(object.kind, ObjectKind::BoundFunction(_))),
                    _ => false,
                }),
                BuiltinFunction::ArrayCtor => Ok(matches!(left, Value::Array(_))),
                BuiltinFunction::ObjectCtor => Ok(matches!(
                    left,
                    Value::Object(_)
                        | Value::Array(_)
                        | Value::Map(_)
                        | Value::Set(_)
                        | Value::Iterator(_)
                        | Value::Promise(_)
                        | Value::Closure(_)
                        | Value::BuiltinFunction(_)
                        | Value::HostFunction(_)
                )),
                BuiltinFunction::MapCtor => Ok(matches!(left, Value::Map(_))),
                BuiltinFunction::SetCtor => Ok(matches!(left, Value::Set(_))),
                BuiltinFunction::PromiseCtor => Ok(matches!(left, Value::Promise(_))),
                BuiltinFunction::NumberCtor => Ok(matches!(
                    left,
                    Value::Object(object)
                        if self.objects.get(object).is_some_and(
                            |object| matches!(object.kind, ObjectKind::NumberObject(_))
                        )
                )),
                BuiltinFunction::DateCtor => Ok(matches!(
                    left,
                    Value::Object(object)
                        if self
                            .objects
                            .get(object)
                            .is_some_and(|object| matches!(object.kind, ObjectKind::Date(_)))
                )),
                BuiltinFunction::RegExpCtor => Ok(matches!(
                    left,
                    Value::Object(object)
                        if self
                            .objects
                            .get(object)
                            .is_some_and(|object| matches!(object.kind, ObjectKind::RegExp(_)))
                )),
                BuiltinFunction::StringCtor => Ok(matches!(
                    left,
                    Value::Object(object)
                        if self.objects.get(object).is_some_and(
                            |object| matches!(object.kind, ObjectKind::StringObject(_))
                        )
                )),
                BuiltinFunction::BooleanCtor => Ok(matches!(
                    left,
                    Value::Object(object)
                        if self.objects.get(object).is_some_and(
                            |object| matches!(object.kind, ObjectKind::BooleanObject(_))
                        )
                )),
                BuiltinFunction::ErrorCtor => Ok(matches!(
                    left,
                    Value::Object(object)
                        if self
                            .objects
                            .get(object)
                            .is_some_and(|object| matches!(object.kind, ObjectKind::Error(_)))
                )),
                BuiltinFunction::TypeErrorCtor => Ok(self.error_kind_matches(left, "TypeError")),
                BuiltinFunction::ReferenceErrorCtor => {
                    Ok(self.error_kind_matches(left, "ReferenceError"))
                }
                BuiltinFunction::RangeErrorCtor => Ok(self.error_kind_matches(left, "RangeError")),
                BuiltinFunction::SyntaxErrorCtor => {
                    Ok(self.error_kind_matches(left, "SyntaxError"))
                }
                _ => Err(MustardError::runtime(
                    "TypeError: right-hand side of instanceof must be a supported constructor",
                )),
            },
            Value::Closure(_) => Ok(false),
            _ => Err(MustardError::runtime(
                "TypeError: right-hand side of instanceof must be a supported constructor",
            )),
        }
    }

    fn error_kind_matches(&self, value: Value, expected: &str) -> bool {
        matches!(
            value,
            Value::Object(object)
                if self
                    .objects
                    .get(object)
                    .is_some_and(|object| matches!(&object.kind, ObjectKind::Error(name) if name == expected))
        )
    }
}

impl Runtime {
    fn apply_bigint_binary(
        &self,
        operator: BinaryOp,
        left: Value,
        right: Value,
    ) -> MustardResult<Value> {
        let (left, right) = bigint_operands(left, right, mixed_bigint_number_error)?;
        match operator {
            BinaryOp::Add => Ok(Value::BigInt(left + right)),
            BinaryOp::Sub => Ok(Value::BigInt(left - right)),
            BinaryOp::Mul => Ok(Value::BigInt(left * right)),
            BinaryOp::Div => {
                if right.is_zero() {
                    Err(MustardError::runtime("RangeError: BigInt division by zero"))
                } else {
                    Ok(Value::BigInt(left / right))
                }
            }
            BinaryOp::Rem => {
                if right.is_zero() {
                    Err(MustardError::runtime("RangeError: BigInt division by zero"))
                } else {
                    Ok(Value::BigInt(left % right))
                }
            }
            BinaryOp::Pow => {
                if right < BigInt::zero() {
                    return Err(MustardError::runtime(
                        "RangeError: BigInt exponent must be non-negative",
                    ));
                }
                let exponent = right.to_u32().ok_or_else(|| {
                    MustardError::runtime("RangeError: BigInt exponent is too large")
                })?;
                Ok(Value::BigInt(left.pow(exponent)))
            }
            _ => unreachable!(),
        }
    }
}

fn bigint_operands(
    left: Value,
    right: Value,
    mixed_error: fn() -> MustardError,
) -> MustardResult<(BigInt, BigInt)> {
    match (left, right) {
        (Value::BigInt(left), Value::BigInt(right)) => Ok((left, right)),
        (Value::BigInt(_), _) | (_, Value::BigInt(_)) => Err(mixed_error()),
        _ => unreachable!(),
    }
}

fn mixed_bigint_number_error() -> MustardError {
    MustardError::runtime("TypeError: cannot mix BigInt and Number values in arithmetic")
}

fn mixed_bigint_comparison_error() -> MustardError {
    MustardError::runtime("TypeError: cannot compare BigInt and Number values")
}
