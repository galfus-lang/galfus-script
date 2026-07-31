use super::super::*;

#[test]
fn parse_generic_anchor_call_expression() {
    let source = source("fn main(): null {\n  var buffer = buffer::array<i32>(size)\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let expression = syntax.node(initializer).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("buffer::array<i32>(size)")
    );

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::GenericExpression);

    let generic_target = target_node.first_child().unwrap();
    let generic_args = target_node.child(1).unwrap();

    assert_eq!(
        syntax.node(generic_target).unwrap().kind(),
        SyntaxNodeKind::PathExpression
    );

    assert_eq!(
        syntax.node(generic_args).unwrap().kind(),
        SyntaxNodeKind::GenericArgumentList
    );
}

#[test]
fn parse_generic_member_call_expression() {
    let source = source("fn main(): null {\n  registry.get<[i8]>(\"name\")\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let expression = syntax.node(statement).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::GenericExpression);
    assert_eq!(source.slice(target_node.span()), Some("registry.get<[i8]>"));
}

#[test]
fn parse_generic_call_with_multiple_arguments() {
    let source = source("fn main(): null {\n  makePair<[i8], User>(name, user)\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let expression = syntax.node(statement).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    let generic_args = target_node.child(1).unwrap();
    let generic_args_node = syntax.node(generic_args).unwrap();

    assert_eq!(generic_args_node.child_count(), 2);
    assert_eq!(source.slice(generic_args_node.span()), Some("<[i8], User>"));
}

#[test]
fn parse_generic_call_with_nested_generic_argument() {
    let source = source("fn main(): null {\n  makeBox<Map<[i8], User>>(value)\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let expression = syntax.node(statement).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("makeBox<Map<[i8], User>>(value)")
    );

    let target = expression_node.first_child().unwrap();
    let generic_args = syntax.node(target).unwrap().child(1).unwrap();
    let first_arg = syntax.node(generic_args).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(first_arg).unwrap().kind(),
        SyntaxNodeKind::GenericType
    );
}

#[test]
fn parse_less_than_expression_still_works() {
    let source = source("fn main(): null {\n  const ok = a < b\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let initializer = syntax.node(statement).unwrap().child(1).unwrap();
    let expression = syntax.node(initializer).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(expression).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
}

#[test]
fn parse_generic_expression_without_call_is_not_generic_expression() {
    let source = source("fn main(): null {\n  const ok = makeBox<User>\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());
}
