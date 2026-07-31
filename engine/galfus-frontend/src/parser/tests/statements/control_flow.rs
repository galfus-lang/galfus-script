use super::super::*;

#[test]
fn parse_break_statement() {
    let source = source("fn main(): null {\n  break\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    assert_eq!(statement_node.kind(), SyntaxNodeKind::BreakStatement);
    assert_eq!(source.slice(statement_node.span()), Some("break"));
    assert!(statement_node.children().is_empty());
}

#[test]
fn parse_continue_statement() {
    let source = source("fn main(): null {\n  continue\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    assert_eq!(statement_node.kind(), SyntaxNodeKind::ContinueStatement);
    assert_eq!(source.slice(statement_node.span()), Some("continue"));
    assert!(statement_node.children().is_empty());
}

#[test]
fn parse_break_inside_for_statement() {
    let source = source("fn main(): null {\n  for item in items {\n    break\n  }\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let for_statement = body_node.first_child().unwrap();
    let for_node = syntax.node(for_statement).unwrap();

    assert_eq!(for_node.kind(), SyntaxNodeKind::ForStatement);

    let loop_body = for_node.child(2).unwrap();
    let loop_body_node = syntax.node(loop_body).unwrap();

    let break_statement = loop_body_node.first_child().unwrap();
    let break_node = syntax.node(break_statement).unwrap();

    assert_eq!(break_node.kind(), SyntaxNodeKind::BreakStatement);
    assert_eq!(source.slice(break_node.span()), Some("break"));
}

#[test]
fn parse_continue_inside_for_statement() {
    let source = source("fn main(): null {\n  for item in items {\n    continue\n  }\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let for_statement = body_node.first_child().unwrap();
    let for_node = syntax.node(for_statement).unwrap();

    let loop_body = for_node.child(2).unwrap();
    let loop_body_node = syntax.node(loop_body).unwrap();

    let continue_statement = loop_body_node.first_child().unwrap();
    let continue_node = syntax.node(continue_statement).unwrap();

    assert_eq!(continue_node.kind(), SyntaxNodeKind::ContinueStatement);

    assert_eq!(source.slice(continue_node.span()), Some("continue"));
}

#[test]
fn parse_break_and_continue_inside_if_blocks() {
    let source = source(
        "fn main(): null {\n  for item in items {\n    if item.done {\n      continue\n    }\n    if item.failed {\n      break\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let for_statement = body_node.first_child().unwrap();
    let for_node = syntax.node(for_statement).unwrap();

    let loop_body = for_node.child(2).unwrap();
    let loop_body_node = syntax.node(loop_body).unwrap();

    assert_eq!(loop_body_node.child_count(), 2);

    let first_if = loop_body_node.first_child().unwrap();
    let second_if = loop_body_node.child(1).unwrap();

    let first_if_node = syntax.node(first_if).unwrap();
    let second_if_node = syntax.node(second_if).unwrap();

    assert_eq!(first_if_node.kind(), SyntaxNodeKind::IfStatement);
    assert_eq!(second_if_node.kind(), SyntaxNodeKind::IfStatement);

    let first_if_block = first_if_node.child(1).unwrap();
    let second_if_block = second_if_node.child(1).unwrap();

    let continue_statement = syntax.node(first_if_block).unwrap().first_child().unwrap();
    let break_statement = syntax.node(second_if_block).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(continue_statement).unwrap().kind(),
        SyntaxNodeKind::ContinueStatement
    );

    assert_eq!(
        syntax.node(break_statement).unwrap().kind(),
        SyntaxNodeKind::BreakStatement
    );
}

#[test]
fn parse_break_requires_statement_terminator() {
    let source = source("fn main(): null {\n  break print(\"x\")\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.message() == "expected statement terminator, found `LeftParen`"
        })
        .expect("missing statement terminator diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}

#[test]
fn parse_continue_requires_statement_terminator() {
    let source = source("fn main(): null {\n  continue print(\"x\")\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.message() == "expected statement terminator, found `LeftParen`"
        })
        .expect("missing statement terminator diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}
