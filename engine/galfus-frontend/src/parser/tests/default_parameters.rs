use super::*;

#[test]
fn parse_default_parameter() {
    let source = source("fn greet(name: [i8], punctuation: [i8] = \"!\"): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.kind(), SyntaxNodeKind::ParameterList);
    assert_eq!(parameters_node.child_count(), 2);

    let parameter = parameters_node.child(1).unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    assert_eq!(parameter_node.kind(), SyntaxNodeKind::Parameter);
    assert_eq!(
        source.slice(parameter_node.span()),
        Some("punctuation: [i8] = \"!\"")
    );

    assert_eq!(parameter_node.child_count(), 3);

    let default = parameter_node.child(2).unwrap();
    let default_node = syntax.node(default).unwrap();

    assert_eq!(default_node.kind(), SyntaxNodeKind::ParameterDefault);
    assert_eq!(source.slice(default_node.span()), Some("= \"!\""));

    let value = default_node.first_child().unwrap();

    assert_eq!(
        syntax.node(value).unwrap().kind(),
        SyntaxNodeKind::StringLiteral
    );
}

#[test]
fn parse_multiple_default_parameters() {
    let source = source("fn spawn(kind: [i8] = \"enemy\", health: i32 = 100): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.child_count(), 2);

    for parameter in parameters_node.children() {
        let parameter_node = syntax.node(*parameter).unwrap();

        assert_eq!(parameter_node.kind(), SyntaxNodeKind::Parameter);
        assert_eq!(parameter_node.child_count(), 3);

        let default = parameter_node.child(2).unwrap();

        assert_eq!(
            syntax.node(default).unwrap().kind(),
            SyntaxNodeKind::ParameterDefault
        );
    }
}

#[test]
fn parse_default_parameter_with_expression() {
    let source = source("fn configure(limit: i32 = 10 + 20): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameter = syntax.node(parameters).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let default = parameter_node.child(2).unwrap();
    let default_node = syntax.node(default).unwrap();

    let value = default_node.first_child().unwrap();

    assert_eq!(
        syntax.node(value).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );

    assert_eq!(
        source.slice(syntax.node(value).unwrap().span()),
        Some("10 + 20")
    );
}

#[test]
fn parse_default_parameter_with_struct_literal() {
    let source = source("fn create(user: User = new(User) { name }): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameter = syntax.node(parameters).unwrap().first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    let default = parameter_node.child(2).unwrap();
    let default_node = syntax.node(default).unwrap();

    let value = default_node.first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::StructLiteral);
    assert_eq!(source.slice(value_node.span()), Some("new(User) { name }"));
}

#[test]
fn parse_default_parameter_accepts_trailing_comma() {
    let source = source("fn greet(name: [i8] = \"Ana\",): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.child_count(), 1);

    let parameter = parameters_node.first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    assert_eq!(parameter_node.kind(), SyntaxNodeKind::Parameter);
    assert_eq!(parameter_node.child_count(), 3);
}

#[test]
fn parse_rest_parameter_after_default_with_default_is_valid() {
    let source =
        source("fn log(prefix: [i8] = \"\", ...values: [[i8]] | null = null): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.child_count(), 2);

    let rest = parameters_node.child(1).unwrap();
    let rest_node = syntax.node(rest).unwrap();

    assert_eq!(rest_node.kind(), SyntaxNodeKind::RestParameter);
    assert_eq!(rest_node.child_count(), 3);

    let rest_type = rest_node.child(1).unwrap();
    let rest_default = rest_node.child(2).unwrap();

    assert_eq!(
        syntax.node(rest_type).unwrap().kind(),
        SyntaxNodeKind::UnionType
    );

    assert_eq!(
        syntax.node(rest_default).unwrap().kind(),
        SyntaxNodeKind::ParameterDefault
    );
}

#[test]
fn parse_parameter_default_before_required_parameter() {
    let source = source(
        "fn someFunction(name: [i8] = '', age: u32): null {
            return
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.first_child(root).unwrap();
    let parameters = syntax
        .first_child_of_kind(function, SyntaxNodeKind::ParameterList)
        .unwrap();

    let first = syntax.child(parameters, 0).unwrap();
    let second = syntax.child(parameters, 1).unwrap();

    assert!(
        syntax
            .first_child_of_kind(first, SyntaxNodeKind::ParameterDefault)
            .is_some()
    );

    assert!(
        syntax
            .first_child_of_kind(second, SyntaxNodeKind::ParameterDefault)
            .is_none()
    );
}

#[test]
fn parse_initial_omitted_argument() {
    let source = source(
        "fn main(): null {
            someFunction(_, 32)
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

    let statement = syntax.first_child(block).unwrap();
    let call = syntax.first_child(statement).unwrap();

    let arguments = syntax
        .first_child_of_kind(call, SyntaxNodeKind::ArgumentList)
        .unwrap();

    assert_eq!(syntax.node(arguments).unwrap().child_count(), 2);

    let first = syntax.child(arguments, 0).unwrap();
    let second = syntax.child(arguments, 1).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::OmittedArgument
    );

    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::Argument
    );
}
#[test]
fn parse_middle_omitted_argument() {
    let source = source(
        "fn main(): null {
            someFunction(1, _, 3)
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

    let statement = syntax.first_child(block).unwrap();
    let call = syntax.first_child(statement).unwrap();

    let arguments = syntax
        .first_child_of_kind(call, SyntaxNodeKind::ArgumentList)
        .unwrap();

    assert_eq!(syntax.node(arguments).unwrap().child_count(), 3);

    let middle = syntax.child(arguments, 1).unwrap();

    assert_eq!(
        syntax.node(middle).unwrap().kind(),
        SyntaxNodeKind::OmittedArgument
    );
}

#[test]
fn parse_trailing_comma_does_not_create_omitted_argument() {
    let source = source(
        "fn main(): null {
            someFunction(1,)
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

    let statement = syntax.first_child(block).unwrap();
    let call = syntax.first_child(statement).unwrap();

    let arguments = syntax
        .first_child_of_kind(call, SyntaxNodeKind::ArgumentList)
        .unwrap();

    assert_eq!(syntax.node(arguments).unwrap().child_count(), 1);

    let only = syntax.child(arguments, 0).unwrap();

    assert_eq!(syntax.node(only).unwrap().kind(), SyntaxNodeKind::Argument);
}
