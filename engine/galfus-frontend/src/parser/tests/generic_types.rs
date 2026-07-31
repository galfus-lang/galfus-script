use super::*;

#[test]
fn parse_generic_type() {
    let source = source("struct Scene {\n  nodes: WeakVec<Node>,\n}");

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
    let field_type_node = syntax.node(field_type).unwrap();

    assert_eq!(field_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(source.slice(field_type_node.span()), Some("WeakVec<Node>"));

    let base = field_type_node.first_child().unwrap();
    let arguments = field_type_node.child(1).unwrap();

    assert_eq!(syntax.node(base).unwrap().kind(), SyntaxNodeKind::NamedType);
    assert_eq!(
        source.slice(syntax.node(base).unwrap().span()),
        Some("WeakVec")
    );

    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.kind(), SyntaxNodeKind::TypeArgumentList);
    assert_eq!(arguments_node.child_count(), 1);
}

#[test]
fn parse_generic_type_with_multiple_arguments() {
    let source = source("struct Registry {\n  users: Map<[i8], User>,\n}");

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
    let field_type_node = syntax.node(field_type).unwrap();

    assert_eq!(field_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(
        source.slice(field_type_node.span()),
        Some("Map<[i8], User>")
    );

    let arguments = field_type_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.child_count(), 2);

    let first = arguments_node.first_child().unwrap();
    let second = arguments_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("[i8]")
    );
    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("User")
    );
}

#[test]
fn parse_nested_generic_type() {
    let source = source("struct Registry {\n  users: Map<[i8], WeakVec<User>>,\n}");

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
    let field_type_node = syntax.node(field_type).unwrap();

    assert_eq!(field_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(
        source.slice(field_type_node.span()),
        Some("Map<[i8], WeakVec<User>>")
    );

    let arguments = field_type_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    let nested = arguments_node.child(1).unwrap();

    assert_eq!(
        syntax.node(nested).unwrap().kind(),
        SyntaxNodeKind::GenericType
    );

    assert_eq!(
        source.slice(syntax.node(nested).unwrap().span()),
        Some("WeakVec<User>")
    );
}

#[test]
fn parse_generic_type_with_union_argument() {
    let source = source("struct MaybeUsers {\n  users: Box<User | null>,\n}");

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
    let field_type_node = syntax.node(field_type).unwrap();

    assert_eq!(field_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(
        source.slice(field_type_node.span()),
        Some("Box<User | null>")
    );

    let arguments = field_type_node.child(1).unwrap();
    let first_argument = syntax.node(arguments).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(first_argument).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );
}

#[test]
fn parse_generic_type_with_array_argument() {
    let source = source("struct BufferBox {\n  buffer: Box<[i32]>,\n}");

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
    let field_type_node = syntax.node(field_type).unwrap();

    assert_eq!(field_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(source.slice(field_type_node.span()), Some("Box<[i32]>"));

    let arguments = field_type_node.child(1).unwrap();
    let first_argument = syntax.node(arguments).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(first_argument).unwrap().kind(),
        SyntaxNodeKind::ArrayType
    );
}

#[test]
fn parse_generic_type_with_trailing_comma() {
    let source = source("struct Registry {\n  users: Map<[i8], User,>,\n}");

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
    let field_type_node = syntax.node(field_type).unwrap();

    let arguments = field_type_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.child_count(), 2);
}

#[test]
fn parse_generic_type_with_newlines() {
    let source = source("struct Registry {\n  users: Map<\n    [i8],\n    User,\n  >,\n}");

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

    assert_eq!(
        syntax.node(field_type).unwrap().kind(),
        SyntaxNodeKind::GenericType
    );
}
