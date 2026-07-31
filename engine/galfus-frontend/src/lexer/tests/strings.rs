use super::*;

#[test]
fn lexer_reads_double_quoted_string() {
    assert_eq!(kinds("\"hello\""), vec![TokenKind::String, TokenKind::Eof]);
}

#[test]
fn lexer_reads_single_quoted_string() {
    assert_eq!(kinds("'hello'"), vec![TokenKind::String, TokenKind::Eof]);
}

#[test]
fn lexer_tracks_string_span() {
    let source = source("  \"hello\"");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::String);
    assert_eq!(token.span().start(), 2);
    assert_eq!(token.span().end(), 9);
    assert_eq!(source.slice(token.span()), Some("\"hello\""));
}

#[test]
fn lexer_reports_unterminated_double_quoted_string() {
    let source = source("\"hello");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::String);

    let diagnostics = lexer.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics.has_errors());

    let diagnostic = diagnostics.iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0002");
    assert_eq!(diagnostic.message(), "unterminated string literal");
    assert_eq!(diagnostic.span(), Span::new(SourceId::new(0), 0, 6));
}

#[test]
fn lexer_reports_string_interrupted_by_newline() {
    let source = source("\"hello\nworld\"");
    let mut lexer = Lexer::new(&source);

    let first = lexer.next_token();
    let newline = lexer.next_token();
    let second = lexer.next_token();
    let third = lexer.next_token();

    assert_eq!(first.kind(), &TokenKind::String);
    assert_eq!(source.slice(first.span()), Some("\"hello"));

    assert_eq!(newline.kind(), &TokenKind::Newline);
    assert_eq!(source.slice(newline.span()), Some("\n"));

    assert_eq!(second.kind(), &TokenKind::Identifier);
    assert_eq!(source.slice(second.span()), Some("world"));

    assert_eq!(third.kind(), &TokenKind::String);
    assert_eq!(source.slice(third.span()), Some("\""));

    let diagnostics = lexer.diagnostics();

    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.has_errors());

    let mut diagnostics = diagnostics.iter();

    let first_diagnostic = diagnostics.next().unwrap();
    let second_diagnostic = diagnostics.next().unwrap();

    assert_eq!(first_diagnostic.code().as_str(), "L0002");
    assert_eq!(first_diagnostic.message(), "unterminated string literal");
    assert_eq!(first_diagnostic.span(), Span::new(SourceId::new(0), 0, 6));

    assert_eq!(second_diagnostic.code().as_str(), "L0002");
    assert_eq!(second_diagnostic.message(), "unterminated string literal");
    assert_eq!(
        second_diagnostic.span(),
        Span::new(SourceId::new(0), 12, 13)
    );
}

#[test]
fn lexer_reads_multiline_string() {
    assert_eq!(
        kinds("`line 1\nline 2`"),
        vec![TokenKind::String, TokenKind::Eof]
    );
}

#[test]
fn lexer_tracks_multiline_string_span() {
    let source = source("  `line 1\nline 2`");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::String);
    assert_eq!(source.slice(token.span()), Some("`line 1\nline 2`"));
}

#[test]
fn lexer_reports_unterminated_multiline_string() {
    let source = source("`line 1\nline 2");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::String);
    assert_eq!(source.slice(token.span()), Some("`line 1\nline 2"));

    let diagnostics = lexer.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics.has_errors());

    let diagnostic = diagnostics.iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0003");
    assert_eq!(
        diagnostic.message(),
        "unterminated multiline string literal"
    );
    assert_eq!(
        diagnostic.span(),
        Span::new(SourceId::new(0), 0, "`line 1\nline 2".len())
    );
}
