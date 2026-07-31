use super::*;

#[test]
fn resolve_reports_unknown_path_expression_root() {
    let source = source(
        r#"
        fn main(): null {
            var created = missing::create()
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let expression =
        find_path_expression_by_text(syntax, &source, root, "missing::create").unwrap();

    assert!(resolution.reference_symbol(expression).is_none());

    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains("unresolved name `missing`"))
    );
}

#[test]
fn resolve_binds_top_level_initializer_name_expression() {
    let source = source(
        r#"
        const first = 1
        const second = first
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let expression = find_name_expression_by_text(syntax, &source, root, "first").unwrap();

    let symbol = resolution.reference_symbol(expression).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "first");
    assert_eq!(symbol.kind(), SymbolKind::Const);
}

#[test]
fn resolve_binds_parameter_default_name_expression() {
    let source = source(
        r#"
        const fallback = 1

        fn main(value: i32 = fallback): null {
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let expression = find_name_expression_by_text(syntax, &source, root, "fallback").unwrap();

    let symbol = resolution.reference_symbol(expression).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "fallback");
    assert_eq!(symbol.kind(), SymbolKind::Const);
}

#[test]
fn resolve_reports_unknown_top_level_initializer_name_expression() {
    let source = source(
        r#"
        const value = missing
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let expression = find_name_expression_by_text(syntax, &source, root, "missing").unwrap();

    assert!(resolution.reference_symbol(expression).is_none());

    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains("unresolved name `missing`"))
    );
}

#[test]
fn resolve_reports_unknown_name_expression() {
    let source = source(
        r#"
        fn main(): null {
            var value = missing
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let expression = find_name_expression_by_text(syntax, &source, root, "missing").unwrap();

    assert!(resolution.reference_symbol(expression).is_none());

    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains("unresolved name `missing`"))
    );
}

#[test]
fn resolve_does_not_bind_builtin_type_as_value_name() {
    let source = source(
        r#"
        fn main(): null {
            var value = i8
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let expression = find_name_expression_by_text(syntax, &source, root, "i8").unwrap();

    assert!(resolution.reference_symbol(expression).is_none());
}

#[test]
fn resolve_binds_struct_anchor_function_path_expression_member() {
    let source = source(
        r#"
struct User {
  id: i64,
}

fn User::rename(user: User): User {
  return user
}

fn main(): null {
  var method = User::rename
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();
    let root = syntax.root().unwrap();

    let expression = find_path_expression_by_text(syntax, &source, root, "User::rename").unwrap();

    let root_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();

    let member_symbol = resolution
        .symbol(resolution.path_reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(root_symbol.name(), "User");
    assert_eq!(root_symbol.kind(), SymbolKind::Struct);

    assert_eq!(member_symbol.name(), "User::rename");
    assert_eq!(member_symbol.kind(), SymbolKind::Function);

    assert_eq!(
        resolution.path_reference_kind(expression),
        Some(PathReferenceKind::AnchorFunction)
    );
}
