use super::super::*;
use galfus_core::DiagnosticCodeKind;

#[test]
fn parse_expression_function_body() {
    let source = source(
        "fn main(): null {\n  const double = fn (value: i32): i32 => value * 2\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::ExpressionFunction);

    assert_eq!(
        source.slice(expression_node.span()),
        Some("fn (value: i32): i32 => value * 2")
    );

    assert_eq!(expression_node.child_count(), 3);

    let parameters = expression_node.first_child().unwrap();
    let return_type = expression_node.child(1).unwrap();
    let expression_body = expression_node.child(2).unwrap();

    assert_eq!(
        syntax.node(parameters).unwrap().kind(),
        SyntaxNodeKind::ParameterList
    );

    assert_eq!(
        syntax.node(return_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );

    assert_eq!(
        syntax.node(expression_body).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
}

#[test]
fn parse_expression_function_without_return_type() {
    let source =
        source("fn main(): null {\n  const double = fn (value: i32) => value * 2\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let initializer = syntax.node(statement).unwrap().child(1).unwrap();
    let expression = syntax.node(initializer).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::ExpressionFunction);

    assert_eq!(expression_node.child_count(), 2);

    let parameters = expression_node.first_child().unwrap();
    let expression_body = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(parameters).unwrap().kind(),
        SyntaxNodeKind::ParameterList
    );

    assert_eq!(
        syntax.node(expression_body).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
}

#[test]
fn parse_block_function_body() {
    let source = source(
        "fn main(): null {\n  const printer = fn (value: [i8]): null {\n    print(value)\n    return\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let initializer = syntax.node(statement).unwrap().child(1).unwrap();
    let expression = syntax.node(initializer).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BlockFunction);

    let block_body = expression_node.child(2).unwrap();

    assert_eq!(
        syntax.node(block_body).unwrap().kind(),
        SyntaxNodeKind::Block
    );
}

#[test]
fn parse_expression_function_with_rest_default_parameter() {
    let source = source(
        "fn main(): null {\n  const summarize = fn (...values: [i32] | null = null): i32 => 0\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let initializer = syntax.node(statement).unwrap().child(1).unwrap();
    let expression = syntax.node(initializer).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    let parameters = expression_node.first_child().unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.child_count(), 1);

    let parameter = parameters_node.first_child().unwrap();
    let parameter_node = syntax.node(parameter).unwrap();

    assert_eq!(parameter_node.kind(), SyntaxNodeKind::RestParameter);
    assert_eq!(parameter_node.child_count(), 3);
}

#[test]
fn parse_grouped_expression_still_works() {
    let source = source("fn main(): null {\n  const value = (1 + 2) * 3\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let initializer = syntax.node(statement).unwrap().child(1).unwrap();
    let expression = syntax.node(initializer).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);

    let left = expression_node.first_child().unwrap();

    assert_eq!(
        syntax.node(left).unwrap().kind(),
        SyntaxNodeKind::GroupedExpression
    );
}

#[test]
fn parse_expression_function_as_call_argument() {
    let source =
        source("fn main(): null {\n  items.map(fn (item: i32): i32 => item * 2)\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let expression = syntax.node(statement).unwrap().first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);

    let arguments = expression_node.child(1).unwrap();
    let argument = syntax.node(arguments).unwrap().first_child().unwrap();
    let argument_node = syntax.node(argument).unwrap();

    let value = argument_node.first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::ExpressionFunction);
}

#[test]
fn parse_anonymous_function_metadata() {
    let source = source("fn main(): null { const value = fn(stamp) (): i32 => 1; return }");

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let anonymous = find_first_of_kind(syntax, root, SyntaxNodeKind::ExpressionFunction).unwrap();

    assert!(
        syntax
            .first_child_of_kind(anonymous, SyntaxNodeKind::KeywordMetadataList)
            .is_some()
    );
}

#[test]
fn parse_rejects_legacy_anonymous_function_syntax() {
    let source = source("fn main(): null { const value = (item: i32): i32 => item; return }");

    let result = parse(&source);

    assert!(result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    assert!(find_first_of_kind(syntax, root, SyntaxNodeKind::ExpressionFunction).is_none());
}

#[test]
fn parse_rejects_block_body_after_arrow() {
    let source = source("fn main(): null => { return }");

    let result = parse(&source);

    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == ParserDiagnosticCode::ArrowMustIntroduceExpression.as_code()
    }));

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    assert!(syntax.node(root).unwrap().first_child().is_none());
}
