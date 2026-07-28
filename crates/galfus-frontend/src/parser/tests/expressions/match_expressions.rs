use super::super::*;

fn first_match_expression(result: &ParseResult) -> NodeId {
    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let function = syntax.node(root).unwrap().first_child().unwrap();
    let function_node = syntax.node(function).unwrap();
    let body = function_node.child(3).unwrap();
    let statement = syntax.node(body).unwrap().first_child().unwrap();
    let statement_node = syntax.node(statement).unwrap();

    assert_eq!(statement_node.kind(), SyntaxNodeKind::ExpressionStatement);

    statement_node.first_child().unwrap()
}

#[test]
fn parse_match_expression_with_binding_pattern() {
    let source = source(
        "fn main(): null {\n  match value {\n    other {\n      print(other)\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let match_expression = first_match_expression(&result);
    let match_node = syntax.node(match_expression).unwrap();

    assert_eq!(match_node.kind(), SyntaxNodeKind::MatchExpression);
    assert_eq!(match_node.child_count(), 2);

    let subject = match_node.first_child().unwrap();
    let arms = match_node.child(1).unwrap();

    assert_eq!(
        syntax.node(subject).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );
    assert_eq!(
        source.slice(syntax.node(subject).unwrap().span()),
        Some("value")
    );

    let arms_node = syntax.node(arms).unwrap();

    assert_eq!(arms_node.kind(), SyntaxNodeKind::MatchArmList);
    assert_eq!(arms_node.child_count(), 1);

    let arm = arms_node.first_child().unwrap();
    let arm_node = syntax.node(arm).unwrap();

    assert_eq!(arm_node.kind(), SyntaxNodeKind::MatchArm);

    let pattern = arm_node.first_child().unwrap();

    assert_eq!(
        syntax.node(pattern).unwrap().kind(),
        SyntaxNodeKind::BindingPattern
    );

    assert_eq!(
        source.slice(syntax.node(pattern).unwrap().span()),
        Some("other")
    );
}

#[test]
fn parse_match_expression_with_variant_patterns() {
    let source = source(
        "fn main(): null {\n  match result {\n    Result::Ok(user) {\n      print(user.name)\n    },\n    Result::Error(message) {\n      print(message)\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let match_expression = first_match_expression(&result);
    let match_node = syntax.node(match_expression).unwrap();

    let arms = match_node.child(1).unwrap();
    let arms_node = syntax.node(arms).unwrap();

    assert_eq!(arms_node.child_count(), 2);

    let first_arm = arms_node.first_child().unwrap();
    let first_arm_node = syntax.node(first_arm).unwrap();

    let first_pattern = first_arm_node.first_child().unwrap();
    let first_pattern_node = syntax.node(first_pattern).unwrap();

    assert_eq!(first_pattern_node.kind(), SyntaxNodeKind::VariantPattern);

    assert_eq!(
        source.slice(first_pattern_node.span()),
        Some("Result::Ok(user)")
    );

    assert_eq!(first_pattern_node.child_count(), 3);

    let payload = first_pattern_node.child(2).unwrap();
    let payload_node = syntax.node(payload).unwrap();

    assert_eq!(payload_node.kind(), SyntaxNodeKind::VariantPatternPayload);

    assert_eq!(payload_node.child_count(), 1);

    let payload_pattern = payload_node.first_child().unwrap();

    assert_eq!(
        syntax.node(payload_pattern).unwrap().kind(),
        SyntaxNodeKind::BindingPattern
    );
}

#[test]
fn parse_match_expression_with_unit_variant_pattern() {
    let source = source(
        "fn main(): null {\n  match color {\n    Color::Red {\n      print(\"red\")\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let match_expression = first_match_expression(&result);
    let match_node = syntax.node(match_expression).unwrap();

    let arms = match_node.child(1).unwrap();
    let arm = syntax.node(arms).unwrap().first_child().unwrap();
    let arm_node = syntax.node(arm).unwrap();

    let pattern = arm_node.first_child().unwrap();
    let pattern_node = syntax.node(pattern).unwrap();

    assert_eq!(pattern_node.kind(), SyntaxNodeKind::VariantPattern);
    assert_eq!(source.slice(pattern_node.span()), Some("Color::Red"));
    assert_eq!(pattern_node.child_count(), 2);
}

#[test]
fn parse_match_expression_rejects_dot_variant_pattern() {
    let source = source(
        "fn main(): null {\n  match color {\n    Color.Red {\n      print(\"red\")\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_match_expression_with_literal_patterns() {
    let source = source(
        "fn main(): null {\n  match code {\n    200 {\n      print(\"ok\")\n    },\n    404 {\n      print(\"not found\")\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let match_expression = first_match_expression(&result);
    let match_node = syntax.node(match_expression).unwrap();

    let arms = match_node.child(1).unwrap();
    let arms_node = syntax.node(arms).unwrap();

    assert_eq!(arms_node.child_count(), 2);

    let first_arm = arms_node.first_child().unwrap();
    let first_arm_node = syntax.node(first_arm).unwrap();

    let pattern = first_arm_node.first_child().unwrap();
    let pattern_node = syntax.node(pattern).unwrap();

    assert_eq!(pattern_node.kind(), SyntaxNodeKind::LiteralPattern);
    assert_eq!(source.slice(pattern_node.span()), Some("200"));

    let literal = pattern_node.first_child().unwrap();

    assert_eq!(
        syntax.node(literal).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );
}

#[test]
fn parse_match_subject_allows_struct_literal_inside_call_argument() {
    let source = source(
        "fn main(): null {\n  match normalize(new(User) { name }) {\n    other {\n      print(other)\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let match_expression = first_match_expression(&result);
    let match_node = syntax.node(match_expression).unwrap();

    let subject = match_node.first_child().unwrap();
    let subject_node = syntax.node(subject).unwrap();

    assert_eq!(subject_node.kind(), SyntaxNodeKind::CallExpression);
    assert_eq!(
        source.slice(subject_node.span()),
        Some("normalize(new(User) { name })")
    );
}

#[test]
fn parse_match_subject_identifier_does_not_become_struct_literal() {
    let source = source(
        "fn main(): null {\n  match result {\n    other {\n      print(other)\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let match_expression = first_match_expression(&result);
    let match_node = syntax.node(match_expression).unwrap();

    let subject = match_node.first_child().unwrap();

    assert_eq!(
        syntax.node(subject).unwrap().kind(),
        SyntaxNodeKind::NameExpression
    );

    assert_eq!(
        source.slice(syntax.node(subject).unwrap().span()),
        Some("result")
    );
}

#[test]
fn parse_match_expression_with_wildcard_pattern() {
    let source = source(
        "fn main(): null {\n  match value {\n    _ {\n      print(\"fallback\")\n    }\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let match_expression = first_match_expression(&result);
    let match_node = syntax.node(match_expression).unwrap();

    let arms = match_node.child(1).unwrap();
    let arm = syntax.node(arms).unwrap().first_child().unwrap();
    let arm_node = syntax.node(arm).unwrap();

    let pattern = arm_node.first_child().unwrap();
    let pattern_node = syntax.node(pattern).unwrap();

    assert_eq!(pattern_node.kind(), SyntaxNodeKind::WildcardPattern);
    assert_eq!(source.slice(pattern_node.span()), Some("_"));
    assert_eq!(pattern_node.child_count(), 0);
}

#[test]
fn parse_match_expression_with_expression_arms() {
    let source = source(
        "fn describe(value: i32): [i8] {\n  return match value {\n    0 => \"zero\",\n    1 => \"one\",\n    _ => \"many\",\n  }\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let match_expression =
        find_first_of_kind(syntax, root, SyntaxNodeKind::MatchExpression).unwrap();
    let match_node = syntax.node(match_expression).unwrap();
    let arms = match_node.child(1).unwrap();
    let arms_node = syntax.node(arms).unwrap();

    assert_eq!(arms_node.child_count(), 3);

    let first_arm = arms_node.first_child().unwrap();
    let first_arm_node = syntax.node(first_arm).unwrap();
    let body = syntax
        .node(first_arm_node.child(1).unwrap())
        .unwrap()
        .first_child()
        .unwrap();

    assert_eq!(
        syntax.node(body).unwrap().kind(),
        SyntaxNodeKind::StringLiteral
    );
    assert_eq!(
        source.slice(syntax.node(body).unwrap().span()),
        Some("\"zero\"")
    );
}

#[test]
fn parse_match_expression_in_binding_initializer() {
    let source = source(
        "fn main(value: i32): null {\n  var label = match value {\n    0 => \"zero\",\n    _ => \"many\",\n  }\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    assert!(find_first_of_kind(syntax, root, SyntaxNodeKind::MatchExpression).is_some());
}
