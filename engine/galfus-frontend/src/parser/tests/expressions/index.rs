use super::super::*;

#[test]
fn parse_index_expression_in_return() {
    let source = source("fn main(): i32 {\n  return items[0]\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::IndexExpression);
    assert_eq!(source.slice(expression_node.span()), Some("items[0]"));

    let target = expression_node.first_child().unwrap();
    let index = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(target).unwrap().span()),
        Some("items")
    );

    assert_eq!(
        syntax.node(index).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );
    assert_eq!(source.slice(syntax.node(index).unwrap().span()), Some("0"));
}

#[test]
fn parse_index_expression_with_binary_index() {
    let source = source("fn main(): i32 {\n  return items[index + 1]\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::IndexExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("items[index + 1]")
    );

    let index = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(index).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
}

#[test]
fn parse_chained_index_expression() {
    let source = source("fn main(): i32 {\n  return grid[x][y]\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::IndexExpression);
    assert_eq!(source.slice(expression_node.span()), Some("grid[x][y]"));

    let target = expression_node.first_child().unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::IndexExpression
    );

    assert_eq!(
        source.slice(syntax.node(target).unwrap().span()),
        Some("grid[x]")
    );
}

#[test]
fn parse_member_after_index_expression() {
    let source = source("fn main(): [i8] {\n  return users[0].name\n}");

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
    assert_eq!(source.slice(expression_node.span()), Some("users[0].name"));

    let target = expression_node.first_child().unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::IndexExpression
    );
}

#[test]
fn parse_index_assignment_statement() {
    let source = source("fn main(): null {\n  items[index] = value\n  return\n}");

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
        Some("items[index] = value")
    );

    let target = assignment_node.first_child().unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::IndexExpression
    );
}

#[test]
fn parse_index_compound_assignment_statement() {
    let source = source("fn main(): null {\n  items[index] += 1\n  return\n}");

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
        Some("items[index] += 1")
    );

    let target = assignment_node.first_child().unwrap();
    let operator = assignment_node.child(1).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::IndexExpression
    );

    assert_eq!(
        source.slice(syntax.node(operator).unwrap().span()),
        Some("+=")
    );
}

#[test]
fn parse_index_expression_allows_newline_before_left_bracket() {
    let source = source("fn main(): i32 {\n  return items\n  [index]\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::IndexExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("items\n  [index]")
    );
}

#[test]
fn parse_index_expression_allows_internal_newlines() {
    let source = source("fn main(): i32 {\n  return items[\n    index + 1\n  ]\n}");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::IndexExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("items[\n    index + 1\n  ]")
    );
}
