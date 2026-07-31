use super::*;

#[test]
fn parse_grouped_expression_still_works() {
    let source = source("var value = (1 + 2)");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();

    let initializer = syntax
        .first_child_of_kind(var_item, SyntaxNodeKind::Initializer)
        .unwrap();

    let expression = syntax.first_child(initializer).unwrap();

    assert_eq!(
        syntax.node(expression).unwrap().kind(),
        SyntaxNodeKind::GroupedExpression
    );
}

#[test]
fn parse_tuple_expression() {
    let source = source("var point = (10.0, 20.0)");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();

    let initializer = syntax
        .first_child_of_kind(var_item, SyntaxNodeKind::Initializer)
        .unwrap();

    let expression = syntax.first_child(initializer).unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::TupleExpression);
    assert_eq!(expression_node.child_count(), 2);

    let first = syntax.child(expression, 0).unwrap();
    let second = syntax.child(expression, 1).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::FloatLiteral
    );
    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::FloatLiteral
    );
}

#[test]
fn parse_tuple_expression_with_trailing_comma() {
    let source = source("var point = (10.0, 20.0,)");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let var_item = syntax.first_child(root).unwrap();

    let initializer = syntax
        .first_child_of_kind(var_item, SyntaxNodeKind::Initializer)
        .unwrap();

    let expression = syntax.first_child(initializer).unwrap();

    assert_eq!(
        syntax.node(expression).unwrap().kind(),
        SyntaxNodeKind::TupleExpression
    );

    assert_eq!(syntax.node(expression).unwrap().child_count(), 2);
}

#[test]
fn parse_tuple_expression_as_call_argument() {
    let source = source("fn main(): null { print((10, 20)); return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let function = syntax.first_child(root).unwrap();
    let block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let statement = syntax.first_child(block).unwrap();

    let call = syntax
        .first_child_of_kind(statement, SyntaxNodeKind::CallExpression)
        .unwrap();

    let arguments = syntax
        .first_child_of_kind(call, SyntaxNodeKind::ArgumentList)
        .unwrap();

    let argument = syntax.first_child(arguments).unwrap();
    let tuple = syntax.first_child(argument).unwrap();

    assert_eq!(
        syntax.node(tuple).unwrap().kind(),
        SyntaxNodeKind::TupleExpression
    );
}

#[test]
fn parse_rejects_single_element_tuple_expression() {
    let source = source(
        r#"
        fn main(): null {
            var value = (1,)
            return
        }
        "#,
    );

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_rejects_single_element_tuple_type() {
    let source = source(
        r#"
        type Value = (i32,)
        "#,
    );

    let result = parse(&source);

    assert!(result.has_errors());
}
