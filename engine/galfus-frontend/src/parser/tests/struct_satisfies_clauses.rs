use super::*;

#[test]
fn parse_struct_satisfies_single_constraint() {
    let source = source("struct User satisfies Identifiable {\n  id: i64,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    assert_eq!(struct_node.kind(), SyntaxNodeKind::StructItem);
    assert_eq!(struct_node.child_count(), 3);

    let name = struct_node.first_child().unwrap();
    let satisfies = struct_node.child(1).unwrap();
    let fields = struct_node.child(2).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("User")
    );

    let satisfies_node = syntax.node(satisfies).unwrap();

    assert_eq!(satisfies_node.kind(), SyntaxNodeKind::SatisfiesClause);

    assert_eq!(
        source.slice(satisfies_node.span()),
        Some("satisfies Identifiable")
    );

    assert_eq!(satisfies_node.child_count(), 1);

    let constraint = satisfies_node.first_child().unwrap();

    assert_eq!(
        syntax.node(constraint).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );

    assert_eq!(
        source.slice(syntax.node(constraint).unwrap().span()),
        Some("Identifiable")
    );

    assert_eq!(
        syntax.node(fields).unwrap().kind(),
        SyntaxNodeKind::StructFieldList
    );
}

#[test]
fn parse_struct_satisfies_multiple_constraints() {
    let source =
        source("struct User satisfies Identifiable, Stringable {\n  id: i64,\n  name: [i8],\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    let satisfies = struct_node.child(1).unwrap();
    let satisfies_node = syntax.node(satisfies).unwrap();

    assert_eq!(satisfies_node.kind(), SyntaxNodeKind::SatisfiesClause);
    assert_eq!(satisfies_node.child_count(), 2);

    assert_eq!(
        source.slice(satisfies_node.span()),
        Some("satisfies Identifiable, Stringable")
    );

    let first = satisfies_node.first_child().unwrap();
    let second = satisfies_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("Identifiable")
    );

    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("Stringable")
    );
}

#[test]
fn parse_struct_satisfies_generic_constraint() {
    let source = source(
        "struct Range satisfies Iterable<i32, RangeIterator> {\n  start: i32,\n  end: i32,\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    let satisfies = struct_node.child(1).unwrap();
    let satisfies_node = syntax.node(satisfies).unwrap();

    assert_eq!(satisfies_node.kind(), SyntaxNodeKind::SatisfiesClause);
    assert_eq!(satisfies_node.child_count(), 1);

    let constraint = satisfies_node.first_child().unwrap();
    let constraint_node = syntax.node(constraint).unwrap();

    assert_eq!(constraint_node.kind(), SyntaxNodeKind::GenericType);

    assert_eq!(
        source.slice(constraint_node.span()),
        Some("Iterable<i32, RangeIterator>")
    );
}

#[test]
fn parse_generic_struct_with_satisfies_clause() {
    let source = source("struct Box<T> satisfies Container<T> {\n  value: T,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    assert_eq!(struct_node.child_count(), 4);

    let name = struct_node.first_child().unwrap();
    let generics = struct_node.child(1).unwrap();
    let satisfies = struct_node.child(2).unwrap();
    let fields = struct_node.child(3).unwrap();

    assert_eq!(source.slice(syntax.node(name).unwrap().span()), Some("Box"));

    assert_eq!(
        syntax.node(generics).unwrap().kind(),
        SyntaxNodeKind::GenericParameterList
    );

    assert_eq!(
        syntax.node(satisfies).unwrap().kind(),
        SyntaxNodeKind::SatisfiesClause
    );

    assert_eq!(
        syntax.node(fields).unwrap().kind(),
        SyntaxNodeKind::StructFieldList
    );
}

#[test]
fn parse_struct_without_satisfies_shape_is_unchanged() {
    let source = source("struct User {\n  name: [i8],\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    assert_eq!(struct_node.child_count(), 2);

    assert_eq!(
        syntax.node(struct_node.child(1).unwrap()).unwrap().kind(),
        SyntaxNodeKind::StructFieldList
    );
}
