use super::*;
use regex_syntax::ast::{self, Ast, ClassSet, ClassSetItem};
use regex_syntax::hir::{Class, ClassUnicode, ClassUnicodeRange, Hir, HirKind};

fn unsupported(message: &str) -> MustardError {
    MustardError::runtime(format!(
        "SyntaxError: unsupported regular expression: {message}"
    ))
}

fn legacy_canonicalize(ch: char) -> char {
    let mut upper = ch.to_uppercase();
    let first = upper.next().expect("uppercase is non-empty");
    if upper.next().is_some() || (!ch.is_ascii() && first.is_ascii()) {
        ch
    } else {
        first
    }
}

fn class_text(class: ClassUnicode) -> String {
    Hir::class(Class::Unicode(class)).to_string()
}

fn singleton(ch: char) -> ClassUnicode {
    ClassUnicode::new([ClassUnicodeRange::new(ch, ch)])
}

impl Runtime {
    pub(super) fn normalize_regexp_pattern(
        &mut self,
        pattern: &str,
        flags: RegExpFlagsState,
    ) -> MustardResult<String> {
        self.charge_native_helper_work(pattern.len())?;
        self.ensure_heap_capacity(pattern.len().saturating_mul(128))?;
        // In JS a class escape \b is a backspace, not a word-boundary assertion.
        let mut translated = String::new();
        let mut in_class = false;
        let mut chars = pattern.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    if in_class && next == 'b' {
                        translated.push_str(r"\x08");
                    } else {
                        translated.push(ch);
                        translated.push(next);
                    }
                } else {
                    translated.push(ch);
                }
            } else {
                if ch == '[' {
                    in_class = true;
                }
                if ch == ']' {
                    in_class = false;
                }
                translated.push(ch);
            }
        }
        let ast = ast::parse::ParserBuilder::new()
            .nest_limit(100)
            .build()
            .parse(&translated)
            .map_err(|error| {
                MustardError::runtime(format!("SyntaxError: invalid regular expression: {error}"))
            })?;
        self.regexp_ast_text(&ast, &translated, flags)
    }

    fn regexp_ast_text(
        &mut self,
        ast: &Ast,
        source: &str,
        flags: RegExpFlagsState,
    ) -> MustardResult<String> {
        self.charge_native_helper_work(1)?;
        Ok(match ast {
            Ast::Empty(_) => String::new(),
            Ast::Literal(literal) => {
                class_text(self.regexp_fold_class(singleton(literal.c), flags)?)
            }
            Ast::Dot(_) => if flags.dot_all {
                "(?s:.)"
            } else {
                r"[^\n\r\x{2028}\x{2029}]"
            }
            .to_string(),
            Ast::ClassPerl(class) => class_text(self.regexp_perl_class(class, flags)),
            Ast::ClassBracketed(class) => {
                class_text(self.regexp_bracket_class(class, source, flags)?)
            }
            Ast::ClassUnicode(class) => class_text(self.regexp_unicode_class(
                &source[class.span.start.offset..class.span.end.offset],
                flags,
            )?),
            Ast::Assertion(assertion) => match assertion.kind {
                ast::AssertionKind::WordBoundary => r"(?-u:\b)".to_string(),
                ast::AssertionKind::NotWordBoundary => r"(?-u:\B)".to_string(),
                ast::AssertionKind::StartLine | ast::AssertionKind::EndLine => ast.to_string(),
                _ => return Err(unsupported("non-ECMAScript assertion")),
            },
            Ast::Repetition(repetition) => format!(
                "(?:{}){}",
                self.regexp_ast_text(&repetition.ast, source, flags)?,
                &source[repetition.ast.span().end.offset..repetition.span.end.offset]
            ),
            Ast::Group(group) => {
                if let ast::GroupKind::NonCapturing(group_flags) = &group.kind
                    && !group_flags.items.is_empty()
                {
                    return Err(unsupported("inline flags"));
                }
                format!(
                    "{}{}{}",
                    &source[group.span.start.offset..group.ast.span().start.offset],
                    self.regexp_ast_text(&group.ast, source, flags)?,
                    &source[group.ast.span().end.offset..group.span.end.offset]
                )
            }
            Ast::Concat(concat) => {
                let mut out = String::new();
                for ast in &concat.asts {
                    out.push_str(&self.regexp_ast_text(ast, source, flags)?);
                }
                out
            }
            Ast::Alternation(alternation) => {
                let mut out = String::new();
                for (index, ast) in alternation.asts.iter().enumerate() {
                    if index != 0 {
                        out.push('|');
                    }
                    out.push_str(&self.regexp_ast_text(ast, source, flags)?);
                }
                out
            }
            Ast::Flags(_) => return Err(unsupported("inline flags")),
        })
    }

    fn regexp_perl_class(&self, class: &ast::ClassPerl, flags: RegExpFlagsState) -> ClassUnicode {
        let mut result = match class.kind {
            ast::ClassPerlKind::Digit => ClassUnicode::new([ClassUnicodeRange::new('0', '9')]),
            ast::ClassPerlKind::Word => {
                let mut ranges = vec![
                    ClassUnicodeRange::new('0', '9'),
                    ClassUnicodeRange::new('A', 'Z'),
                    ClassUnicodeRange::new('_', '_'),
                    ClassUnicodeRange::new('a', 'z'),
                ];
                if flags.unicode && flags.ignore_case {
                    ranges.extend([
                        ClassUnicodeRange::new('ſ', 'ſ'),
                        ClassUnicodeRange::new('K', 'K'),
                    ]);
                }
                ClassUnicode::new(ranges)
            }
            ast::ClassPerlKind::Space => ClassUnicode::new(
                [
                    ('\u{0009}', '\u{000D}'),
                    (' ', ' '),
                    ('\u{00A0}', '\u{00A0}'),
                    ('\u{1680}', '\u{1680}'),
                    ('\u{2000}', '\u{200A}'),
                    ('\u{2028}', '\u{2029}'),
                    ('\u{202F}', '\u{202F}'),
                    ('\u{205F}', '\u{205F}'),
                    ('\u{3000}', '\u{3000}'),
                    ('\u{FEFF}', '\u{FEFF}'),
                ]
                .map(|(start, end)| ClassUnicodeRange::new(start, end)),
            ),
        };
        if class.negated {
            result.negate();
        }
        result
    }

    fn regexp_unicode_class(
        &self,
        source: &str,
        flags: RegExpFlagsState,
    ) -> MustardResult<ClassUnicode> {
        if !flags.unicode {
            return Err(unsupported("Unicode property classes require the u flag"));
        }
        let hir = regex_syntax::ParserBuilder::new()
            .case_insensitive(flags.ignore_case)
            .build()
            .parse(source)
            .map_err(|error| {
                MustardError::runtime(format!("SyntaxError: invalid regular expression: {error}"))
            })?;
        match hir.kind() {
            HirKind::Class(Class::Unicode(class)) => Ok(class.clone()),
            _ => Err(unsupported("Unicode character class")),
        }
    }

    fn regexp_bracket_class(
        &mut self,
        class: &ast::ClassBracketed,
        source: &str,
        flags: RegExpFlagsState,
    ) -> MustardResult<ClassUnicode> {
        let mut value = match &class.kind {
            ClassSet::Item(item) => self.regexp_class_item(item, source, flags)?,
            ClassSet::BinaryOp(_) => {
                return Err(unsupported(
                    "character-class set syntax requires an unsupported flag",
                ));
            }
        };
        if class.negated {
            value.negate();
        }
        Ok(value)
    }

    fn regexp_class_item(
        &mut self,
        item: &ClassSetItem,
        source: &str,
        flags: RegExpFlagsState,
    ) -> MustardResult<ClassUnicode> {
        self.charge_native_helper_work(1)?;
        match item {
            ClassSetItem::Empty(_) => Ok(ClassUnicode::empty()),
            ClassSetItem::Literal(literal) => self.regexp_fold_class(singleton(literal.c), flags),
            ClassSetItem::Range(range) => self.regexp_fold_class(
                ClassUnicode::new([ClassUnicodeRange::new(range.start.c, range.end.c)]),
                flags,
            ),
            ClassSetItem::Perl(class) => Ok(self.regexp_perl_class(class, flags)),
            ClassSetItem::Unicode(class) => self.regexp_unicode_class(
                &source[class.span.start.offset..class.span.end.offset],
                flags,
            ),
            ClassSetItem::Union(union) => {
                let mut result = ClassUnicode::empty();
                for item in &union.items {
                    result.union(&self.regexp_class_item(item, source, flags)?);
                }
                Ok(result)
            }
            _ => Err(unsupported("non-ECMAScript nested/POSIX character class")),
        }
    }

    fn regexp_fold_class(
        &mut self,
        mut class: ClassUnicode,
        flags: RegExpFlagsState,
    ) -> MustardResult<ClassUnicode> {
        if !flags.ignore_case {
            return Ok(class);
        }
        if flags.unicode {
            class.case_fold_simple();
            return Ok(class);
        }
        let count = class
            .ranges()
            .iter()
            .map(|range| (range.end() as usize).saturating_sub(range.start() as usize) + 1)
            .sum::<usize>();
        self.charge_native_helper_work(count)?;
        self.ensure_heap_capacity(count.saturating_mul(128))?;
        let mut canonical = HashSet::new();
        for range in class.ranges() {
            for point in range.start() as u32..=range.end() as u32 {
                if let Some(ch) = char::from_u32(point) {
                    canonical.insert(legacy_canonicalize(ch));
                }
            }
        }
        class.case_fold_simple();
        let mut ranges = Vec::new();
        for range in class.ranges() {
            for point in range.start() as u32..=range.end() as u32 {
                self.charge_native_helper_work(1)?;
                if let Some(ch) = char::from_u32(point)
                    && canonical.contains(&legacy_canonicalize(ch))
                {
                    ranges.push(ClassUnicodeRange::new(ch, ch));
                }
            }
        }
        Ok(ClassUnicode::new(ranges))
    }
}
