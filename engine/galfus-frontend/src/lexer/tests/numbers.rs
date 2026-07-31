use super::*;

#[test]
fn lexer_reads_integer_literals() {
    assert_eq!(
        kinds("0 10 123 1_000 999_999"),
        vec![
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_tracks_integer_span() {
    let source = source("  12345");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::Integer);
    assert_eq!(token.span().start(), 2);
    assert_eq!(token.span().end(), 7);
    assert_eq!(source.slice(token.span()), Some("12345"));
}

#[test]
fn lexer_stops_integer_before_identifier() {
    assert_eq!(
        kinds("123abc"),
        vec![TokenKind::Integer, TokenKind::Identifier, TokenKind::Eof]
    );
}

#[test]
fn lexer_reads_hex_integer_literals() {
    assert_eq!(
        kinds("0xFF 0xff 0x10 0XAB"),
        vec![
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_reads_binary_integer_literals() {
    assert_eq!(
        kinds("0b0 0b1010 0B1111_0000"),
        vec![
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_reads_octal_integer_literals() {
    assert_eq!(
        kinds("0o0 0o755 0O123"),
        vec![
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Integer,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_tracks_prefixed_integer_span() {
    let source = source("  0xFF");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::Integer);
    assert_eq!(source.slice(token.span()), Some("0xFF"));
}

#[test]
fn lexer_reads_float_literals() {
    assert_eq!(
        kinds("1.0 0.5 10.25 1_000.50"),
        vec![
            TokenKind::Float,
            TokenKind::Float,
            TokenKind::Float,
            TokenKind::Float,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_tracks_float_span() {
    let source = source("  10.25");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::Float);
    assert_eq!(source.slice(token.span()), Some("10.25"));
}

#[test]
fn lexer_does_not_parse_range_as_float() {
    assert_eq!(
        kinds("1..9"),
        vec![
            TokenKind::Integer,
            TokenKind::DotDot,
            TokenKind::Integer,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_does_not_parse_trailing_dot_as_float() {
    assert_eq!(
        kinds("1."),
        vec![TokenKind::Integer, TokenKind::Dot, TokenKind::Eof]
    );
}

#[test]
fn lexer_accepts_valid_numeric_separators() {
    let source = source("1_000 0xff_ff 0b1010_0101 0o755_123 1_000.50");
    let result = lex(&source);

    assert!(!result.has_errors());
}

#[test]
fn lexer_reports_trailing_numeric_separator() {
    let source = source("100_");
    let result = lex(&source);

    assert!(result.has_errors());
    assert_eq!(result.diagnostics().len(), 1);

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0005");
    assert_eq!(diagnostic.message(), "invalid numeric separator");
    assert_eq!(diagnostic.span(), Span::new(SourceId::new(0), 3, 4));
}

#[test]
fn lexer_reports_repeated_numeric_separator() {
    let source = source("1__000");
    let result = lex(&source);

    assert!(result.has_errors());
    assert_eq!(result.diagnostics().len(), 1);

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0005");
    assert_eq!(diagnostic.message(), "invalid numeric separator");
    assert_eq!(diagnostic.span(), Span::new(SourceId::new(0), 2, 3));
}

#[test]
fn lexer_reports_separator_after_numeric_prefix() {
    let source = source("0x_FF");
    let result = lex(&source);

    assert!(result.has_errors());
    assert_eq!(result.diagnostics().len(), 1);

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0005");
    assert_eq!(diagnostic.message(), "invalid numeric separator");
    assert_eq!(diagnostic.span(), Span::new(SourceId::new(0), 2, 3));
}

#[test]
fn lexer_reports_invalid_separator_in_float_fraction() {
    let source = source("10.5_");
    let result = lex(&source);

    assert!(result.has_errors());
    assert_eq!(result.diagnostics().len(), 1);

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0005");
    assert_eq!(diagnostic.message(), "invalid numeric separator");
    assert_eq!(diagnostic.span(), Span::new(SourceId::new(0), 4, 5));
}
