use super::super::*;

#[test]
fn parse_call_expression_without_arguments() {
    let source = source("fn main(): null { var user: User = createUser(); return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let var_statement = body_node.first_child().unwrap();
    let var_node = syntax.node(var_statement).unwrap();

    let initializer = var_node.child(2).unwrap();
    let initializer_node = syntax.node(initializer).unwrap();

    let expression = initializer_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(source.slice(expression_node.span()), Some("createUser()"));
    assert_eq!(expression_node.child_count(), 2);

    let target = expression_node.first_child().unwrap();
    let arguments = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(target).unwrap().span()),
        Some("createUser")
    );

    assert_eq!(
        syntax.node(arguments).unwrap().kind(),
        SyntaxNodeKind::ArgumentList
    );
    assert!(syntax.node(arguments).unwrap().children().is_empty());
}

#[test]
fn parse_call_expression_with_arguments() {
    let source = source("fn main(): null { const value = add(1, 2); return }");

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
    assert_eq!(source.slice(expression_node.span()), Some("add(1, 2)"));

    let arguments = expression_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.kind(), SyntaxNodeKind::ArgumentList);
    assert_eq!(arguments_node.child_count(), 2);

    let first_argument = arguments_node.first_child().unwrap();
    let first_argument_node = syntax.node(first_argument).unwrap();

    assert_eq!(first_argument_node.kind(), SyntaxNodeKind::Argument);

    let first_expression = first_argument_node.first_child().unwrap();

    assert_eq!(
        syntax.node(first_expression).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );

    assert_eq!(
        source.slice(syntax.node(first_expression).unwrap().span()),
        Some("1")
    );
}

#[test]
fn parse_call_expression_accepts_trailing_comma() {
    let source = source("fn main(): null { const value = add(1, 2,); return }");

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

    let arguments = expression_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.child_count(), 2);
    assert_eq!(source.slice(arguments_node.span()), Some("(1, 2,)"));
}

#[test]
fn parse_call_argument_list_recovers_missing_comma() {
    let source = source("fn main(): null { const value = add(1 2); return }");

    let result = parse(&source);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "P0001"
            && diagnostic.message() == "expected `Comma`, found `Integer`"
    }));

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
    let arguments = expression_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.child_count(), 2);
}

#[test]
fn parse_reports_missing_statement_terminator() {
    let source = source("fn main(): null { const value = add(1, 2,) return }");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.message() == "expected statement terminator, found `Return`")
        .expect("missing statement terminator diagnostic");

    assert_eq!(diagnostic.code().as_str(), "P0001");
}

#[test]
fn parse_member_expression_in_return() {
    let source = source("fn main(): [i8] {\n  return user.name\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();

    let body = function_node.child(3).unwrap();
    let body_node = syntax.node(body).unwrap();

    let return_statement = body_node.first_child().unwrap();
    let return_node = syntax.node(return_statement).unwrap();

    let expression = return_node.first_child().unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(source.slice(expression_node.span()), Some("user.name"));
    assert_eq!(expression_node.child_count(), 2);

    let target = expression_node.first_child().unwrap();
    let member = expression_node.child(1).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(target).unwrap().span()),
        Some("user")
    );

    assert_eq!(
        syntax.node(member).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(member).unwrap().span()),
        Some("name")
    );
}

#[test]
fn parse_call_on_member_expression() {
    let source = source("fn main(): null {\n  const value = math.sin(1)\n  return\n}");

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
    assert_eq!(source.slice(expression_node.span()), Some("math.sin(1)"));

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(source.slice(target_node.span()), Some("math.sin"));

    let arguments = expression_node.child(1).unwrap();
    let arguments_node = syntax.node(arguments).unwrap();

    assert_eq!(arguments_node.kind(), SyntaxNodeKind::ArgumentList);
    assert_eq!(arguments_node.child_count(), 1);
}

#[test]
fn parse_allows_newline_before_member_access() {
    let source = source("fn main(): null {\n  const value = math\n  .random()\n  return\n}");

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
        Some("math\n  .random()")
    );

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(source.slice(target_node.span()), Some("math\n  .random"));
}

#[test]
fn parse_allows_newline_after_member_dot() {
    let source = source("fn main(): null {\n  const value = math.\n  random()\n  return\n}");

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
        Some("math.\n  random()")
    );
}

#[test]
fn parse_allows_newline_before_anchor_access() {
    let source =
        source("fn main(): null {\n  const value = user\n  ::rename(\"Ana\")\n  return\n}");

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
        Some("user\n  ::rename(\"Ana\")")
    );

    let target = expression_node.first_child().unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::PathExpression);
    assert_eq!(source.slice(target_node.span()), Some("user\n  ::rename"));
}

#[test]
fn parse_allows_newline_after_anchor_operator() {
    let source =
        source("fn main(): null {\n  const value = user::\n  rename(\"Ana\")\n  return\n}");

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
        Some("user::\n  rename(\"Ana\")")
    );
}

#[test]
fn parse_null_safe_member_expression() {
    let source = source("var name = user?.name");

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

    assert_eq!(
        expression_node.kind(),
        SyntaxNodeKind::NullSafeMemberExpression
    );

    assert_eq!(expression_node.child_count(), 2);

    let target = syntax.child(expression, 0).unwrap();
    let member = syntax.child(expression, 1).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        syntax.node(member).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
}

#[test]
fn parse_mixed_member_and_null_safe_member_chain() {
    let source = source("var name = user.parent?.name");

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

    assert_eq!(
        expression_node.kind(),
        SyntaxNodeKind::NullSafeMemberExpression
    );

    let target = syntax.child(expression, 0).unwrap();
    let target_node = syntax.node(target).unwrap();

    assert_eq!(target_node.kind(), SyntaxNodeKind::MemberExpression);
    assert_eq!(source.slice(target_node.span()), Some("user.parent"));
}

#[test]
fn parse_null_safe_member_call() {
    let source = source("var name = user?.getName()");

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

    assert_eq!(expression_node.kind(), SyntaxNodeKind::CallExpression);

    let callee = syntax.child(expression, 0).unwrap();
    let callee_node = syntax.node(callee).unwrap();

    assert_eq!(callee_node.kind(), SyntaxNodeKind::NullSafeMemberExpression);
}

#[test]
fn parse_reports_call_spread_argument() {
    let source = source(
        r#"
        call(1, ...values)
        "#,
    );

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_reports_call_spread_argument_before_trailing_argument() {
    let source = source(
        r#"
        call(1, ...values, 10)
        "#,
    );

    let result = parse(&source);

    assert!(result.has_errors());
}
