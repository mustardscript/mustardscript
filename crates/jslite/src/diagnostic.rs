use std::fmt;

use crate::span::SourceSpan;

pub type JsliteResult<T> = Result<T, JsliteError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    Parse,
    Validation,
    Runtime,
    Limit,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFrame {
    pub function_name: Option<String>,
    pub span: SourceSpan,
}

impl Diagnostic {
    pub fn parse(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self {
            kind: DiagnosticKind::Parse,
            message: message.into(),
            span,
        }
    }

    pub fn validation(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self {
            kind: DiagnosticKind::Validation,
            message: message.into(),
            span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum JsliteError {
    Diagnostics(Vec<Diagnostic>),
    Message {
        kind: DiagnosticKind,
        message: String,
        span: Option<SourceSpan>,
        traceback: Vec<TraceFrame>,
    },
}

impl JsliteError {
    pub fn validation(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self::Message {
            kind: DiagnosticKind::Validation,
            message: message.into(),
            span,
            traceback: Vec::new(),
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Message {
            kind: DiagnosticKind::Runtime,
            message: message.into(),
            span: None,
            traceback: Vec::new(),
        }
    }

    pub fn runtime_at(message: impl Into<String>, span: SourceSpan) -> Self {
        Self::Message {
            kind: DiagnosticKind::Runtime,
            message: message.into(),
            span: Some(span),
            traceback: Vec::new(),
        }
    }

    pub fn with_traceback(mut self, frames: Vec<TraceFrame>) -> Self {
        if frames.is_empty() {
            return self;
        }
        if let Self::Message {
            span, traceback, ..
        } = &mut self
        {
            if span.is_none() {
                *span = frames.first().map(|frame| frame.span);
            }
            if traceback.is_empty() {
                *traceback = frames;
            }
        }
        self
    }
}

impl fmt::Display for JsliteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostics(items) => {
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{:?}: {}", item.kind, item.message)?;
                    if let Some(span) = item.span {
                        write!(f, " [{}..{}]", span.start, span.end)?;
                    }
                }
                Ok(())
            }
            Self::Message {
                kind,
                message,
                span,
                traceback,
            } => {
                write!(f, "{kind:?}: {message}")?;
                if let Some(span) = span {
                    write!(f, " [{}..{}]", span.start, span.end)?;
                }
                for frame in traceback {
                    write!(
                        f,
                        "\n  at {} [{}..{}]",
                        frame.function_name.as_deref().unwrap_or("<script>"),
                        frame.span.start,
                        frame.span.end
                    )?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for JsliteError {}
