use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    diagnostic::{JsliteError, JsliteResult},
    span::SourceSpan,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StructuredNumber {
    Finite(f64),
    NaN,
    Infinity,
    NegInfinity,
    NegZero,
}

impl StructuredNumber {
    pub fn from_f64(value: f64) -> Self {
        if value.is_nan() {
            Self::NaN
        } else if value == 0.0f64 && value.is_sign_negative() {
            Self::NegZero
        } else if value == f64::INFINITY {
            Self::Infinity
        } else if value == f64::NEG_INFINITY {
            Self::NegInfinity
        } else {
            Self::Finite(value)
        }
    }

    pub fn to_f64(&self) -> f64 {
        match self {
            Self::Finite(value) => *value,
            Self::NaN => f64::NAN,
            Self::Infinity => f64::INFINITY,
            Self::NegInfinity => f64::NEG_INFINITY,
            Self::NegZero => -0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StructuredValue {
    Undefined,
    Null,
    Hole,
    Bool(bool),
    String(String),
    Number(StructuredNumber),
    Array(Vec<StructuredValue>),
    Object(IndexMap<String, StructuredValue>),
}

impl StructuredValue {
    pub fn validate_plain_object(
        prototype_is_plain: bool,
        has_accessors: bool,
        has_cycles: bool,
        span: Option<SourceSpan>,
    ) -> JsliteResult<()> {
        if !prototype_is_plain {
            return Err(JsliteError::validation(
                "host objects with custom prototypes cannot cross the host boundary",
                span,
            ));
        }
        if has_accessors {
            return Err(JsliteError::validation(
                "host objects with accessors cannot cross the host boundary",
                span,
            ));
        }
        if has_cycles {
            return Err(JsliteError::validation(
                "cyclic values cannot cross the host boundary",
                span,
            ));
        }
        Ok(())
    }
}

impl From<bool> for StructuredValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<&str> for StructuredValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<String> for StructuredValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<f64> for StructuredValue {
    fn from(value: f64) -> Self {
        Self::Number(StructuredNumber::from_f64(value))
    }
}
