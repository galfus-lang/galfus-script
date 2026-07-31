use super::*;

#[test]
fn parse_generic_parameter_constraint_identifier() {
    let source = source("fn add<T: int>(a: T, b: T): T {\n  return a + b\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let generics = function_node.child(1).unwrap();
    let generics_node = syntax.node(generics).unwrap();

    assert_eq!(generics_node.kind(), SyntaxNodeKind::GenericParameterList);
    assert_eq!(generics_node.child_count(), 1);

    let parameter = generics_node.first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    assert_eq!(parameter_node.kind(), SyntaxNodeKind::GenericParameter);
    assert_eq!(parameter_node.child_count(), 2);
    assert_eq!(source.slice(parameter_node.span()), Some("T: int"));

    let constraint = parameter_node.child(1).unwrap();
    let constraint_node = syntax.node(constraint).unwrap();

    assert_eq!(
        constraint_node.kind(),
        SyntaxNodeKind::GenericParameterConstraint
    );

    let constraint_type = constraint_node.first_child().unwrap();

    assert_eq!(
        syntax.node(constraint_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );

    assert_eq!(
        source.slice(syntax.node(constraint_type).unwrap().span()),
        Some("int")
    );
}

#[test]
fn parse_generic_parameter_constraint_function_type() {
    let source = source("fn call<T: fn (): null>(callback: T): T {\n  return callback\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let generics = function_node.child(1).unwrap();
    let parameter = syntax.node(generics).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let constraint = parameter_node.child(1).unwrap();
    let constraint_type = syntax.node(constraint).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(constraint_type).unwrap().kind(),
        SyntaxNodeKind::FunctionType
    );

    assert_eq!(
        source.slice(syntax.node(constraint_type).unwrap().span()),
        Some("fn (): null")
    );
}

#[test]
fn parse_generic_parameter_constraint_direct_type() {
    let source = source("fn process<T: User>(value: T): T {\n  return value\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let generics = function_node.child(1).unwrap();
    let parameter = syntax.node(generics).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let constraint = parameter_node.child(1).unwrap();
    let constraint_type = syntax.node(constraint).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(constraint_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );

    assert_eq!(
        source.slice(syntax.node(constraint_type).unwrap().span()),
        Some("User")
    );
}

#[test]
fn parse_generic_parameter_constraint_generic_type() {
    let source = source("fn load<T: Result<Texture, LoadError>>(value: T): T {\n  return value\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let generics = function_node.child(1).unwrap();
    let parameter = syntax.node(generics).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let constraint = parameter_node.child(1).unwrap();
    let constraint_type = syntax.node(constraint).unwrap().first_child().unwrap();
    let constraint_type_node = syntax.node(constraint_type).unwrap();

    assert_eq!(constraint_type_node.kind(), SyntaxNodeKind::GenericType);
    assert_eq!(
        source.slice(constraint_type_node.span()),
        Some("Result<Texture, LoadError>")
    );
}

#[test]
fn parse_multiple_generic_parameter_constraints() {
    let source = source("fn pair<T: int, U: bool>(first: T, second: U): T {\n  return first\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let generics = function_node.child(1).unwrap();
    let generics_node = syntax.node(generics).unwrap();

    assert_eq!(generics_node.child_count(), 2);

    let first = generics_node.first_child().unwrap();
    let second = generics_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("T: int")
    );
    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("U: bool")
    );
}

#[test]
fn parse_unconstrained_generic_parameter_still_works() {
    let source = source("fn identity<T>(value: T): T {\n  return value\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let generics = function_node.child(1).unwrap();
    let generics_node = syntax.node(generics).unwrap();

    let parameter = generics_node.first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    assert_eq!(parameter_node.kind(), SyntaxNodeKind::GenericParameter);
    assert_eq!(parameter_node.child_count(), 1);
}
