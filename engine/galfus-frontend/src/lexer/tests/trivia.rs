use super::*;

#[test]
fn lexer_skips_line_comments_but_preserves_newline() {
    assert_eq!(
        kinds("// hello\nfn"),
        vec![TokenKind::Newline, TokenKind::Fn, TokenKind::Eof]
    );
}

#[test]
fn lexer_skips_line_comment_until_eof() {
    assert_eq!(kinds("// hello"), vec![TokenKind::Eof]);
}

#[test]
fn lexer_skips_block_comments() {
    assert_eq!(kinds("/* hello */ fn"), vec![TokenKind::Fn, TokenKind::Eof]);
}

#[test]
fn lexer_skips_block_comment_with_newlines_inside() {
    assert_eq!(
        kinds("/* hello\nworld */ fn"),
        vec![TokenKind::Fn, TokenKind::Eof]
    );
}

#[test]
fn lexer_skips_mixed_trivia_but_preserves_line_newline() {
    assert_eq!(
        kinds("  // line\n  /* block */  fn"),
        vec![TokenKind::Newline, TokenKind::Fn, TokenKind::Eof]
    );
}

#[test]
fn lexer_reports_unterminated_block_comment() {
    let source = source("/* hello");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::Eof);

    let diagnostics = lexer.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics.has_errors());

    let diagnostic = diagnostics.iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0001");
    assert_eq!(diagnostic.message(), "unterminated block comment");
    assert_eq!(diagnostic.span(), Span::new(SourceId::new(0), 0, 8));
}
