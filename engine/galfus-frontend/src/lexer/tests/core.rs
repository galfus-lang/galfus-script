use super::*;

#[test]
fn lexer_returns_eof_for_empty_source() {
    assert_eq!(kinds(""), vec![TokenKind::Eof]);
}

#[test]
fn lexer_skips_horizontal_whitespace() {
    assert_eq!(kinds("   \t  "), vec![TokenKind::Eof]);
}

#[test]
fn lexer_reads_newline_tokens() {
    assert_eq!(
        kinds("\n\r\n\r"),
        vec![
            TokenKind::Newline,
            TokenKind::Newline,
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_tracks_newline_span() {
    let source = source("a\r\nb");
    let mut lexer = Lexer::new(&source);

    let first = lexer.next_token();
    let newline = lexer.next_token();
    let second = lexer.next_token();

    assert_eq!(first.kind(), &TokenKind::Identifier);
    assert_eq!(source.slice(first.span()), Some("a"));

    assert_eq!(newline.kind(), &TokenKind::Newline);
    assert_eq!(source.slice(newline.span()), Some("\r\n"));

    assert_eq!(second.kind(), &TokenKind::Identifier);
    assert_eq!(source.slice(second.span()), Some("b"));
}

#[test]
fn lexer_reads_single_char_delimiters() {
    assert_eq!(
        kinds("( ) { } [ ]"),
        vec![
            TokenKind::LeftParen,
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::RightBrace,
            TokenKind::LeftBracket,
            TokenKind::RightBracket,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_reads_single_char_punctuation() {
    assert_eq!(
        kinds(", . : ; @"),
        vec![
            TokenKind::Comma,
            TokenKind::Dot,
            TokenKind::Colon,
            TokenKind::Semicolon,
            TokenKind::At,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_reads_single_char_operators() {
    assert_eq!(
        kinds("+ - * / % ! = < > & | ^ ~"),
        vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Bang,
            TokenKind::Equal,
            TokenKind::Less,
            TokenKind::Greater,
            TokenKind::Amp,
            TokenKind::Pipe,
            TokenKind::Caret,
            TokenKind::Tilde,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_tracks_token_spans() {
    let source = source("  +  -");
    let mut lexer = Lexer::new(&source);

    let plus = lexer.next_token();
    let minus = lexer.next_token();
    let eof = lexer.next_token();

    assert_eq!(plus.kind(), &TokenKind::Plus);
    assert_eq!(plus.span().start(), 2);
    assert_eq!(plus.span().end(), 3);

    assert_eq!(minus.kind(), &TokenKind::Minus);
    assert_eq!(minus.span().start(), 5);
    assert_eq!(minus.span().end(), 6);

    assert_eq!(eof.kind(), &TokenKind::Eof);
    assert_eq!(eof.span().start(), 6);
    assert_eq!(eof.span().end(), 6);
}

#[test]
fn lexer_returns_unknown_for_unrecognized_character() {
    assert_eq!(kinds("¬"), vec![TokenKind::Unknown, TokenKind::Eof]);
    assert_eq!(kinds("´"), vec![TokenKind::Unknown, TokenKind::Eof]);
}

#[test]
fn lexer_reports_unknown_character() {
    let source = source("¬");
    let mut lexer = Lexer::new(&source);

    let token = lexer.next_token();

    assert_eq!(token.kind(), &TokenKind::Unknown);
    assert_eq!(token.span(), Span::new(SourceId::new(0), 0, "¬".len()));

    let diagnostics = lexer.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics.has_errors());

    let diagnostic = diagnostics.iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0004");
    assert_eq!(diagnostic.message(), "unknown character");
    assert_eq!(diagnostic.span(), Span::new(SourceId::new(0), 0, "¬".len()));
}

#[test]
fn lexer_reads_two_char_operators() {
    assert_eq!(
        kinds("== != <= >= && || :: .. => += -= *= /= %= &= |= ^= << >> ?. ?? ** ..."),
        vec![
            TokenKind::EqualEqual,
            TokenKind::BangEqual,
            TokenKind::LessEqual,
            TokenKind::GreaterEqual,
            TokenKind::AmpAmp,
            TokenKind::PipePipe,
            TokenKind::ColonColon,
            TokenKind::DotDot,
            TokenKind::Arrow,
            TokenKind::PlusEqual,
            TokenKind::MinusEqual,
            TokenKind::StarEqual,
            TokenKind::SlashEqual,
            TokenKind::PercentEqual,
            TokenKind::AmpEqual,
            TokenKind::PipeEqual,
            TokenKind::CaretEqual,
            TokenKind::ShiftLeft,
            TokenKind::ShiftRight,
            TokenKind::QuestionDot,
            TokenKind::QuestionQuestion,
            TokenKind::StarStar,
            TokenKind::DotDotDot,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_does_not_create_increment_or_decrement_tokens() {
    assert_eq!(
        kinds("++ --"),
        vec![
            TokenKind::Plus,
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Minus,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_reads_three_char_operators() {
    assert_eq!(
        kinds("**= <<= >>= ..."),
        vec![
            TokenKind::StarStarEqual,
            TokenKind::ShiftLeftEqual,
            TokenKind::ShiftRightEqual,
            TokenKind::DotDotDot,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_prefers_longest_operator_match() {
    assert_eq!(
        kinds("... .. . **= ** * <<= << <="),
        vec![
            TokenKind::DotDotDot,
            TokenKind::DotDot,
            TokenKind::Dot,
            TokenKind::StarStarEqual,
            TokenKind::StarStar,
            TokenKind::Star,
            TokenKind::ShiftLeftEqual,
            TokenKind::ShiftLeft,
            TokenKind::LessEqual,
            TokenKind::Eof,
        ]
    );
}
