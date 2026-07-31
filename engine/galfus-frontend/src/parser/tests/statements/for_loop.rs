use super::super::*;

#[test]
fn parse_for_statement() {
    let source =
        source("fn main(): null {\n  for item in items {\n    print(item)\n  }\n  return\n}");

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
    assert_eq!(for_node.child_count(), 3);

    let binding = for_node.first_child().unwrap();
    let iterable = for_node.child(1).unwrap();
    let loop_body = for_node.child(2).unwrap();

    assert_eq!(
        syntax.node(binding).unwrap().kind(),
        SyntaxNodeKind::ForBinding
    );
    assert_eq!(
        source.slice(syntax.node(binding).unwrap().span()),
        Some("item")
    );

    assert_eq!(
        syntax.node(iterable).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(iterable).unwrap().span()),
        Some("items")
    );

    assert_eq!(
        syntax.node(loop_body).unwrap().kind(),
        SyntaxNodeKind::Block
    );
}

#[test]
fn parse_for_statement_with_call_iterable() {
    let source =
        source("fn main(): null {\n  for item in getItems() {\n    print(item)\n  }\n  return\n}");

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

    let iterable = for_node.child(1).unwrap();

    assert_eq!(
        syntax.node(iterable).unwrap().kind(),
        SyntaxNodeKind::CallExpression
    );

    assert_eq!(
        source.slice(syntax.node(iterable).unwrap().span()),
        Some("getItems()")
    );
}

#[test]
fn parse_for_statement_with_member_index_iterable() {
    let source = source(
        "fn main(): null {\n  for item in user.groups[0] {\n    print(item)\n  }\n  return\n}",
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

    let iterable = for_node.child(1).unwrap();
    let iterable_node = syntax.node(iterable).unwrap();

    assert_eq!(iterable_node.kind(), SyntaxNodeKind::IndexExpression);
    assert_eq!(source.slice(iterable_node.span()), Some("user.groups[0]"));
}

#[test]
fn parse_for_statement_allows_newlines_between_parts() {
    let source = source(
        "fn main(): null {\n  for\n    item\n    in\n    items\n  {\n    print(item)\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());
}

#[test]
fn parse_for_iterable_identifier_does_not_become_struct_literal() {
    let source =
        source("fn main(): null {\n  for item in items {\n    print(item)\n  }\n  return\n}");

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

    let iterable = for_node.child(1).unwrap();
    let iterable_node = syntax.node(iterable).unwrap();

    assert_eq!(iterable_node.kind(), SyntaxNodeKind::NameExpression);
    assert_eq!(source.slice(iterable_node.span()), Some("items"));
}

#[test]
fn parse_for_iterable_allows_struct_literal_inside_call_argument() {
    let source = source(
        "fn main(): null {\n  for item in getItems(new(Filter) { active: true }) {\n    print(item)\n  }\n  return\n}",
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

    let iterable = for_node.child(1).unwrap();
    let iterable_node = syntax.node(iterable).unwrap();

    assert_eq!(iterable_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(
        source.slice(iterable_node.span()),
        Some("getItems(new(Filter) { active: true })")
    );

    let arguments = iterable_node.child(1).unwrap();
    let argument = syntax.node(arguments).unwrap().first_child().unwrap();
    let argument_node = syntax.node(argument).unwrap();

    let value = argument_node.first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::StructLiteral);
    assert_eq!(
        source.slice(value_node.span()),
        Some("new(Filter) { active: true }")
    );
}

#[test]
fn parse_for_statement_requires_in_keyword() {
    let source = source("fn main(): null {\n  for item items {\n    print(item)\n  }\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "expected `In`, found `Identifier`")
        .expect("missing expected in diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}

#[test]
fn parse_for_statement_with_ignored_binding() {
    let source = source("fn main(): null {\n for _ in items {\n print(\"x\")\n }\n return\n}");

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();
    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();
    let for_statement = body_node.first_child().unwrap();
    let for_node = syntax.node(for_statement).unwrap();

    assert_eq!(for_node.kind(), SyntaxNodeKind::ForStatement);

    let binding = for_node.first_child().unwrap();
    let binding_node = syntax.node(binding).unwrap();

    assert_eq!(binding_node.kind(), SyntaxNodeKind::ForBinding);
    assert_eq!(binding_node.child_count(), 1);
    assert_eq!(source.slice(binding_node.span()), Some("_"));

    let wildcard = binding_node.first_child().unwrap();
    assert_eq!(
        syntax.node(wildcard).unwrap().kind(),
        SyntaxNodeKind::WildcardPattern
    );
}

#[test]
fn parse_for_statement_with_index_binding() {
    let source =
        source("fn main(): null {\n for value, index in items {\n print(value)\n }\n return\n}");

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();
    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();
    let for_statement = body_node.first_child().unwrap();
    let for_node = syntax.node(for_statement).unwrap();

    assert_eq!(for_node.kind(), SyntaxNodeKind::ForStatement);

    let binding = for_node.first_child().unwrap();
    let binding_node = syntax.node(binding).unwrap();

    assert_eq!(binding_node.kind(), SyntaxNodeKind::ForBinding);
    assert_eq!(binding_node.child_count(), 2);
    assert_eq!(source.slice(binding_node.span()), Some("value, index"));
}
