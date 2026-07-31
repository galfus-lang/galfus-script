use super::*;

#[test]
fn parse_simple_var_binding_as_binding_pattern() {
    let source = source("var name = value");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();
    let var_node = syntax.node(var_item).unwrap();

    assert_eq!(var_node.kind(), SyntaxNodeKind::VarItem);

    let binding = syntax.child(var_item, 0).unwrap();
    let binding_node = syntax.node(binding).unwrap();

    assert_eq!(binding_node.kind(), SyntaxNodeKind::BindingPattern);

    let inner = syntax.first_child(binding).unwrap();

    assert_eq!(
        syntax.node(inner).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(inner).unwrap().span()),
        Some("name")
    );
}

#[test]
fn parse_struct_destructuring_binding() {
    let source = source("var { id, name } = user");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();
    let binding = syntax.child(var_item, 0).unwrap();
    let inner = syntax.first_child(binding).unwrap();

    assert_eq!(
        syntax.node(inner).unwrap().kind(),
        SyntaxNodeKind::StructBindingPattern
    );

    assert_eq!(syntax.node(inner).unwrap().child_count(), 2);

    let first_field = syntax.child(inner, 0).unwrap();
    let second_field = syntax.child(inner, 1).unwrap();

    assert_eq!(
        syntax.node(first_field).unwrap().kind(),
        SyntaxNodeKind::StructBindingField
    );
    assert_eq!(
        syntax.node(second_field).unwrap().kind(),
        SyntaxNodeKind::StructBindingField
    );
}

#[test]
fn parse_struct_destructuring_alias_binding() {
    let source = source("var { name: userName } = user");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();
    let binding = syntax.child(var_item, 0).unwrap();
    let pattern = syntax.first_child(binding).unwrap();

    let field = syntax.first_child(pattern).unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(field_node.kind(), SyntaxNodeKind::StructBindingField);
    assert_eq!(field_node.child_count(), 2);

    let name = syntax.child(field, 0).unwrap();
    let alias = syntax.child(field, 1).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("name")
    );
    assert_eq!(
        syntax.node(alias).unwrap().kind(),
        SyntaxNodeKind::BindingPattern
    );

    let alias_inner = syntax.first_child(alias).unwrap();

    assert_eq!(
        source.slice(syntax.node(alias_inner).unwrap().span()),
        Some("userName")
    );
}

#[test]
fn parse_tuple_destructuring_binding() {
    let source = source("var (x, y) = point");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();
    let binding = syntax.child(var_item, 0).unwrap();
    let pattern = syntax.first_child(binding).unwrap();

    assert_eq!(
        syntax.node(pattern).unwrap().kind(),
        SyntaxNodeKind::TupleBindingPattern
    );

    assert_eq!(syntax.node(pattern).unwrap().child_count(), 2);
}

#[test]
fn parse_array_destructuring_binding() {
    let source = source("var [a, b, c] = values");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();
    let binding = syntax.child(var_item, 0).unwrap();
    let pattern = syntax.first_child(binding).unwrap();

    assert_eq!(
        syntax.node(pattern).unwrap().kind(),
        SyntaxNodeKind::ArrayBindingPattern
    );

    assert_eq!(syntax.node(pattern).unwrap().child_count(), 3);
}

#[test]
fn parse_array_destructuring_rest_binding() {
    let source = source("var [first, ...rest] = values");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();
    let binding = syntax.child(var_item, 0).unwrap();
    let pattern = syntax.first_child(binding).unwrap();

    assert_eq!(
        syntax.node(pattern).unwrap().kind(),
        SyntaxNodeKind::ArrayBindingPattern
    );

    assert_eq!(syntax.node(pattern).unwrap().child_count(), 2);

    let rest = syntax.child(pattern, 1).unwrap();

    assert_eq!(
        syntax.node(rest).unwrap().kind(),
        SyntaxNodeKind::RestBindingPattern
    );
}

#[test]
fn parse_block_destructuring_binding() {
    let source = source(
        "fn main(): null {
            var { id, name } = user
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

    let var_statement = syntax.first_child(block).unwrap();

    assert_eq!(
        syntax.node(var_statement).unwrap().kind(),
        SyntaxNodeKind::VarStatement
    );

    let binding = syntax.child(var_statement, 0).unwrap();

    assert_eq!(
        syntax.node(binding).unwrap().kind(),
        SyntaxNodeKind::BindingPattern
    );
}

#[test]
fn parse_wildcard_binding_pattern() {
    let source = source("var _ = value");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let var_item = syntax.first_child(root).unwrap();
    let wildcard = syntax.child(var_item, 0).unwrap();

    assert_eq!(
        syntax.node(wildcard).unwrap().kind(),
        SyntaxNodeKind::WildcardPattern
    );
}

#[test]
fn parse_match_wildcard_pattern() {
    let source = source(
        r#"
fn code(value: i32): i32 {
  return match value {
    _ => 0,
  }
}
"#,
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    assert!(find_first_of_kind(syntax, root, SyntaxNodeKind::WildcardPattern).is_some());
}
