use super::*;

#[test]
fn parse_struct_field_default() {
    let source = source("struct Person {\n  name: [i8] = \"Anonymous\",\n  age: u32 = 0,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    assert_eq!(struct_node.kind(), SyntaxNodeKind::StructItem);

    let fields = struct_node.child(1).unwrap();
    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.child_count(), 2);

    let first_field = fields_node.first_child().unwrap();
    let first_field_node = syntax.node(first_field).unwrap();

    assert_eq!(first_field_node.kind(), SyntaxNodeKind::StructField);
    assert_eq!(
        source.slice(first_field_node.span()),
        Some("name: [i8] = \"Anonymous\"")
    );

    assert_eq!(first_field_node.child_count(), 3);

    let default = first_field_node.child(2).unwrap();
    let default_node = syntax.node(default).unwrap();

    assert_eq!(default_node.kind(), SyntaxNodeKind::StructFieldDefault);
    assert_eq!(source.slice(default_node.span()), Some("= \"Anonymous\""));
}

#[test]
fn parse_struct_field_default_with_union_null() {
    let source = source("struct Person {\n  email: [i8] | null = null,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    let fields = struct_node.child(1).unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    let field_type = field_node.child(1).unwrap();
    let default = field_node.child(2).unwrap();

    assert_eq!(
        syntax.node(field_type).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );

    assert_eq!(
        syntax.node(default).unwrap().kind(),
        SyntaxNodeKind::StructFieldDefault
    );

    assert_eq!(
        source.slice(field_node.span()),
        Some("email: [i8] | null = null")
    );
}

#[test]
fn parse_const_struct_field() {
    let source = source("struct User {\n  const id: i64,\n  name: [u8],\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();
    let fields = struct_node.child(1).unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(field_node.kind(), SyntaxNodeKind::StructField);
    assert_eq!(source.slice(field_node.span()), Some("const id: i64"));
    assert_eq!(field_node.child_count(), 3);

    let marker = field_node.first_child().unwrap();
    let name = field_node.child(1).unwrap();
    let field_type = field_node.child(2).unwrap();

    assert_eq!(
        syntax.node(marker).unwrap().kind(),
        SyntaxNodeKind::StructFieldConst
    );
    assert_eq!(source.slice(syntax.node(name).unwrap().span()), Some("id"));
    assert_eq!(
        syntax.node(field_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );
}

#[test]
fn parse_regular_struct_field_still_uses_struct_field() {
    let source = source("struct User {\n  name: [i8] = \"Anonymous\",\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    let fields = struct_node.child(1).unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(field_node.kind(), SyntaxNodeKind::StructField);
    assert_eq!(field_node.child_count(), 3);

    let name = field_node.first_child().unwrap();
    let field_type = field_node.child(1).unwrap();
    let default = field_node.child(2).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("name")
    );

    assert_eq!(
        syntax.node(field_type).unwrap().kind(),
        SyntaxNodeKind::ArrayType
    );

    assert_eq!(
        syntax.node(default).unwrap().kind(),
        SyntaxNodeKind::StructFieldDefault
    );
}

#[test]
fn parse_weak_struct_field() {
    let source = source("struct CacheEntry {\n  weak resource: Resource | null = null,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    assert_eq!(struct_node.kind(), SyntaxNodeKind::StructItem);

    let fields = struct_node.child(1).unwrap();
    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.child_count(), 1);

    let field = fields_node.first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(field_node.kind(), SyntaxNodeKind::WeakStructField);
    assert_eq!(
        source.slice(field_node.span()),
        Some("weak resource: Resource | null = null")
    );

    assert_eq!(field_node.child_count(), 3);

    let name = field_node.first_child().unwrap();
    let field_type = field_node.child(1).unwrap();
    let default = field_node.child(2).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("resource")
    );

    assert_eq!(
        syntax.node(field_type).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );

    assert_eq!(
        syntax.node(default).unwrap().kind(),
        SyntaxNodeKind::StructFieldDefault
    );
}

#[test]
fn parse_weak_struct_field_without_default() {
    let source = source("struct CacheEntry {\n  weak resource: Resource | null,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    let fields = struct_node.child(1).unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(field_node.kind(), SyntaxNodeKind::WeakStructField);
    assert_eq!(field_node.child_count(), 2);

    let field_type = field_node.child(1).unwrap();

    assert_eq!(
        syntax.node(field_type).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );
}

#[test]
fn parse_weak_struct_field_non_nullable_is_syntax_valid() {
    let source = source("struct CacheEntry {\n  weak resource: Resource,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let struct_item = syntax.node(root).unwrap().first_child().unwrap();
    let struct_node = syntax.node(struct_item).unwrap();

    let fields = struct_node.child(1).unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(field_node.kind(), SyntaxNodeKind::WeakStructField);
    assert_eq!(
        source.slice(field_node.span()),
        Some("weak resource: Resource")
    );
}
