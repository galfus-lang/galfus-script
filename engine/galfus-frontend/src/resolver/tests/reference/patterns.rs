use super::*;

#[test]
fn resolve_declares_instanceof_type_pattern_binding_in_arm_scope() {
    let source = source(
        r#"
        fn main(value: i32 | null): i32 {
            return instanceof value {
                i32 count => count,
                _ => 0,
            }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let arm = find_node_by_kind_and_text(
        syntax,
        &source,
        root,
        SyntaxNodeKind::InstanceofArm,
        "i32 count => count",
    )
    .unwrap();

    let arm_scope = resolution
        .scope(resolution.node_scope(arm).unwrap())
        .unwrap();
    let binding_symbol = resolution
        .symbol(arm_scope.symbol(string_table.intern("count")).unwrap())
        .unwrap();

    assert_eq!(arm_scope.kind(), ScopeKind::InstanceofArm);
    assert_eq!(binding_symbol.kind(), SymbolKind::TypePatternBinding);

    let expression = find_name_expression_by_text(syntax, &source, root, "count").unwrap();
    let reference_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(
        string_table.resolve(reference_symbol.name()).unwrap_or(""),
        "count"
    );
    assert_eq!(reference_symbol.kind(), SymbolKind::TypePatternBinding);
}

#[test]
fn resolve_declares_instanceof_fallback_binding_in_arm_scope() {
    let source = source(
        r#"
        fn main(value: i32): i32 {
            return instanceof value {
                other => other,
            }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let arm = find_node_by_kind_and_text(
        syntax,
        &source,
        root,
        SyntaxNodeKind::InstanceofArm,
        "other => other",
    )
    .unwrap();

    let arm_scope = resolution
        .scope(resolution.node_scope(arm).unwrap())
        .unwrap();
    let binding_symbol = resolution
        .symbol(arm_scope.symbol(string_table.intern("other")).unwrap())
        .unwrap();

    assert_eq!(binding_symbol.kind(), SymbolKind::PatternBinding);

    let expression = find_name_expression_by_text(syntax, &source, root, "other").unwrap();
    let reference_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(
        string_table.resolve(reference_symbol.name()).unwrap_or(""),
        "other"
    );
    assert_eq!(reference_symbol.kind(), SymbolKind::PatternBinding);
}

#[test]
fn resolve_instanceof_type_pattern_binding_reaches_block_arm_body() {
    let source = source(
        r#"
        fn main(value: i32 | null): i32 {
            instanceof value {
                i32 count {
                    return count
                },
                _ {
                    return 0
                }
            }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let expression = find_name_expression_by_text(syntax, &source, root, "count").unwrap();
    let reference_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(
        string_table.resolve(reference_symbol.name()).unwrap_or(""),
        "count"
    );
    assert_eq!(reference_symbol.kind(), SymbolKind::TypePatternBinding);
}

#[test]
fn resolve_declares_match_binding_pattern_in_arm_scope() {
    let source = source(
        r#"
        fn main(value: i32): i32 {
            return match value {
                other => other,
            }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let arm = find_node_by_kind_and_text(
        syntax,
        &source,
        root,
        SyntaxNodeKind::MatchArm,
        "other => other",
    )
    .unwrap();

    let arm_scope = resolution
        .scope(resolution.node_scope(arm).unwrap())
        .unwrap();
    let binding_symbol = resolution
        .symbol(arm_scope.symbol(string_table.intern("other")).unwrap())
        .unwrap();

    assert_eq!(binding_symbol.kind(), SymbolKind::PatternBinding);

    let expression = find_name_expression_by_text(syntax, &source, root, "other").unwrap();
    let reference_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(
        string_table.resolve(reference_symbol.name()).unwrap_or(""),
        "other"
    );
    assert_eq!(reference_symbol.kind(), SymbolKind::PatternBinding);
}

#[test]
fn resolve_declares_choice_payload_binding_in_arm_scope() {
    let source = source(
        r#"
        choice Result<V, F> {
            Ok(V),
            Err(F),
        }

        fn main(result: Result<i32, [i8]>): i32 {
            return match result {
                Result::Ok(value) => value,
                Result::Err(error) => 0,
            }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let arm = find_node_by_kind_and_text(
        syntax,
        &source,
        root,
        SyntaxNodeKind::MatchArm,
        "Result::Ok(value) => value",
    )
    .unwrap();

    let arm_scope = resolution
        .scope(resolution.node_scope(arm).unwrap())
        .unwrap();
    let binding_symbol = resolution
        .symbol(arm_scope.symbol(string_table.intern("value")).unwrap())
        .unwrap();

    assert_eq!(binding_symbol.kind(), SymbolKind::PatternBinding);

    let expression = find_name_expression_by_text(syntax, &source, root, "value").unwrap();
    let reference_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(
        string_table.resolve(reference_symbol.name()).unwrap_or(""),
        "value"
    );
    assert_eq!(reference_symbol.kind(), SymbolKind::PatternBinding);
}

#[test]
fn resolve_binds_variant_pattern_member() {
    let source = source(
        r#"
        enum Color {
            Red,
            Blue,
        }

        fn main(color: Color): i32 {
            return match color {
                Color::Red => 1,
                Color::Blue => 2,
            }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let pattern = find_node_by_kind_and_text(
        syntax,
        &source,
        root,
        SyntaxNodeKind::VariantPattern,
        "Color::Red",
    )
    .unwrap();

    let root_symbol = resolution
        .symbol(resolution.reference_symbol(pattern).unwrap())
        .unwrap();
    let member_symbol = resolution
        .symbol(resolution.path_reference_symbol(pattern).unwrap())
        .unwrap();

    assert_eq!(
        string_table.resolve(root_symbol.name()).unwrap_or(""),
        "Color"
    );
    assert_eq!(root_symbol.kind(), SymbolKind::Enum);

    assert_eq!(
        string_table.resolve(member_symbol.name()).unwrap_or(""),
        "Red"
    );
    assert_eq!(member_symbol.kind(), SymbolKind::EnumVariant);
}

#[test]
fn resolve_reports_unknown_variant_pattern_member() {
    let source = source(
        r#"
        enum Color {
            Red,
        }

        fn main(color: Color): i32 {
            return match color {
                Color::Blue => 2,
            }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message()
            .contains("unresolved path member `Blue`")
    }));
}
