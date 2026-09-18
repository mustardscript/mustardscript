use super::*;
use swc_common::BytePos;
use swc_ecma_ast::{AssignOp, EsVersion};
use swc_ecma_lexer::{
    Lexer, StringInput, Syntax,
    common::{input::Tokens, lexer::Lexer as LexerState},
    token::{BinOpToken, Keyword, Token, Word},
};

// Oxc is recursive and has no configurable nesting guard. Check tokens before
// calling it, including for malformed/unterminated input and the module retry.
// A lexer is required: punctuation in strings/comments/regexes is not code.
const MAX_SOURCE_NESTING: usize = 64;

// This is a conservative token-complexity bound, not a second JS parser.
// Delimiters alone miss arrow, label, unary, member and unbraced statement chains.
const MAX_CHAIN_TOKENS: usize = 128;

pub(super) struct LexicalFailure {
    pub(super) prefix_end: usize,
    pub(super) message: String,
}

pub(super) fn check_source_nesting(source: &str) -> MustardResult<Option<LexicalFailure>> {
    let end = u32::try_from(source.len()).map_err(|_| {
        MustardError::Diagnostics(vec![Diagnostic::parse(
            "source exceeds parser byte range",
            None,
        )])
    })?;
    let mut lexer = Lexer::new(
        Syntax::Es(Default::default()),
        EsVersion::EsNext,
        StringInput::new(source, BytePos(0), BytePos(end)),
        None,
    );
    let mut chains = vec![0usize];
    let mut statement_prefixes = vec![0usize];
    let mut previous = None;

    loop {
        let expression_allowed = LexerState::state(&lexer).is_expr_allowed;
        let Some(mut item) = lexer.next() else {
            break;
        };
        // SWC exposes regexp rescanning separately, using the lexer's own
        // expression-context tracking to distinguish it from division.
        if expression_allowed
            && matches!(
                item.token,
                Token::BinOp(BinOpToken::Div) | Token::AssignOp(AssignOp::DivAssign)
            )
        {
            lexer.set_next_regexp(Some(item.span.lo));
            let regexp = lexer.next();
            lexer.set_next_regexp(None);
            let Some(regexp) = regexp else {
                return Ok(Some(LexicalFailure {
                    prefix_end: source.len(),
                    message: "unterminated regexp token".into(),
                }));
            };
            item = regexp;
        }
        // A semicolon completes a statement unless an `else` still attaches
        // to it. Do not reset at `;else`: each earlier if remains on Oxc's stack.
        let follows_else = matches!(item.token, Token::Word(Word::Keyword(Keyword::Else)));
        let continues_expression_or_statement = matches!(
            item.token,
            Token::Word(Word::Keyword(
                Keyword::Else | Keyword::Catch | Keyword::Finally
            )) | Token::BinOp(_)
                | Token::AssignOp(_)
                | Token::Arrow
                | Token::Dot
                | Token::QuestionMark
                | Token::Colon
                | Token::BackQuote
                | Token::LParen
                | Token::LBracket
                | Token::RParen
                | Token::RBracket
                | Token::RBrace
                | Token::PlusPlus
                | Token::MinusMinus
        );
        if (matches!(previous, Some(Token::Semi)) && !follows_else)
            || (matches!(previous, Some(Token::RBrace)) && !continues_expression_or_statement)
        {
            *chains.last_mut().unwrap() = 0;
            *statement_prefixes.last_mut().unwrap() = 0;
        } else if matches!(previous, Some(Token::Comma)) {
            // Commas separate flat lists, but cannot discard enclosing
            // unbraced statements (e.g. `if (x) a,b; else if ...`).
            *chains.last_mut().unwrap() = *statement_prefixes.last().unwrap();
        }
        if matches!(
            item.token,
            Token::Word(Word::Keyword(
                Keyword::If
                    | Keyword::Else
                    | Keyword::For
                    | Keyword::While
                    | Keyword::Do
                    | Keyword::With
            ))
        ) {
            *statement_prefixes.last_mut().unwrap() += 1;
        }
        match &item.token {
            Token::Error(error) => {
                // Parsing only the lexically checked prefix preserves Oxc's
                // precise diagnostics without handing an unchecked suffix to
                // its recursive parser after the lexer stops.
                return Ok(Some(LexicalFailure {
                    prefix_end: (item.span.hi.0 as usize).min(source.len()),
                    message: error.kind().msg().into_owned(),
                }));
            }
            Token::LParen | Token::LBracket | Token::LBrace | Token::DollarLBrace => {
                *chains.last_mut().unwrap() += 1;
                chains.push(0);
                statement_prefixes.push(0);
                if chains.len() > MAX_SOURCE_NESTING + 1 {
                    return Err(nesting_error(
                        "maximum 64 delimiter/template levels",
                        item.span,
                    ));
                }
            }
            Token::RParen | Token::RBracket | Token::RBrace => {
                if chains.len() > 1 {
                    chains.pop();
                    statement_prefixes.pop();
                }
                *chains.last_mut().unwrap() += 1;
            }
            _ => *chains.last_mut().unwrap() += 1,
        }
        if chains.iter().sum::<usize>() > MAX_CHAIN_TOKENS {
            return Err(nesting_error(
                "maximum 128 uninterrupted chain tokens",
                item.span,
            ));
        }
        previous = Some(item.token);
    }
    Ok(None)
}

fn nesting_error(reason: &str, span: swc_common::Span) -> MustardError {
    MustardError::Diagnostics(vec![Diagnostic::parse(
        format!("source nesting limit exceeded ({reason})"),
        Some(SourceSpan::new(span.lo.0, span.hi.0)),
    )])
}
