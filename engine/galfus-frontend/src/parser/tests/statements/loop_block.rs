use super::super::*;

#[test]
fn parse_loop_statement() {
    let source = source("fn main(): null {\n  loop {\n    tick()\n  }\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let loop_statement = body_node.first_child().unwrap();
    let loop_node = syntax.node(loop_statement).unwrap();

    assert_eq!(loop_node.kind(), SyntaxNodeKind::LoopStatement);
    assert_eq!(loop_node.child_count(), 1);

    let loop_body = loop_node.first_child().unwrap();
    let loop_body_node = syntax.node(loop_body).unwrap();

    assert_eq!(loop_body_node.kind(), SyntaxNodeKind::Block);
    assert_eq!(loop_body_node.child_count(), 1);

    let inner_statement = loop_body_node.first_child().unwrap();

    assert_eq!(
        syntax.node(inner_statement).unwrap().kind(),
        SyntaxNodeKind::ExpressionStatement
    );
}

#[test]
fn parse_loop_statement_with_break() {
    let source = source("fn main(): null {\n  loop {\n    break\n  }\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let loop_statement = body_node.first_child().unwrap();
    let loop_node = syntax.node(loop_statement).unwrap();

    let loop_body = loop_node.first_child().unwrap();
    let loop_body_node = syntax.node(loop_body).unwrap();

    let break_statement = loop_body_node.first_child().unwrap();
    let break_node = syntax.node(break_statement).unwrap();

    assert_eq!(break_node.kind(), SyntaxNodeKind::BreakStatement);
    assert_eq!(source.slice(break_node.span()), Some("break"));
}

#[test]
fn parse_loop_statement_with_continue() {
    let source = source("fn main(): null {\n  loop {\n    continue\n  }\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let loop_statement = body_node.first_child().unwrap();
    let loop_node = syntax.node(loop_statement).unwrap();

    let loop_body = loop_node.first_child().unwrap();
    let loop_body_node = syntax.node(loop_body).unwrap();

    let continue_statement = loop_body_node.first_child().unwrap();
    let continue_node = syntax.node(continue_statement).unwrap();

    assert_eq!(continue_node.kind(), SyntaxNodeKind::ContinueStatement);

    assert_eq!(source.slice(continue_node.span()), Some("continue"));
}

#[test]
fn parse_loop_statement_with_if_break() {
    let source = source(
        "fn main(): null {\n  loop {\n    if done {\n      break\n    }\n    tick()\n  }\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let loop_statement = body_node.first_child().unwrap();
    let loop_node = syntax.node(loop_statement).unwrap();

    let loop_body = loop_node.first_child().unwrap();
    let loop_body_node = syntax.node(loop_body).unwrap();

    assert_eq!(loop_body_node.child_count(), 2);

    let if_statement = loop_body_node.first_child().unwrap();
    let tick_statement = loop_body_node.child(1).unwrap();

    assert_eq!(
        syntax.node(if_statement).unwrap().kind(),
        SyntaxNodeKind::IfStatement
    );

    assert_eq!(
        syntax.node(tick_statement).unwrap().kind(),
        SyntaxNodeKind::ExpressionStatement
    );

    let if_node = syntax.node(if_statement).unwrap();
    let if_body = if_node.child(1).unwrap();
    let if_body_node = syntax.node(if_body).unwrap();

    let break_statement = if_body_node.first_child().unwrap();

    assert_eq!(
        syntax.node(break_statement).unwrap().kind(),
        SyntaxNodeKind::BreakStatement
    );
}

#[test]
fn parse_loop_statement_allows_newline_before_block() {
    let source = source("fn main(): null {\n  loop\n  {\n    break\n  }\n}");

    let result = parse(&source);

    assert!(!result.has_errors());
}

#[test]
fn parse_loop_statement_requires_block() {
    let source = source("fn main(): null {\n  loop break\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "expected expression, found `Break`")
        .expect("missing expected expression diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0006");
}
