use super::super::*;

fn first_instanceof_expression(result: &ParseResult) -> NodeId {
    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();
    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    assert_eq!(statement_node.kind(), SyntaxNodeKind::ExpressionStatement);

    statement_node.first_child().unwrap()
}

#[test]
fn parse_instanceof_expression_with_type_patterns() {
    let source = source(
        "fn main(): i32 {\n  return instanceof value {\n    i32 v => v ** 2,\n    [i8] text => text.length,\n    _ => 0,\n  }\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let expression =
        find_first_of_kind(syntax, root, SyntaxNodeKind::InstanceofExpression).unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::InstanceofExpression);

    let subject = expression_node.first_child().unwrap();
    let arms = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(subject).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );

    let arms_node = syntax.node(arms).unwrap();

    assert_eq!(arms_node.kind(), SyntaxNodeKind::InstanceofArmList);

    assert_eq!(arms_node.child_count(), 3);

    let first_arm = arms_node.first_child().unwrap();
    let first_arm_node = syntax.node(first_arm).unwrap();

    let first_pattern = first_arm_node.first_child().unwrap();
    let first_pattern_node = syntax.node(first_pattern).unwrap();

    assert_eq!(first_pattern_node.kind(), SyntaxNodeKind::TypePattern);
    assert_eq!(source.slice(first_pattern_node.span()), Some("i32 v"));

    let body = syntax
        .node(first_arm_node.child(1).unwrap())
        .unwrap()
        .first_child()
        .unwrap();
    assert_eq!(
        syntax.node(body).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
}

#[test]
fn parse_instanceof_wildcard_pattern() {
    let source =
        source("fn main(): i32 {\n  instanceof value {\n    _ {\n      return 0\n    }\n  }\n}");

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();

    let expression = first_instanceof_expression(&result);
    let expression_node = syntax.node(expression).unwrap();

    let arms = expression_node.child(1).unwrap();
    let arm = syntax.node(arms).unwrap().first_child().unwrap();
    let arm_node = syntax.node(arm).unwrap();

    let pattern = arm_node.first_child().unwrap();
    let pattern_node = syntax.node(pattern).unwrap();

    assert_eq!(pattern_node.kind(), SyntaxNodeKind::WildcardPattern);
    assert_eq!(source.slice(pattern_node.span()), Some("_"));
    assert_eq!(pattern_node.child_count(), 0);
}

#[test]
fn parse_instanceof_expression_with_array_type_pattern() {
    let source = source(
        "fn main(value: [u8] | null): null {\n  instanceof value {\n    [u8] name {\n      return\n    },\n    _ {\n      return\n    }\n  }\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let graph = result.graph();
    let syntax = graph.syntax();
    let root = syntax.root().unwrap();

    assert!(find_first_of_kind(syntax, root, SyntaxNodeKind::InstanceofExpression).is_some());
}

#[test]
fn parse_instanceof_expression_with_null_pattern_and_expression_arms() {
    let source = source(
        "fn main(value: [u8] | i32 | null): null {\n  instanceof value {\n    [u8] name => log(name),\n    i32 count => log(count),\n    null => log(\"missing\"),\n  }\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let expression =
        find_first_of_kind(syntax, root, SyntaxNodeKind::InstanceofExpression).unwrap();
    let expression_node = syntax.node(expression).unwrap();
    let arms = expression_node.child(1).unwrap();
    let arms_node = syntax.node(arms).unwrap();

    assert_eq!(arms_node.child_count(), 3);

    let last_arm = arms_node.child(2).unwrap();
    let last_arm_node = syntax.node(last_arm).unwrap();
    let pattern = last_arm_node.first_child().unwrap();
    let body = syntax
        .node(last_arm_node.child(1).unwrap())
        .unwrap()
        .first_child()
        .unwrap();

    assert_eq!(
        syntax.node(pattern).unwrap().kind(),
        SyntaxNodeKind::TypePattern
    );
    assert_eq!(
        source.slice(syntax.node(pattern).unwrap().span()),
        Some("null")
    );
    assert_eq!(
        syntax.node(body).unwrap().kind(),
        SyntaxNodeKind::CallExpression
    );
}
