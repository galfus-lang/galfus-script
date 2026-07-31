use super::*;

#[test]
fn parse_type_alias_type_path() {
    let source = source("type TextureAlias = game::Texture");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let alias_type = alias_node.child(1).unwrap();
    let alias_type_node = syntax.node(alias_type).unwrap();

    assert_eq!(alias_type_node.kind(), SyntaxNodeKind::Path);
    assert_eq!(source.slice(alias_type_node.span()), Some("game::Texture"));

    assert_eq!(alias_type_node.child_count(), 2);
}

#[test]
fn parse_type_alias_generic_type_path() {
    let source = source("type UserResult = std::Result<User, Error>");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let alias_type = alias_node.child(1).unwrap();
    let alias_type_node = syntax.node(alias_type).unwrap();

    assert_eq!(alias_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(
        source.slice(alias_type_node.span()),
        Some("std::Result<User, Error>")
    );

    let base = alias_type_node.first_child().unwrap();

    assert_eq!(syntax.node(base).unwrap().kind(), SyntaxNodeKind::Path);

    assert_eq!(
        source.slice(syntax.node(base).unwrap().span()),
        Some("std::Result")
    );
}

#[test]
fn parse_struct_field_type_path() {
    let source = source("struct Asset {\n  texture: game::Texture,\n}");

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

    assert_eq!(field_type_node.kind(), SyntaxNodeKind::Path);
    assert_eq!(source.slice(field_type_node.span()), Some("game::Texture"));
}

#[test]
fn parse_simple_anchored_function_still_works_with_type_path() {
    let source = source("fn User::rename(self): User {\n  return self\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let anchor = function_node.first_child().unwrap();
    let name = function_node.child(1).unwrap();

    assert_eq!(
        syntax.node(anchor).unwrap().kind(),
        SyntaxNodeKind::FunctionAnchor
    );

    assert_eq!(
        source.slice(syntax.node(anchor).unwrap().span()),
        Some("User")
    );

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("rename")
    );
}

#[test]
fn parse_type_path_anchored_function() {
    let source = source("fn game::Texture::load(self): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let anchor = function_node.first_child().unwrap();
    let name = function_node.child(1).unwrap();

    let anchor_node = syntax.node(anchor).unwrap();

    assert_eq!(anchor_node.kind(), SyntaxNodeKind::FunctionAnchor);
    assert_eq!(source.slice(anchor_node.span()), Some("game::Texture"));

    let anchor_type = anchor_node.first_child().unwrap();

    assert_eq!(
        syntax.node(anchor_type).unwrap().kind(),
        SyntaxNodeKind::Path
    );

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("load")
    );
}

#[test]
fn parse_generic_type_path_anchored_function() {
    let source = source("fn std::Box<T>::unwrap(self): T {\n  return self.value\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let anchor = function_node.first_child().unwrap();
    let anchor_node = syntax.node(anchor).unwrap();

    assert_eq!(anchor_node.kind(), SyntaxNodeKind::FunctionAnchor);
    assert_eq!(source.slice(anchor_node.span()), Some("std::Box<T>"));

    let anchor_type = anchor_node.first_child().unwrap();

    assert_eq!(
        syntax.node(anchor_type).unwrap().kind(),
        SyntaxNodeKind::GenericType
    );

    let generic_base = syntax.node(anchor_type).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(generic_base).unwrap().kind(),
        SyntaxNodeKind::Path
    );
}
