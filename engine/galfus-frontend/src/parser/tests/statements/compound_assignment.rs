use super::super::*;
use crate::AssignmentOperatorKind;

#[test]
fn parse_plus_equal_assignment_statement() {
    let source = source("fn main(): null {\n  count += 1\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let assignment = body_node.first_child().unwrap();
    let assignment_node = syntax.node(assignment).unwrap();

    assert_eq!(assignment_node.kind(), SyntaxNodeKind::AssignmentStatement);
    assert_eq!(source.slice(assignment_node.span()), Some("count += 1"));

    let target = assignment_node.first_child().unwrap();
    let operator = assignment_node.child(1).unwrap();
    let value = assignment_node.child(2).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(target).unwrap().span()),
        Some("count")
    );

    assert_eq!(
        syntax.node(operator).unwrap().kind(),
        SyntaxNodeKind::AssignmentOperator
    );
    assert_eq!(
        source.slice(syntax.node(operator).unwrap().span()),
        Some("+=")
    );

    assert_eq!(
        syntax.node(value).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );
    assert_eq!(source.slice(syntax.node(value).unwrap().span()), Some("1"));
}

#[test]
fn parse_member_compound_assignment_statement() {
    let source = source("fn main(): null {\n  user.score += 10\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let assignment = body_node.first_child().unwrap();
    let assignment_node = syntax.node(assignment).unwrap();

    assert_eq!(assignment_node.kind(), SyntaxNodeKind::AssignmentStatement);
    assert_eq!(
        source.slice(assignment_node.span()),
        Some("user.score += 10")
    );

    let target = assignment_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(source.slice(target_node.span()), Some("user.score"));

    let operator = assignment_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(operator).unwrap().span()),
        Some("+=")
    );
}

#[test]
fn parse_all_compound_assignment_operators() {
    let operators = [
        "+=", "-=", "*=", "/=", "%=", "**=", "&=", "|=", "^=", "<<=", ">>=",
    ];

    for operator in operators {
        let text = format!("fn main(): null {{\n  value {operator} 1\n  return\n}}");
        let source = source(&text);

        let result = parse(&source);

        assert!(
            !result.has_errors(),
            "expected `{operator}` to parse without errors"
        );

        let syntax = result.graph().syntax();

        let root = syntax.root().unwrap();
        let function = syntax.node(root).unwrap().first_child().unwrap();
        let function_node = syntax.node(function).unwrap();

        let body = function_node.child(3).unwrap();
        let body_node = syntax.node(body).unwrap();

        let assignment = body_node.first_child().unwrap();
        let assignment_node = syntax.node(assignment).unwrap();

        assert_eq!(
            assignment_node.kind(),
            SyntaxNodeKind::AssignmentStatement,
            "expected `{operator}` to create AssignmentStatement"
        );

        let operator_node = assignment_node.child(1).unwrap();

        assert_eq!(
            source.slice(syntax.node(operator_node).unwrap().span()),
            Some(operator)
        );
    }
}

#[test]
fn parse_compound_assignment_operator_keeps_operator_kind() {
    let source = source(
        "fn main(): null {
            value += 1
            return
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.first_child(root).unwrap();
    let body = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let assignment = syntax.first_child(body).unwrap();
    let assignment_node = syntax.node(assignment).unwrap();

    assert_eq!(assignment_node.kind(), SyntaxNodeKind::AssignmentStatement);

    let operator = syntax
        .first_child_of_kind(assignment, SyntaxNodeKind::AssignmentOperator)
        .unwrap();

    let operator_node = syntax.node(operator).unwrap();

    assert_eq!(
        operator_node.assignment_operator(),
        Some(AssignmentOperatorKind::AddAssign)
    );
}

#[test]
fn parse_compound_assignment_allows_newline_after_operator() {
    let source = source("fn main(): null {\n  count +=\n    1\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let assignment = body_node.first_child().unwrap();
    let assignment_node = syntax.node(assignment).unwrap();

    assert_eq!(assignment_node.kind(), SyntaxNodeKind::AssignmentStatement);
    assert_eq!(
        source.slice(assignment_node.span()),
        Some("count +=\n    1")
    );
}

#[test]
fn parse_compound_assignment_does_not_allow_newline_before_operator() {
    let source = source("fn main(): null {\n  count\n  += 1\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_invalid_compound_assignment_target_reports_error() {
    let source = source("fn main(): null {\n  a + b += 10\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "invalid assignment target `BinaryExpression`")
        .expect("missing invalid assignment target diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0005");
}

#[test]
fn parse_null_fallback_assignment_statement() {
    let source = source(
        "fn main(): null {
            name ??= \"Anon\"
            return
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.first_child(root).unwrap();
    let block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let statement = syntax.first_child(block).unwrap();

    assert_eq!(
        syntax.node(statement).unwrap().kind(),
        SyntaxNodeKind::AssignmentStatement
    );

    let operator = syntax.child(statement, 1).unwrap();

    assert_eq!(
        syntax.node(operator).unwrap().assignment_operator(),
        Some(AssignmentOperatorKind::NullFallbackAssign)
    );
}
