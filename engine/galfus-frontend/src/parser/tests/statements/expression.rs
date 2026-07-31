use super::super::*;

#[test]
fn parse_call_expression_statement() {
    let source = source("fn main(): null {\n  print(\"hello\")\n  return\n}");

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

    assert_eq!(statement_node.kind(), SyntaxNodeKind::ExpressionStatement);
    assert_eq!(
        source.slice(statement_node.span()),
        Some("print(\"hello\")")
    );

    let expression = statement_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);
}

#[test]
fn parse_anchor_call_expression_statement() {
    let source = source("fn main(): null {\n  user::rename(\"Ana\")\n  return\n}");

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

    assert_eq!(statement_node.kind(), SyntaxNodeKind::ExpressionStatement);
    assert_eq!(
        source.slice(statement_node.span()),
        Some("user::rename(\"Ana\")")
    );

    let expression = statement_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::PathExpression);
}

#[test]
fn parse_member_call_expression_statement() {
    let source = source("fn main(): null {\n  console.log(\"hello\")\n  return\n}");

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

    assert_eq!(statement_node.kind(), SyntaxNodeKind::ExpressionStatement);
    assert_eq!(
        source.slice(statement_node.span()),
        Some("console.log(\"hello\")")
    );

    let expression = statement_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);
}

#[test]
fn parse_rejects_non_call_expression_statement() {
    let source = source("fn main(): null {\n  1 + 2\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.message() == "expected call expression statement, found `BinaryExpression`"
        })
        .expect("missing non-call expression statement diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0005");
}

#[test]
fn parse_newline_without_expression_continuation_creates_two_statements() {
    let source = source("fn main(): null {\n  const value = math\n  random()\n  return\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    assert_eq!(body_node.child_count(), 3);

    let first = body_node.first_child().unwrap();
    let second = body_node.child(1).unwrap();
    let third = body_node.child(2).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::ConstStatement
    );
    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::ExpressionStatement
    );
    assert_eq!(
        syntax.node(third).unwrap().kind(),
        SyntaxNodeKind::ReturnStatement
    );

    assert_eq!(
        source.slice(syntax.node(first).unwrap().span()),
        Some("const value = math")
    );
    assert_eq!(
        source.slice(syntax.node(second).unwrap().span()),
        Some("random()")
    );
}

#[test]
fn parse_expression_statement_requires_terminator() {
    let source = source("fn main(): null {\n  print(\"a\") print(\"b\")\n  return\n}");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.message() == "expected statement terminator, found `Identifier`"
        })
        .expect("missing statement terminator diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}
