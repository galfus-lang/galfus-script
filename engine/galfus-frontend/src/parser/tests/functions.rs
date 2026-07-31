use super::*;

#[test]
fn parse_function_item_minimal() {
    let source = source("fn main(): null {}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.kind(), SyntaxNodeKind::SourceFile);
    assert_eq!(root_node.child_count(), 1);

    let function = root_node.first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    assert_eq!(function_node.kind(), SyntaxNodeKind::FunctionItem);
    assert_eq!(
        source.slice(function_node.span()),
        Some("fn main(): null {}")
    );
    assert_eq!(function_node.child_count(), 4);

    let name = function_node.first_child().unwrap();
    let parameters = function_node.child(1).unwrap();
    let return_type = function_node.child(2).unwrap();
    let body = function_node.child(3).unwrap();

    assert_eq!(
        syntax.node(name).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("main")
    );

    assert_eq!(
        syntax.node(parameters).unwrap().kind(),
        SyntaxNodeKind::ParameterList
    );

    assert_eq!(
        syntax.node(return_type).unwrap().kind(),
        SyntaxNodeKind::TypeNull
    );

    assert_eq!(syntax.node(body).unwrap().kind(), SyntaxNodeKind::Block);
}

#[test]
fn parse_reports_expected_item_at_top_level() {
    let source = source("return");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "P0002");
    assert_eq!(diagnostic.message(), "expected item, found `Return`");
}

#[test]
fn parse_return_statement_inside_block() {
    let source = source("fn main(): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    let function = root_node.first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    assert_eq!(body_node.kind(), SyntaxNodeKind::Block);
    assert_eq!(body_node.child_count(), 1);

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    assert_eq!(statement_node.kind(), SyntaxNodeKind::ReturnStatement);
    assert_eq!(source.slice(statement_node.span()), Some("return"));
}

#[test]
fn parse_reports_expected_statement_inside_block() {
    let source = source("fn main(): null { export }");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "P0005");
    assert_eq!(diagnostic.message(), "expected statement, found `Export`");
}

#[test]
fn parse_function_parameters() {
    let source = source("fn sum(a: i32, b: i32): i32 { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    let function = root_node.first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.kind(), SyntaxNodeKind::ParameterList);
    assert_eq!(parameters_node.child_count(), 2);

    let first_parameter = parameters_node.first_child().unwrap();
    let first_parameter_node = syntax.node(first_parameter).unwrap();

    assert_eq!(first_parameter_node.kind(), SyntaxNodeKind::Parameter);
    assert_eq!(source.slice(first_parameter_node.span()), Some("a: i32"));

    let first_name = first_parameter_node.first_child().unwrap();
    let first_type = first_parameter_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(first_name).unwrap().span()),
        Some("a")
    );
    assert_eq!(
        syntax.node(first_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );
    assert_eq!(
        source.slice(syntax.node(first_type).unwrap().span()),
        Some("i32")
    );

    let second_parameter = parameters_node.child(1).unwrap();
    let second_parameter_node = syntax.node(second_parameter).unwrap();

    assert_eq!(source.slice(second_parameter_node.span()), Some("b: i32"));
}

#[test]
fn parse_empty_parameter_list_still_works() {
    let source = source("fn main(): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let parameters = function_node.child(1).unwrap();
    let parameters_node = syntax.node(parameters).unwrap();

    assert_eq!(parameters_node.kind(), SyntaxNodeKind::ParameterList);
    assert!(parameters_node.children().is_empty());
}

#[test]
fn parse_parameter_list_accepts_trailing_comma() {
    let source = source("fn sum(a: i32, b: i32,): i32 { return }");

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
}

#[test]
fn parse_parameter_list_accepts_multiline_trailing_comma() {
    let source = source("fn sum(\n  a: i32,\n  b: i32,\n): i32 {\n  return\n}");

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
}

#[test]
fn parse_stamp_function_item() {
    let source = source(
        r#"
        fn(stamp) max(a: i32, b: i32): i32 {
            return a
        }
        "#,
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let graph = result.graph();
    let syntax = graph.syntax();
    let root = syntax.root().unwrap();

    assert!(find_first_of_kind(syntax, root, SyntaxNodeKind::FunctionItem).is_some());
    assert!(find_first_of_kind(syntax, root, SyntaxNodeKind::KeywordMetadataList).is_some());
}

#[test]
fn parse_named_function_expression_body() {
    let source = source("fn double(value: i32): i32 => value * 2");

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let body = syntax.node(function).unwrap().last_child().unwrap();

    assert_eq!(
        syntax.node(body).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );
}

#[test]
fn parse_export_stamp_function_item() {
    let source = source(
        r#"
        export fn(stamp) min(a: i32, b: i32): i32 {
            return a
        }
        "#,
    );

    let result = parse(&source);

    assert!(!result.has_errors());
}

#[test]
fn parse_decorated_rest_parameter() {
    let source = source(
        r#"
        fn summarize(@nonempty ...values: [i32]): i32 {
            return 0
        }
        "#,
    );

    let result = parse(&source);

    assert!(!result.has_errors());
}

#[test]
fn parse_rejects_function_without_return_type() {
    let source = source(
        r#"
        fn log(message: [u8]) {
            return
        }
        "#,
    );

    let result = parse(&source);

    assert!(result.has_errors());
}
