use super::*;

#[test]
fn parse_rest_parameter() {
    let source = source("fn summarize(...values: [i32]): i32 {\n  return 0\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.kind(), SyntaxNodeKind::ParameterList);
    assert_eq!(parameters_node.child_count(), 1);

    let parameter = parameters_node.first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    assert_eq!(parameter_node.kind(), SyntaxNodeKind::RestParameter);
    assert_eq!(
        source.slice(parameter_node.span()),
        Some("...values: [i32]")
    );

    assert_eq!(parameter_node.child_count(), 2);

    let name = parameter_node.first_child().unwrap();
    let type_annotation = parameter_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("values")
    );

    assert_eq!(
        syntax.node(type_annotation).unwrap().kind(),
        SyntaxNodeKind::ArrayType
    );
}

#[test]
fn parse_normal_parameter_followed_by_rest_parameter() {
    let source = source("fn log(prefix: [i8], ...values: [[i8]]): null {\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.child_count(), 2);

    let first = parameters_node.first_child().unwrap();
    let second = parameters_node.child(1).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::Parameter
    );
    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::RestParameter
    );

    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("...values: [[i8]]")
    );
}

#[test]
fn parse_rest_parameter_accepts_trailing_comma() {
    let source = source("fn summarize(...values: [i32],): i32 {\n  return 0\n}");

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

    assert_eq!(
        syntax.node(parameter).unwrap().kind(),
        SyntaxNodeKind::RestParameter
    );
}

#[test]
fn parse_rest_parameter_allows_newline_after_spread() {
    let source = source("fn summarize(...\n  values: [i32]): i32 {\n  return 0\n}");

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

    assert_eq!(parameter_node.kind(), SyntaxNodeKind::RestParameter);
    assert_eq!(
        source.slice(parameter_node.span()),
        Some("...\n  values: [i32]")
    );
}

#[test]
fn parse_rest_parameter_must_be_last() {
    let source = source("fn invalid(...values: [i32], other: i32): null {\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "rest parameter must be the last parameter")
        .expect("missing rest parameter position diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0006");
}

#[test]
fn parse_rest_syntax_is_not_valid_parameter_name_without_spread_context() {
    let source = source("fn invalid(value: ...i32): null {\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());
}
