use super::*;

#[test]
fn parse_grouped_function_type() {
    let source = source("type MaybeCallback = (fn (): null) | null");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let alias_type = alias_node.child(1).unwrap();
    let alias_type_node = syntax.node(alias_type).unwrap();

    assert_eq!(alias_type_node.kind(), SyntaxNodeKind::UnionType);
    assert_eq!(
        source.slice(alias_type_node.span()),
        Some("(fn (): null) | null")
    );

    let left = alias_type_node.first_child().unwrap();
    let right = alias_type_node.child(1).unwrap();

    assert_eq!(
        syntax.node(left).unwrap().kind(),
        SyntaxNodeKind::GroupedType
    );

    assert_eq!(syntax.node(right).unwrap().kind(), SyntaxNodeKind::TypeNull);

    let grouped_inner = syntax.node(left).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(grouped_inner).unwrap().kind(),
        SyntaxNodeKind::FunctionType
    );
}

#[test]
fn parse_function_type_returning_union() {
    let source = source("type Callback = fn (): [i8] | null");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let alias_type = alias_node.child(1).unwrap();
    let alias_type_node = syntax.node(alias_type).unwrap();

    assert_eq!(alias_type_node.kind(), SyntaxNodeKind::FunctionType);

    let return_type = alias_type_node.child(1).unwrap();

    assert_eq!(
        syntax.node(return_type).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );

    assert_eq!(
        source.slice(syntax.node(return_type).unwrap().span()),
        Some("[i8] | null")
    );
}

#[test]
fn parse_struct_field_nullable_function_type() {
    let source = source("struct Button {\n  onClick: (fn (): null) | null = null,\n}");

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
        SyntaxNodeKind::UnionType
    );

    assert_eq!(
        source.slice(syntax.node(field_type).unwrap().span()),
        Some("(fn (): null) | null")
    );

    let default = field_node.child(2).unwrap();

    assert_eq!(
        syntax.node(default).unwrap().kind(),
        SyntaxNodeKind::StructFieldDefault
    );
}

#[test]
fn parse_var_annotation_nullable_function_type() {
    let source =
        source("fn main(): null {\n  var callback: (fn (): null) | null = null\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let annotation = statement_node.child(1).unwrap();
    let annotation_node = syntax.node(annotation).unwrap();

    let annotation_type = annotation_node.first_child().unwrap();

    assert_eq!(
        syntax.node(annotation_type).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );

    assert_eq!(
        source.slice(syntax.node(annotation_type).unwrap().span()),
        Some("(fn (): null) | null")
    );
}

#[test]
fn parse_grouped_type_inside_generic_argument() {
    let source = source("type Listener = Box<(fn ([i8]): null) | null>");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let generic_type = alias_node.child(1).unwrap();
    let generic_type_node = syntax.node(generic_type).unwrap();

    assert_eq!(generic_type_node.kind(), SyntaxNodeKind::GenericType);

    let args = generic_type_node.child(1).unwrap();
    let first_arg = syntax.node(args).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(first_arg).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );

    assert_eq!(
        source.slice(syntax.node(first_arg).unwrap().span()),
        Some("(fn ([i8]): null) | null")
    );
}

#[test]
fn parse_grouped_union_type() {
    let source = source("type MaybeUserList = [User | null]");

    let result = parse(&source);

    assert!(!result.has_errors());
}

#[test]
fn parse_grouped_plain_union_type() {
    let source = source("type MaybeUserList = [(User | null)]");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let array_type = alias_node.child(1).unwrap();
    let element_type = syntax.node(array_type).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(element_type).unwrap().kind(),
        SyntaxNodeKind::GroupedType
    );
}
