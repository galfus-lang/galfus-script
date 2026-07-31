use super::super::*;

#[test]
fn parse_return_statement_with_integer_expression() {
    let source = source("fn one(): i32 { return 1 }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let return_statement = body_node.first_child().unwrap();
    let return_node = syntax.node(return_statement).unwrap();

    assert_eq!(return_node.kind(), SyntaxNodeKind::ReturnStatement);
    assert_eq!(source.slice(return_node.span()), Some("return 1"));
    assert_eq!(return_node.child_count(), 1);

    let expression = return_node.first_child().unwrap();

    assert_eq!(
        syntax.node(expression).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );

    assert_eq!(
        source.slice(syntax.node(expression).unwrap().span()),
        Some("1")
    );
}

#[test]
fn parse_return_statement_with_null_expression() {
    let source = source("fn none(): null { return null }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let return_statement = body_node.first_child().unwrap();
    let return_node = syntax.node(return_statement).unwrap();

    assert_eq!(return_node.kind(), SyntaxNodeKind::ReturnStatement);
    assert_eq!(source.slice(return_node.span()), Some("return null"));
    assert_eq!(return_node.child_count(), 1);

    let expression = return_node.first_child().unwrap();

    assert_eq!(
        syntax.node(expression).unwrap().kind(),
        SyntaxNodeKind::NullLiteral
    );
}

#[test]
fn parse_return_statement_with_cast_expression() {
    let source = source("fn byte(): u8 { return <u8> 300 }");

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let return_statement = body_node.first_child().unwrap();
    let return_node = syntax.node(return_statement).unwrap();

    assert_eq!(return_node.kind(), SyntaxNodeKind::ReturnStatement);
    assert_eq!(source.slice(return_node.span()), Some("return <u8> 300"));
    assert_eq!(return_node.child_count(), 1);

    let expression = return_node.first_child().unwrap();

    assert_eq!(
        syntax.node(expression).unwrap().kind(),
        SyntaxNodeKind::CastExpression
    );
}

#[test]
fn parse_empty_return_statement_still_works() {
    let source = source("fn main(): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let return_statement = body_node.first_child().unwrap();
    let return_node = syntax.node(return_statement).unwrap();

    assert_eq!(return_node.kind(), SyntaxNodeKind::ReturnStatement);
    assert_eq!(source.slice(return_node.span()), Some("return"));
    assert!(return_node.children().is_empty());
}
