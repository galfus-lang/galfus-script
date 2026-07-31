use super::super::*;

#[test]
fn parse_name_assignment_statement() {
    let source = source("fn main(): null {\n  var name = \"Ana\"\n  name = \"Bia\"\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let assignment = body_node.child(1).unwrap();
    let assignment_node = syntax.node(assignment).unwrap();

    assert_eq!(assignment_node.kind(), SyntaxNodeKind::AssignmentStatement);
    assert_eq!(source.slice(assignment_node.span()), Some("name = \"Bia\""));
    assert_eq!(assignment_node.child_count(), 3);

    let target = assignment_node.first_child().unwrap();
    let operator = assignment_node.child(1).unwrap();
    let value = assignment_node.child(2).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(target).unwrap().span()),
        Some("name")
    );

    assert_eq!(
        syntax.node(operator).unwrap().kind(),
        SyntaxNodeKind::AssignmentOperator
    );
    assert_eq!(
        source.slice(syntax.node(operator).unwrap().span()),
        Some("=")
    );

    assert_eq!(
        syntax.node(value).unwrap().kind(),
        SyntaxNodeKind::StringLiteral
    );
    assert_eq!(
        source.slice(syntax.node(value).unwrap().span()),
        Some("\"Bia\"")
    );
}

#[test]
fn parse_member_assignment_statement() {
    let source = source("fn main(): null {\n  user.name = \"Ana\"\n  return\n}");

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
        Some("user.name = \"Ana\"")
    );

    let target = assignment_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(source.slice(target_node.span()), Some("user.name"));
}

#[test]
fn parse_assignment_value_can_be_binary_expression() {
    let source = source("fn main(): null {\n  count = count + 1\n  return\n}");

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

    let value = assignment_node.child(2).unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::BinaryExpression);
    assert_eq!(source.slice(value_node.span()), Some("count + 1"));
}

#[test]
fn parse_assignment_allows_newline_after_equal() {
    let source = source("fn main(): null {\n  name =\n    \"Ana\"\n  return\n}");

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
        Some("name =\n    \"Ana\"")
    );
}

#[test]
fn parse_invalid_assignment_target_reports_error() {
    let source = source("fn main(): null {\n  a + b = 10\n  return\n}");

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
fn parse_assignment_requires_statement_terminator() {
    let source = source("fn main(): null {\n  name = \"Ana\" print(name)\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.message() == "expected statement terminator, found `Identifier`"
        })
        .expect("missing statement terminator diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}
