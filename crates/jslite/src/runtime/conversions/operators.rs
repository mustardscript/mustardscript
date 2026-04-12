use super::*;

impl Runtime {
    pub(in crate::runtime) fn apply_unary(
        &mut self,
        operator: UnaryOp,
        value: Value,
    ) -> JsliteResult<Value> {
        match operator {
            UnaryOp::Plus => match value {
                Value::BigInt(_) => Err(JsliteError::runtime(
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
                    Value::Object(_)
                    | Value::Array(_)
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
    ) -> JsliteResult<Value> {
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
    fn apply_bigint_binary(
        &self,
        operator: BinaryOp,
        left: Value,
        right: Value,
    ) -> JsliteResult<Value> {
        let (left, right) = bigint_operands(left, right, mixed_bigint_number_error)?;
        match operator {
            BinaryOp::Add => Ok(Value::BigInt(left + right)),
            BinaryOp::Sub => Ok(Value::BigInt(left - right)),
            BinaryOp::Mul => Ok(Value::BigInt(left * right)),
            BinaryOp::Div => {
                if right.is_zero() {
                    Err(JsliteError::runtime("RangeError: BigInt division by zero"))
                } else {
                    Ok(Value::BigInt(left / right))
                }
            }
            BinaryOp::Rem => {
                if right.is_zero() {
                    Err(JsliteError::runtime("RangeError: BigInt division by zero"))
                } else {
                    Ok(Value::BigInt(left % right))
                }
            }
            BinaryOp::Pow => {
                if right < BigInt::zero() {
                    return Err(JsliteError::runtime(
                        "RangeError: BigInt exponent must be non-negative",
                    ));
                }
                let exponent = right.to_u32().ok_or_else(|| {
                    JsliteError::runtime("RangeError: BigInt exponent is too large")
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
    mixed_error: fn() -> JsliteError,
) -> JsliteResult<(BigInt, BigInt)> {
    match (left, right) {
        (Value::BigInt(left), Value::BigInt(right)) => Ok((left, right)),
        (Value::BigInt(_), _) | (_, Value::BigInt(_)) => Err(mixed_error()),
        _ => unreachable!(),
    }
}

fn mixed_bigint_number_error() -> JsliteError {
    JsliteError::runtime("TypeError: cannot mix BigInt and Number values in arithmetic")
}

fn mixed_bigint_comparison_error() -> JsliteError {
    JsliteError::runtime("TypeError: cannot compare BigInt and Number values")
}
