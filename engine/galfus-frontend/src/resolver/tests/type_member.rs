use super::*;

#[test]
fn resolve_declares_struct_field_symbols_in_struct_scope() {
    let source = source(
        r#"
        struct User {
            id: i64,
            name: [u8],
            weak parent: User | null = null,
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
    let struct_item = syntax.first_child(root).unwrap();
    let struct_scope = resolution
        .scope(resolution.node_scope(struct_item).unwrap())
        .unwrap();

    let id = resolution
        .symbol(struct_scope.symbol("id").unwrap())
        .unwrap();
    let name = resolution
        .symbol(struct_scope.symbol("name").unwrap())
        .unwrap();
    let parent = resolution
        .symbol(struct_scope.symbol("parent").unwrap())
        .unwrap();

    assert_eq!(id.kind(), SymbolKind::StructField);
    assert_eq!(name.kind(), SymbolKind::StructField);
    assert_eq!(parent.kind(), SymbolKind::StructField);
}

#[test]
fn resolve_declares_enum_variant_symbols_in_enum_scope() {
    let source = source(
        r#"
        enum Status {
            Off,
            On(1),
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
    let enum_item = syntax.first_child(root).unwrap();
    let enum_scope = resolution
        .scope(resolution.node_scope(enum_item).unwrap())
        .unwrap();

    let off = resolution
        .symbol(enum_scope.symbol("Off").unwrap())
        .unwrap();
    let on = resolution.symbol(enum_scope.symbol("On").unwrap()).unwrap();

    assert_eq!(off.kind(), SymbolKind::EnumVariant);
    assert_eq!(on.kind(), SymbolKind::EnumVariant);
}

#[test]
fn resolve_declares_choice_variant_symbols_in_choice_scope() {
    let source = source(
        r#"
        choice Result<V, F> {
            Ok(V),
            Err(F),
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
    let choice_item = syntax.first_child(root).unwrap();
    let choice_scope = resolution
        .scope(resolution.node_scope(choice_item).unwrap())
        .unwrap();

    let ok = resolution
        .symbol(choice_scope.symbol("Ok").unwrap())
        .unwrap();
    let err = resolution
        .symbol(choice_scope.symbol("Err").unwrap())
        .unwrap();

    assert_eq!(ok.kind(), SymbolKind::ChoiceVariant);
    assert_eq!(err.kind(), SymbolKind::ChoiceVariant);
}

#[test]
fn resolve_declares_constraint_member_symbols_in_constraint_scope() {
    let source = source(
        r#"
        constraint Entity<T> {
            id: i64,
            fn toString(self): [i8]
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
    let constraint_item = syntax.first_child(root).unwrap();
    let constraint_scope = resolution
        .scope(resolution.node_scope(constraint_item).unwrap())
        .unwrap();

    let id = resolution
        .symbol(constraint_scope.symbol("id").unwrap())
        .unwrap();
    let to_string = resolution
        .symbol(constraint_scope.symbol("toString").unwrap())
        .unwrap();

    assert_eq!(id.kind(), SymbolKind::ConstraintField);
    assert_eq!(to_string.kind(), SymbolKind::ConstraintFunction);
}

#[test]
fn resolve_reports_duplicate_struct_field_symbol() {
    let source = source(
        r#"
        struct User {
            id: i64,
            id: i32,
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();

    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains("duplicate symbol `id`"))
    );
}

#[test]
fn resolve_reports_duplicate_constraint_member_symbol() {
    let source = source(
        r#"
        constraint Entity {
            id: i64,
            id: i32,
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();

    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains("duplicate symbol `id`"))
    );
}

#[test]
fn resolve_type_member_symbols_do_not_shadow_type_references() {
    let source = source(
        r#"
        struct Node {
            Node: Node,
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(!resolve_result.has_errors());
}
