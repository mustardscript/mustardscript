use super::*;
use swc_common::BytePos;
use swc_ecma_ast::{AssignOp, EsVersion};
use swc_ecma_lexer::{
    Lexer, StringInput, Syntax,
    common::{input::Tokens, lexer::Lexer as LexerState},
    token::{BinOpToken, Token},
};

// Oxc is recursive and has no configurable nesting guard. Check tokens before
// calling it, including for malformed/unterminated input and the module retry.
// A lexer is required: punctuation in strings/comments/regexes is not code.
const MAX_SOURCE_NESTING: usize = 64;

pub(super) fn check_source_nesting(source: &str) -> MustardResult<()> {
    let end = u32::try_from(source.len()).map_err(|_| {
        MustardError::Diagnostics(vec![Diagnostic::parse(
            "source exceeds parser byte range",
            None,
        )])
    })?;
    // Every code opener is a literal ASCII delimiter, including `${`. This
    // conservative raw count lets short/flat sources retain Oxc's exact lexical
    // diagnostics without scanning them twice. Literal text can only overcount.
    if source
        .bytes()
        .filter(|b| matches!(b, b'(' | b'[' | b'{'))
        .take(MAX_SOURCE_NESTING + 1)
        .count()
        <= MAX_SOURCE_NESTING
    {
        return Ok(());
    }
    let mut lexer = Lexer::new(
        Syntax::Es(Default::default()),
        EsVersion::EsNext,
        StringInput::new(source, BytePos(0), BytePos(end)),
        None,
    );
    let mut depth = 0usize;
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
            item = regexp.ok_or_else(|| {
                MustardError::Diagnostics(vec![Diagnostic::parse(
                    "unterminated regexp token",
                    None,
                )])
            })?;
        }
        match item.token {
            Token::LParen | Token::LBracket | Token::LBrace | Token::DollarLBrace => {
                depth += 1;
                if depth > MAX_SOURCE_NESTING {
                    return Err(MustardError::Diagnostics(vec![Diagnostic::parse(
                        "source nesting limit exceeded (maximum 64 delimiter/template levels)",
                        Some(SourceSpan::new(item.span.lo.0, item.span.hi.0)),
                    )]));
                }
            }
            Token::RParen | Token::RBracket | Token::RBrace => depth = depth.saturating_sub(1),
            Token::Error(_) => {
                return Err(MustardError::Diagnostics(vec![Diagnostic::parse(
                    "source tokenization failed",
                    Some(SourceSpan::new(item.span.lo.0, item.span.hi.0)),
                )]));
            }
            _ => {}
        }
    }
    Ok(())
}
