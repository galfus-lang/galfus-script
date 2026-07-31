use super::*;

#[test]
fn parse_array_type_in_parameter() {
    let source = source("fn first(values: [i32]): i32 { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    let parameter = parameters_node.first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let parameter_type = parameter_node.child(1).unwrap();
    let parameter_type_node = syntax.node(parameter_type).unwrap();

    assert_eq!(parameter_type_node.kind(), SyntaxNodeKind::ArrayType);
    assert_eq!(source.slice(parameter_type_node.span()), Some("[i32]"));

    let element_type = parameter_type_node.first_child().unwrap();
    let element_type_node = syntax.node(element_type).unwrap();

    assert_eq!(element_type_node.kind(), SyntaxNodeKind::NamedType);
    assert_eq!(source.slice(element_type_node.span()), Some("i32"));
}

#[test]
fn parse_nested_array_type() {
    let source = source("fn matrix(values: [[i32]]): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameter = syntax.node(parameters).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let outer_array = parameter_node.child(1).unwrap();
    let outer_array_node = syntax.node(outer_array).unwrap();

    assert_eq!(outer_array_node.kind(), SyntaxNodeKind::ArrayType);
    assert_eq!(source.slice(outer_array_node.span()), Some("[[i32]]"));

    let inner_array = outer_array_node.first_child().unwrap();
    let inner_array_node = syntax.node(inner_array).unwrap();

    assert_eq!(inner_array_node.kind(), SyntaxNodeKind::ArrayType);
    assert_eq!(source.slice(inner_array_node.span()), Some("[i32]"));
}

#[test]
fn parse_union_return_type() {
    let source = source("fn find(): User | null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let return_type = function_node.child(2).unwrap();
    let return_type_node = syntax.node(return_type).unwrap();

    assert_eq!(return_type_node.kind(), SyntaxNodeKind::UnionType);
    assert_eq!(source.slice(return_type_node.span()), Some("User | null"));
    assert_eq!(return_type_node.child_count(), 2);

    let first = return_type_node.first_child().unwrap();
    let second = return_type_node.child(1).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );
    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("User")
    );

    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::TypeNull
    );
    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("null")
    );
}

#[test]
fn parse_union_type_inside_array() {
    let source = source("fn many(values: [User | null]): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameter = syntax.node(parameters).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let parameter_type = parameter_node.child(1).unwrap();
    let array_node = syntax.node(parameter_type).unwrap();

    assert_eq!(array_node.kind(), SyntaxNodeKind::ArrayType);
    assert_eq!(source.slice(array_node.span()), Some("[User | null]"));

    let element_type = array_node.first_child().unwrap();
    let element_type_node = syntax.node(element_type).unwrap();

    assert_eq!(element_type_node.kind(), SyntaxNodeKind::UnionType);
    assert_eq!(source.slice(element_type_node.span()), Some("User | null"));
}

#[test]
fn parse_qualified_type_as_path() {
    let source = source("fn main(value: collections::List): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameter = syntax.node(parameters).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let parameter_type = parameter_node.child(1).unwrap();
    let parameter_type_node = syntax.node(parameter_type).unwrap();

    assert_eq!(parameter_type_node.kind(), SyntaxNodeKind::Path);
    assert_eq!(
        source.slice(parameter_type_node.span()),
        Some("collections::List")
    );
    assert_eq!(parameter_type_node.child_count(), 2);

    let first = parameter_type_node.first_child().unwrap();
    let second = parameter_type_node.child(1).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("collections")
    );

    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("List")
    );
}

#[test]
fn parse_qualified_generic_type_uses_path_base() {
    let source = source("fn main(value: collections::List<i32>): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameter = syntax.node(parameters).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let parameter_type = parameter_node.child(1).unwrap();
    let parameter_type_node = syntax.node(parameter_type).unwrap();

    assert_eq!(parameter_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(
        source.slice(parameter_type_node.span()),
        Some("collections::List<i32>")
    );

    let base = parameter_type_node.first_child().unwrap();
    let base_node = syntax.node(base).unwrap();

    assert_eq!(base_node.kind(), SyntaxNodeKind::Path);
    assert_eq!(source.slice(base_node.span()), Some("collections::List"));

    let arguments = parameter_type_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.kind(), SyntaxNodeKind::TypeArgumentList);
}

#[test]
fn parse_namespace_like_call_as_path_expression() {
    let source = source("fn main(): null { const value = math::random(); return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let const_statement = body_node.first_child().unwrap();
    let const_node = syntax.node(const_statement).unwrap();

    let initializer = const_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(source.slice(expression_node.span()), Some("math::random()"));

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::PathExpression);
    assert_eq!(source.slice(target_node.span()), Some("math::random"));

    let left = target_node.first_child().unwrap();
    let segment = target_node.child(1).unwrap();

    assert_eq!(
        syntax.node(left).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(left).unwrap().span()),
        Some("math")
    );

    assert_eq!(
        syntax.node(segment).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(segment).unwrap().span()),
        Some("random")
    );
}

#[test]
fn parse_anchor_like_call_as_path_expression() {
    let source = source("fn main(): null { const value = user::nameUpper(); return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let const_statement = body_node.first_child().unwrap();
    let const_node = syntax.node(const_statement).unwrap();

    let initializer = const_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(
        source.slice(expression_node.span()),
        Some("user::nameUpper()")
    );

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::PathExpression);
    assert_eq!(source.slice(target_node.span()), Some("user::nameUpper"));

    let left = target_node.first_child().unwrap();
    let segment = target_node.child(1).unwrap();

    assert_eq!(
        syntax.node(left).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(left).unwrap().span()),
        Some("user")
    );

    assert_eq!(
        syntax.node(segment).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(segment).unwrap().span()),
        Some("nameUpper")
    );
}

#[test]
fn parse_chained_path_expression() {
    let source =
        source("fn main(): null { const value = collections::list::first(items); return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let const_statement = body_node.first_child().unwrap();
    let const_node = syntax.node(const_statement).unwrap();

    let initializer = const_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::PathExpression);
    assert_eq!(
        source.slice(target_node.span()),
        Some("collections::list::first")
    );

    let left = target_node.first_child().unwrap();
    let left_node = syntax.node(left).unwrap();

    assert_eq!(left_node.kind(), SyntaxNodeKind::PathExpression);
    assert_eq!(source.slice(left_node.span()), Some("collections::list"));
}

#[test]
fn parse_grouped_type_still_works() {
    let source = source("fn main(value: (i32)): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.first_child(root).unwrap();

    let parameters = syntax
        .first_child_of_kind(function, SyntaxNodeKind::ParameterList)
        .unwrap();

    let parameter = syntax.first_child(parameters).unwrap();
    let ty = syntax.child(parameter, 1).unwrap();

    assert_eq!(syntax.node(ty).unwrap().kind(), SyntaxNodeKind::GroupedType);
}

#[test]
fn parse_tuple_type() {
    let source = source("fn main(value: (i32, [i8])): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.first_child(root).unwrap();

    let parameters = syntax
        .first_child_of_kind(function, SyntaxNodeKind::ParameterList)
        .unwrap();

    let parameter = syntax.first_child(parameters).unwrap();
    let ty = syntax.child(parameter, 1).unwrap();
    let ty_node = syntax.node(ty).unwrap();

    assert_eq!(ty_node.kind(), SyntaxNodeKind::TupleType);
    assert_eq!(ty_node.child_count(), 2);

    let first = syntax.child(ty, 0).unwrap();
    let second = syntax.child(ty, 1).unwrap();

    assert!(syntax.node(first).unwrap().kind().is_type());
    assert!(syntax.node(second).unwrap().kind().is_type());
}

#[test]
fn parse_tuple_type_with_trailing_comma() {
    let source = source("fn main(value: (i32, [i8],)): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.first_child(root).unwrap();

    let parameters = syntax
        .first_child_of_kind(function, SyntaxNodeKind::ParameterList)
        .unwrap();

    let parameter = syntax.first_child(parameters).unwrap();
    let ty = syntax.child(parameter, 1).unwrap();

    assert_eq!(syntax.node(ty).unwrap().kind(), SyntaxNodeKind::TupleType);
    assert_eq!(syntax.node(ty).unwrap().child_count(), 2);
}
