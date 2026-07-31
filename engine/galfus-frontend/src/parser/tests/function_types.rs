use super::*;

#[test]
fn parse_type_alias_function_type_no_parameters() {
    let source = source("type Callback = fn (): null");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let function_type = alias_node.child(1).unwrap();
    let function_type_node = syntax.node(function_type).unwrap();

    assert_eq!(function_type_node.kind(), SyntaxNodeKind::FunctionType);
    assert_eq!(source.slice(function_type_node.span()), Some("fn (): null"));

    let parameters = function_type_node.first_child().unwrap();
    let return_type = function_type_node.child(1).unwrap();

    assert_eq!(
        syntax.node(parameters).unwrap().kind(),
        SyntaxNodeKind::FunctionTypeParameterList
    );

    assert_eq!(syntax.node(parameters).unwrap().child_count(), 0);

    assert_eq!(
        syntax.node(return_type).unwrap().kind(),
        SyntaxNodeKind::TypeNull
    );
}

#[test]
fn parse_type_alias_function_type_with_parameters() {
    let source = source("type Predicate = fn ([i8], i32): bool");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let function_type = alias_node.child(1).unwrap();
    let function_type_node = syntax.node(function_type).unwrap();

    assert_eq!(function_type_node.kind(), SyntaxNodeKind::FunctionType);
    assert_eq!(
        source.slice(function_type_node.span()),
        Some("fn ([i8], i32): bool")
    );

    let parameters = function_type_node.first_child().unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.child_count(), 2);

    let first = parameters_node.first_child().unwrap();
    let second = parameters_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("[i8]")
    );

    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("i32")
    );
}

#[test]
fn parse_struct_field_function_type() {
    let source = source("struct Button {\n  onClick: fn (): null,\n}");

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
        SyntaxNodeKind::FunctionType
    );

    assert_eq!(
        source.slice(syntax.node(field_type).unwrap().span()),
        Some("fn (): null")
    );
}

#[test]
fn parse_var_type_annotation_function_type() {
    let source =
        source("fn main(): null {\n  var callback: fn (): null = handleClick\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let type_annotation = statement_node.child(1).unwrap();
    let type_annotation_node = syntax.node(type_annotation).unwrap();

    assert_eq!(type_annotation_node.kind(), SyntaxNodeKind::TypeAnnotation);

    let function_type = type_annotation_node.first_child().unwrap();

    assert_eq!(
        syntax.node(function_type).unwrap().kind(),
        SyntaxNodeKind::FunctionType
    );
}

#[test]
fn parse_generic_function_type() {
    let source = source("type Transform<T, U> = fn (T): U");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let alias = syntax.node(root).unwrap().first_child().unwrap();
    let alias_node = syntax.node(alias).unwrap();

    let function_type = alias_node.child(2).unwrap();
    let function_type_node = syntax.node(function_type).unwrap();

    assert_eq!(function_type_node.kind(), SyntaxNodeKind::FunctionType);
    assert_eq!(source.slice(function_type_node.span()), Some("fn (T): U"));

    let parameters = function_type_node.first_child().unwrap();
    let parameter = syntax.node(parameters).unwrap().first_child().unwrap();

    assert_eq!(
        source.slice(syntax.node(parameter).unwrap().span()),
        Some("T")
    );

    let return_type = function_type_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(return_type).unwrap().span()),
        Some("U")
    );
}

#[test]
fn parse_function_type_as_generic_argument() {
    let source = source("type Listener = Box<fn ([i8]): null>");

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
        SyntaxNodeKind::FunctionType
    );
}
