use super::super::*;

#[test]
fn parse_async_function_declaration_and_await_expression() {
    let source = source(
        "fn(async) fetchUser(id: i64): User {\n  const user = await loadUser(id)\n  return user\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let await_expression = find_first_of_kind(syntax, root, SyntaxNodeKind::AwaitExpression);

    assert!(await_expression.is_some());
}

#[test]
fn parse_await_all_tuple_expression() {
    let source = source(
        "fn(async) fetchBoth(): null {\n  const result = await(all) (\n    loadA(),\n    loadB(),\n  )\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let await_all = find_first_of_kind(syntax, root, SyntaxNodeKind::AwaitAllExpression);

    assert!(await_all.is_some());
}

#[test]
fn parse_await_race_tuple_expression() {
    let source = source(
        "fn(async) fetchFastest(): null {\n  const winner = await(race) (\n    fetchFromA(),\n    fetchFromB(),\n  )\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let await_race = find_first_of_kind(syntax, root, SyntaxNodeKind::AwaitRaceExpression);

    assert!(await_race.is_some());
}

#[test]
fn parse_anonymous_async_function_expression() {
    let source = source(
        "fn main(): null {\n  const load = fn(async) (id: i64): User => await loadUser(id)\n  return\n}",
    );

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}
