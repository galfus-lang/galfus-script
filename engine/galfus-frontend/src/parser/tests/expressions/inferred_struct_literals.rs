use super::super::*;

#[test]
fn parse_inferred_struct_literal_as_call_argument() {
    let source =
        source("fn main(): null {\n  createPerson(new { email: \"user@gmail.com\" })\n  return\n}");

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

    assert_eq!(argument_node.kind(), SyntaxNodeKind::Argument);

    let value = argument_node.first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::InferredStructLiteral);
    assert_eq!(
        source.slice(value_node.span()),
        Some("new { email: \"user@gmail.com\" }")
    );
}

#[test]
fn parse_inferred_struct_literal_with_multiple_fields() {
    let source = source(
        "fn main(): null {\n  createPerson(new {\n    name: \"Ana\",\n    email: \"ana@gmail.com\",\n  })\n  return\n}",
    );

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

    let arguments = expression_node.child(1).unwrap();
    let argument = syntax.node(arguments).unwrap().first_child().unwrap();
    let value = syntax.node(argument).unwrap().first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::InferredStructLiteral);

    let fields = value_node.first_child().unwrap();
    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.kind(), SyntaxNodeKind::StructLiteralFieldList);
    assert_eq!(fields_node.child_count(), 2);
}

#[test]
fn parse_inferred_struct_literal_with_shorthand_field() {
    let source = source("fn main(): null {\n  createPerson(new { email })\n  return\n}");

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

    let arguments = expression_node.child(1).unwrap();
    let argument = syntax.node(arguments).unwrap().first_child().unwrap();
    let value = syntax.node(argument).unwrap().first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    let fields = value_node.first_child().unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(
        field_node.kind(),
        SyntaxNodeKind::StructLiteralFieldShorthand
    );

    assert_eq!(source.slice(field_node.span()), Some("email"));
}

#[test]
fn parse_inferred_struct_literal_in_return() {
    let source = source("fn makePerson(): Person {\n  return new { email: \"user@gmail.com\" }\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let return_statement = syntax.node(body).unwrap().first_child().unwrap();
    let return_node = syntax.node(return_statement).unwrap();

    let value = return_node.first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::InferredStructLiteral);
    assert_eq!(
        source.slice(value_node.span()),
        Some("new { email: \"user@gmail.com\" }")
    );
}
