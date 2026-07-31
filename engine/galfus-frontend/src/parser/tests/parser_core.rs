use super::*;

#[test]
fn parse_stores_tokens_in_graph() {
    let source = source("fn main(): null { return }");

    let result = parse(&source);

    assert!(!result.ast().syntax().tokens().is_empty());
}

#[test]
fn parser_starts_at_first_token() {
    let source = source("fn main(): null { return }");
    let lex_result = lex(&source);
    let (tokens, diagnostics) = lex_result.into_parts();

    let parser = Parser::new(&source, tokens, diagnostics);

    assert_eq!(parser.current().kind(), &TokenKind::Fn);
}

#[test]
fn parser_bump_consumes_current_token() {
    let source = source("fn main(): null { return }");
    let lex_result = lex(&source);
    let (tokens, diagnostics) = lex_result.into_parts();

    let mut parser = Parser::new(&source, tokens, diagnostics);

    let token = parser.bump();

    assert_eq!(token.kind(), &TokenKind::Fn);
    assert_eq!(parser.current().kind(), &TokenKind::Identifier);
}

#[test]
fn parser_expect_consumes_expected_token() {
    let source = source("fn main(): null { return }");
    let lex_result = lex(&source);
    let (tokens, diagnostics) = lex_result.into_parts();

    let mut parser = Parser::new(&source, tokens, diagnostics);

    let token = parser.expect(TokenKind::Fn);

    assert!(token.is_some());
    assert_eq!(parser.current().kind(), &TokenKind::Identifier);
}

#[test]
fn parser_expect_reports_unexpected_token() {
    let source = source("return");
    let lex_result = lex(&source);
    let (tokens, diagnostics) = lex_result.into_parts();

    let mut parser = Parser::new(&source, tokens, diagnostics);

    let token = parser.expect(TokenKind::Fn);

    assert!(token.is_none());
    assert!(parser.graph.has_errors());

    let diagnostic = parser.graph.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "P0001");
    assert_eq!(diagnostic.message(), "expected `Fn`, found `Return`");
}

#[test]
fn parse_creates_source_file_root() {
    let source = source("fn main(): null { return }");

    let result = parse(&source);

    let syntax = result.graph().syntax();
    let root = syntax.root().expect("parse should create root node");
    let root_node = syntax.node(root).expect("root node should exist");

    assert_eq!(root_node.kind(), SyntaxNodeKind::SourceFile);
    assert_eq!(
        source.slice(root_node.span()),
        Some("fn main(): null { return }")
    );
}

#[test]
fn parse_creates_root_even_with_lexical_diagnostics() {
    let source = source("\"unterminated");

    let result = parse(&source);

    assert!(result.has_errors());
    assert!(result.graph().syntax().root().is_some());

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "L0002");
}

#[test]
fn parse_includes_token_tree_diagnostics() {
    let source = source("fn main(): null {");

    let result = parse(&source);

    assert!(result.has_errors());
    assert!(
        result
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "B0001")
    );
}

#[test]
fn parse_recovers_after_invalid_local_binding() {
    let source = source(
        "fn broken(): null {
            var value: =
            return
        }

        fn next(): null { return }",
    );

    let result = parse(&source);

    assert!(result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().expect("parse should create root node");

    assert_eq!(syntax.child_count(root), Some(2));
}
