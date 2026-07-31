use super::*;
use crate::{DelimiterKind, TokenKind, lex};
use galfus_core::{SourceFile, SourceId};

fn tree_for(text: &str) -> TokenTreeResult {
    let source = SourceFile::new(SourceId::new(0), "main.gfs".to_string(), text.to_string());
    let lexed = lex(&source);
    build_token_tree(lexed.tokens().to_vec())
}

#[test]
fn groups_nested_delimiters() {
    let result = tree_for("call([value])");

    assert!(!result.has_errors());
    assert_eq!(result.tree().items().len(), 3);

    let outer = result.tree().items()[1].group().unwrap();
    assert_eq!(outer.delimiter(), DelimiterKind::Parenthesis);
    assert!(outer.closing().is_some());

    let inner = outer.items()[0].group().unwrap();
    assert_eq!(inner.delimiter(), DelimiterKind::Bracket);
    assert!(inner.closing().is_some());
}

#[test]
fn reports_an_unclosed_delimiter_at_its_opening_token() {
    let result = tree_for("call(");
    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "B0001");
    assert_eq!(diagnostic.span().start(), 4);

    let group = result.tree().items()[1].group().unwrap();
    assert!(group.closing().is_none());
}

#[test]
fn recovers_a_nested_group_when_its_parent_closes() {
    let result = tree_for("([)");

    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(
        result.diagnostics().iter().next().unwrap().code().as_str(),
        "B0001"
    );

    let outer = result.tree().items()[0].group().unwrap();
    assert!(outer.closing().is_some());
    assert!(outer.items()[0].group().unwrap().closing().is_none());
}

#[test]
fn retains_an_unexpected_closing_delimiter_as_a_token() {
    let result = tree_for(")");

    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(
        result.diagnostics().iter().next().unwrap().code().as_str(),
        "B0002"
    );
    assert_eq!(
        result.tree().items()[0].token().unwrap().kind(),
        &TokenKind::RightParen
    );
}

#[test]
fn projects_back_to_the_original_token_stream() {
    let source = SourceFile::new(
        SourceId::new(0),
        "main.gfs".to_string(),
        "call([value])".to_string(),
    );
    let lexed = lex(&source);
    let tokens = lexed.tokens().to_vec();
    let result = build_token_tree(tokens.clone());

    assert_eq!(result.tree().clone().into_tokens(), tokens);
}
