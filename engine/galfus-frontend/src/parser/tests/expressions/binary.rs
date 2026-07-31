use super::super::*;
use crate::BinaryOperatorKind;

#[test]
fn parse_binary_addition_expression() {
    let source = source("fn main(): i32 {\n  return 1 + 2\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("1 + 2"));

    let left = expression_node.first_child().unwrap();
    let operator = expression_node.child(1).unwrap();
    let right = expression_node.child(2).unwrap();

    assert_eq!(
        syntax.node(left).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );
    assert_eq!(source.slice(syntax.node(left).unwrap().span()), Some("1"));

    assert_eq!(
        syntax.node(operator).unwrap().kind(),
        SyntaxNodeKind::BinaryOperator
    );
    assert_eq!(
        source.slice(syntax.node(operator).unwrap().span()),
        Some("+")
    );

    assert_eq!(
        syntax.node(right).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );
    assert_eq!(source.slice(syntax.node(right).unwrap().span()), Some("2"));
}

#[test]
fn parse_binary_expression_respects_precedence() {
    let source = source("fn main(): i32 {\n  return 1 + 2 * 3\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("1 + 2 * 3"));

    let root_operator = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(root_operator).unwrap().span()),
        Some("+")
    );

    let right = expression_node.child(2).unwrap();
    let right_node = syntax.node(right).unwrap();

    assert_eq!(right_node.kind(), SyntaxNodeKind::BinaryExpression);

    let right_operator = right_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(right_operator).unwrap().span()),
        Some("*")
    );
}

#[test]
fn parse_power_expression_is_right_associative() {
    let source = source("fn main(): i32 {\n  return 2 ** 3 ** 4\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("2 ** 3 ** 4"));

    let root_operator = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(root_operator).unwrap().span()),
        Some("**")
    );

    let right = expression_node.child(2).unwrap();
    let right_node = syntax.node(right).unwrap();

    assert_eq!(right_node.kind(), SyntaxNodeKind::BinaryExpression);

    let right_operator = right_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(right_operator).unwrap().span()),
        Some("**")
    );
}

#[test]
fn parse_binary_expression_allows_newline_before_operator() {
    let source = source("fn main(): i32 {\n  return 1\n  + 2\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("1\n  + 2"));
}

#[test]
fn parse_binary_expression_allows_newline_after_operator() {
    let source = source("fn main(): i32 {\n  return 1 +\n  2\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("1 +\n  2"));
}

#[test]
fn parse_comparison_expression_after_arithmetic() {
    let source = source("fn main(): bool {\n  return 1 + 2 > 3\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("1 + 2 > 3"));

    let root_operator = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(root_operator).unwrap().span()),
        Some(">")
    );

    let left = expression_node.first_child().unwrap();
    let left_node = syntax.node(left).unwrap();

    assert_eq!(left_node.kind(), SyntaxNodeKind::BinaryExpression);

    let left_operator = left_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(left_operator).unwrap().span()),
        Some("+")
    );
}

#[test]
fn parse_logical_and_has_higher_precedence_than_or() {
    let source = source("fn main(): bool {\n  return a || b && c\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("a || b && c"));

    let root_operator = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(root_operator).unwrap().span()),
        Some("||")
    );

    let right = expression_node.child(2).unwrap();
    let right_node = syntax.node(right).unwrap();

    assert_eq!(right_node.kind(), SyntaxNodeKind::BinaryExpression);

    let right_operator = right_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(right_operator).unwrap().span()),
        Some("&&")
    );
}

#[test]
fn parse_equality_has_higher_precedence_than_logical_and() {
    let source = source("fn main(): bool {\n  return a == b && c != d\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("a == b && c != d")
    );

    let root_operator = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(root_operator).unwrap().span()),
        Some("&&")
    );

    let left = expression_node.first_child().unwrap();
    let right = expression_node.child(2).unwrap();

    assert_eq!(
        syntax.node(left).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
    assert_eq!(
        syntax.node(right).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
}

#[test]
fn parse_null_coalescing_is_right_associative() {
    let source = source("fn main(): User {\n  return a ?? b ?? c\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("a ?? b ?? c"));

    let root_operator = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(root_operator).unwrap().span()),
        Some("??")
    );

    let right = expression_node.child(2).unwrap();
    let right_node = syntax.node(right).unwrap();

    assert_eq!(right_node.kind(), SyntaxNodeKind::BinaryExpression);

    let right_operator = right_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(right_operator).unwrap().span()),
        Some("??")
    );
}

#[test]
fn parse_logical_expression_allows_newline_before_operator() {
    let source = source("fn main(): bool {\n  return a\n  && b\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("a\n  && b"));
}

#[test]
fn parse_logical_expression_allows_newline_after_operator() {
    let source = source("fn main(): bool {\n  return a &&\n  b\n}");

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

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("a &&\n  b"));
}

#[test]
fn parse_binary_operator_keeps_operator_kind() {
    let source = source("fn main(): null { var value = 1 + 2; return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.first_child(root).unwrap();
    let body = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let var_statement = syntax.first_child(body).unwrap();
    let initializer = syntax
        .first_child_of_kind(var_statement, SyntaxNodeKind::Initializer)
        .unwrap();

    let expression = syntax.first_child(initializer).unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);

    let operator = syntax.child(expression, 1).unwrap();
    let operator_node = syntax.node(operator).unwrap();

    assert_eq!(operator_node.kind(), SyntaxNodeKind::BinaryOperator);
    assert_eq!(
        operator_node.binary_operator(),
        Some(BinaryOperatorKind::Add)
    );
}
