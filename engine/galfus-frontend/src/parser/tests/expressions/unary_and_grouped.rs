use super::super::*;
use crate::UnaryOperatorKind;

#[test]
fn parse_grouped_expression_changes_precedence() {
    let source = source("fn main(): i32 {\n  return (1 + 2) * 3\n}");

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
    assert_eq!(source.slice(expression_node.span()), Some("(1 + 2) * 3"));

    let root_operator = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(root_operator).unwrap().span()),
        Some("*")
    );

    let left = expression_node.first_child().unwrap();
    let left_node = syntax.node(left).unwrap();

    assert_eq!(left_node.kind(), SyntaxNodeKind::GroupedExpression);
    assert_eq!(source.slice(left_node.span()), Some("(1 + 2)"));

    let inner = left_node.first_child().unwrap();
    let inner_node = syntax.node(inner).unwrap();

    assert_eq!(inner_node.kind(), SyntaxNodeKind::BinaryExpression);

    let inner_operator = inner_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(inner_operator).unwrap().span()),
        Some("+")
    );
}

#[test]
fn parse_grouped_expression_allows_internal_newlines() {
    let source = source("fn main(): i32 {\n  return (\n    1 + 2\n  ) * 3\n}");

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

    let left = expression_node.first_child().unwrap();
    let left_node = syntax.node(left).unwrap();

    assert_eq!(left_node.kind(), SyntaxNodeKind::GroupedExpression);
    assert_eq!(source.slice(left_node.span()), Some("(\n    1 + 2\n  )"));
}

#[test]
fn parse_postfix_after_grouped_expression() {
    let source = source("fn main(): [i8] {\n  return (getUser()).name\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("(getUser()).name")
    );

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::GroupedExpression);
}

#[test]
fn parse_empty_grouped_expression_reports_error() {
    let source = source("fn main(): i32 {\n  return ()\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "expected expression, found `RightParen`")
        .expect("missing empty grouped expression diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0006");
}

#[test]
fn parse_unary_minus_expression() {
    let source = source("fn main(): i32 {\n  return -1\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::UnaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("-1"));

    let operator = expression_node.first_child().unwrap();
    let operand = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(operator).unwrap().kind(),
        SyntaxNodeKind::UnaryOperator
    );
    assert_eq!(
        source.slice(syntax.node(operator).unwrap().span()),
        Some("-")
    );

    assert_eq!(
        syntax.node(operand).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );
    assert_eq!(
        source.slice(syntax.node(operand).unwrap().span()),
        Some("1")
    );
}

#[test]
fn parse_logical_not_expression() {
    let source = source("fn main(): bool {\n  return !enabled\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::UnaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("!enabled"));

    let operand = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(operand).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(operand).unwrap().span()),
        Some("enabled")
    );
}

#[test]
fn parse_unary_expression_has_higher_precedence_than_binary() {
    let source = source("fn main(): i32 {\n  return -value * 2\n}");

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
    assert_eq!(source.slice(expression_node.span()), Some("-value * 2"));

    let left = expression_node.first_child().unwrap();
    let operator = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(left).unwrap().kind(),
        SyntaxNodeKind::UnaryExpression
    );
    assert_eq!(
        source.slice(syntax.node(operator).unwrap().span()),
        Some("*")
    );
}

#[test]
fn parse_unary_expression_with_member_operand() {
    let source = source("fn main(): bool {\n  return !user.enabled\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::UnaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("!user.enabled"));

    let operand = expression_node.child(1).unwrap();
    let operand_node = syntax.node(operand).unwrap();

    assert_eq!(operand_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(source.slice(operand_node.span()), Some("user.enabled"));
}

#[test]
fn parse_unary_expression_allows_newline_after_operator() {
    let source = source("fn main(): i32 {\n  return -\n  1\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::UnaryExpression);
    assert_eq!(source.slice(expression_node.span()), Some("-\n  1"));
}

#[test]
fn parse_unary_operator_keeps_operator_kind() {
    let source = source("fn main(): null { var value = !flag; return }");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::UnaryExpression);

    let operator = syntax.child(expression, 0).unwrap();
    let operator_node = syntax.node(operator).unwrap();

    assert_eq!(operator_node.kind(), SyntaxNodeKind::UnaryOperator);
    assert_eq!(operator_node.unary_operator(), Some(UnaryOperatorKind::Not));
}
