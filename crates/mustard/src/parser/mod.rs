mod expressions;
mod operators;
mod patterns;
mod scope;
mod statements;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::{GetSpan, SourceType};

use crate::{
    diagnostic::{Diagnostic, MustardError, MustardResult},
    ir::*,
    span::SourceSpan,
};

const FORBIDDEN_AMBIENT_GLOBALS: &[&str] = &[
    "arguments",
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

pub fn compile(source: &str) -> MustardResult<CompiledProgram> {
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
        return Err(MustardError::Diagnostics(diagnostics));
    }

    let mut lowerer = Lowerer::new(source);
    let script = lowerer.lower_program(&parsed.program);
    diagnostics.extend(lowerer.diagnostics);
    if !diagnostics.is_empty() {
        return Err(MustardError::Diagnostics(diagnostics));
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
    internal_name_counter: usize,
}

impl<'a> Lowerer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            diagnostics: Vec::new(),
            _source: source,
            scopes: vec![HashSet::new()],
            internal_name_counter: 0,
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

    fn unsupported(&mut self, message: impl Into<String>, span: Option<SourceSpan>) {
        self.diagnostics
            .push(Diagnostic::validation(message.into(), span));
    }

    fn fresh_internal_name(&mut self, prefix: &str) -> String {
        let name = format!("\0mustard_{prefix}_{}", self.internal_name_counter);
        self.internal_name_counter += 1;
        name
    }

    fn source_snippet(&self, span: SourceSpan) -> String {
        let start = span.start as usize;
        let end = span.end as usize;
        self._source.get(start..end).unwrap_or_default().to_string()
    }
}
