use super::super::*;

#[test]
fn parse_empty_struct_literal() {
    let source = source("fn main(): null {\n  const user = new(User) {}\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::StructLiteral);
    assert_eq!(source.slice(expression_node.span()), Some("new(User) {}"));
    assert_eq!(expression_node.child_count(), 2);

    let name = expression_node.first_child().unwrap();
    let fields = expression_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("User")
    );

    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.kind(), SyntaxNodeKind::StructLiteralFieldList);

    assert!(fields_node.children().is_empty());
}

#[test]
fn parse_struct_literal_with_fields() {
    let source = source(
        "fn main(): null {\n  const user = new(User) {\n    name: \"Ana\",\n    age: 32,\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::StructLiteral);

    let fields = expression_node.child(1).unwrap();
    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.kind(), SyntaxNodeKind::StructLiteralFieldList);
    assert_eq!(fields_node.child_count(), 2);

    let first_field = fields_node.first_child().unwrap();
    let first_field_node = syntax.node(first_field).unwrap();

    assert_eq!(first_field_node.kind(), SyntaxNodeKind::StructLiteralField);

    assert_eq!(source.slice(first_field_node.span()), Some("name: \"Ana\""));

    let first_name = first_field_node.first_child().unwrap();
    let first_value = first_field_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(first_name).unwrap().span()),
        Some("name")
    );
    assert_eq!(
        syntax.node(first_value).unwrap().kind(),
        SyntaxNodeKind::StringLiteral
    );
}

#[test]
fn parse_struct_literal_field_value_can_be_expression() {
    let source = source(
        "fn main(): null {\n  const user = new(User) {\n    age: currentAge + 1,\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    let fields = expression_node.child(1).unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    let value = field_node.child(1).unwrap();

    assert_eq!(
        syntax.node(value).unwrap().kind(),
        SyntaxNodeKind::BinaryExpression
    );

    assert_eq!(
        source.slice(syntax.node(value).unwrap().span()),
        Some("currentAge + 1")
    );
}

#[test]
fn parse_nested_struct_literal() {
    let source = source(
        "fn main(): null {\n  const user = new(User) {\n    address: new(Address) {\n      city: \"Recife\",\n    },\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    let fields = expression_node.child(1).unwrap();
    let field = syntax.node(fields).unwrap().first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    let value = field_node.child(1).unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::StructLiteral);
    assert_eq!(
        source.slice(value_node.span()),
        Some("new(Address) {\n      city: \"Recife\",\n    }")
    );
}

#[test]
fn parse_struct_literal_requires_commas_between_fields() {
    let source = source(
        "fn main(): null {\n  const user = new(User) {\n    name: \"Ana\"\n    age: 32,\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "expected `Comma`, found `Identifier`")
        .expect("missing comma diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}

#[test]
fn parse_if_condition_allows_struct_literal_inside_call_argument() {
    let source = source(
        "fn main(): null {\n  if isValid(new(User) { permission: permission }) {\n    print(\"yes\")\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let if_statement = body_node.first_child().unwrap();
    let if_node = syntax.node(if_statement).unwrap();

    let condition = if_node.first_child().unwrap();
    let condition_node = syntax.node(condition).unwrap();

    assert_eq!(condition_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(
        source.slice(condition_node.span()),
        Some("isValid(new(User) { permission: permission })")
    );

    let arguments = condition_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    let argument = arguments_node.first_child().unwrap();
    let argument_node = syntax.node(argument).unwrap();

    let value = argument_node.first_child().unwrap();
    let value_node = syntax.node(value).unwrap();

    assert_eq!(value_node.kind(), SyntaxNodeKind::StructLiteral);
    assert_eq!(
        source.slice(value_node.span()),
        Some("new(User) { permission: permission }")
    );
}

#[test]
fn parse_if_condition_allows_parenthesized_struct_literal() {
    let source = source(
        "fn main(): null {\n  if (new(User) { permission: permission }) {\n    print(\"yes\")\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let if_statement = body_node.first_child().unwrap();
    let if_node = syntax.node(if_statement).unwrap();

    let condition = if_node.first_child().unwrap();
    let condition_node = syntax.node(condition).unwrap();

    assert_eq!(condition_node.kind(), SyntaxNodeKind::GroupedExpression);

    let inner = condition_node.first_child().unwrap();
    let inner_node = syntax.node(inner).unwrap();

    assert_eq!(inner_node.kind(), SyntaxNodeKind::StructLiteral);
}

#[test]
fn parse_struct_literal_field_shorthand() {
    let source =
        source("fn main(): null {\n  const user = new(User) {\n    name,\n  }\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    let fields = expression_node.child(1).unwrap();
    let fields_node = syntax.node(fields).unwrap();

    let field = fields_node.first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(
        field_node.kind(),
        SyntaxNodeKind::StructLiteralFieldShorthand
    );

    assert_eq!(source.slice(field_node.span()), Some("name"));
    assert_eq!(field_node.child_count(), 1);

    let name = field_node.first_child().unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("name")
    );
}

#[test]
fn parse_struct_literal_mixed_shorthand_and_named_fields() {
    let source = source(
        "fn main(): null {\n  const user = new(User) {\n    name,\n    age: 32,\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let statement = body_node.first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    let initializer = statement_node.child(1).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    let fields = expression_node.child(1).unwrap();
    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.child_count(), 2);

    let first = fields_node.first_child().unwrap();
    let second = fields_node.child(1).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::StructLiteralFieldShorthand
    );

    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::StructLiteralField
    );

    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("name")
    );
    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("age: 32")
    );
}

#[test]
fn parse_struct_literal_shorthand_requires_comma_between_fields() {
    let source = source(
        "fn main(): null {\n  const user = new(User) {\n    name\n    age,\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "expected `Comma`, found `Identifier`")
        .expect("missing comma diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}

#[test]
fn parse_struct_expansion_field() {
    let source = source(
        "struct Person {
            ...User,
            age: i32,
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let struct_item = syntax.first_child(root).unwrap();

    let fields = syntax
        .first_child_of_kind(struct_item, SyntaxNodeKind::StructFieldList)
        .unwrap();

    let expansion = syntax.child(fields, 0).unwrap();
    let normal_field = syntax.child(fields, 1).unwrap();

    assert_eq!(
        syntax.node(expansion).unwrap().kind(),
        SyntaxNodeKind::StructExpansion
    );

    assert_eq!(
        syntax.node(normal_field).unwrap().kind(),
        SyntaxNodeKind::StructField
    );
}

#[test]
fn parse_struct_literal_spread_field() {
    let source = source(
        "fn main(): null {
            var user2 = new(User) {
                ...user,
                name: \"Ana\",
            }
            return
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.first_child(root).unwrap();

    let body = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let var_statement = syntax.first_child(body).unwrap();

    let initializer = syntax
        .first_child_of_kind(var_statement, SyntaxNodeKind::Initializer)
        .unwrap();

    let struct_literal = syntax.first_child(initializer).unwrap();

    assert_eq!(
        syntax.node(struct_literal).unwrap().kind(),
        SyntaxNodeKind::StructLiteral
    );

    let fields = syntax
        .first_child_of_kind(struct_literal, SyntaxNodeKind::StructLiteralFieldList)
        .unwrap();

    let spread = syntax.child(fields, 0).unwrap();
    let field = syntax.child(fields, 1).unwrap();

    assert_eq!(
        syntax.node(spread).unwrap().kind(),
        SyntaxNodeKind::SpreadStructLiteralField
    );

    assert_eq!(
        syntax.node(field).unwrap().kind(),
        SyntaxNodeKind::StructLiteralField
    );
}

#[test]
fn parse_rejects_legacy_typed_struct_literal_syntax() {
    let source = source("fn main(): null {\n  const user = User {}\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_rejects_legacy_inferred_struct_literal_syntax() {
    let source = source("fn main(): null {\n  const user = struct { name }\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());
}
