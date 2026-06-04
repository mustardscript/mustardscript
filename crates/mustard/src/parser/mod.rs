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
use oxc_parser::{ParseOptions, Parser, ParserReturn};
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
    let parsed = parse_source(&allocator, source);
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

fn parse_source<'a>(allocator: &'a Allocator, source: &'a str) -> ParserReturn<'a> {
    let parsed = parse_with_source_type(allocator, source, SourceType::default().with_script(true));
    if parsed.panicked || !has_top_level_await_parse_error(&parsed) {
        return parsed;
    }

    let module_parsed =
        parse_with_source_type(allocator, source, SourceType::default().with_module(true));
    if module_parsed.panicked {
        parsed
    } else {
        module_parsed
    }
}

fn parse_with_source_type<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    source_type: SourceType,
) -> ParserReturn<'a> {
    Parser::new(allocator, source, source_type)
        .with_options(ParseOptions {
            allow_return_outside_function: false,
            ..ParseOptions::default()
        })
        .parse()
}

fn has_top_level_await_parse_error(parsed: &ParserReturn<'_>) -> bool {
    parsed.errors.iter().any(|error| {
        error.to_string().contains(
            "`await` is only allowed within async functions and at the top levels of modules",
        )
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
