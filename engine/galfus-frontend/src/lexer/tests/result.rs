use super::*;

#[test]
fn lex_returns_tokens_and_diagnostics() {
    let source = source("fn main(): null {}");

    let result = lex(&source);

    let kinds: Vec<TokenKind> = result
        .tokens()
        .iter()
        .map(|token| token.kind().clone())
        .collect();

    assert_eq!(
        kinds,
        vec![
            TokenKind::Fn,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::RightParen,
            TokenKind::Colon,
            TokenKind::Null,
            TokenKind::LeftBrace,
            TokenKind::RightBrace,
            TokenKind::Eof,
        ]
    );

    assert!(!result.has_errors());
    assert!(result.diagnostics().is_empty());
}

#[test]
fn lex_preserves_newline_tokens() {
    let source = source("fn main(): null {\n  return\n}");

    let result = lex(&source);

    let kinds: Vec<TokenKind> = result
        .tokens()
        .iter()
        .map(|token| token.kind().clone())
        .collect();

    assert_eq!(
        kinds,
        vec![
            TokenKind::Fn,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::RightParen,
            TokenKind::Colon,
            TokenKind::Null,
            TokenKind::LeftBrace,
            TokenKind::Newline,
            TokenKind::Return,
            TokenKind::Newline,
            TokenKind::RightBrace,
            TokenKind::Eof,
        ]
    );
}
