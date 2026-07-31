use super::*;

#[test]
fn resolve_binds_block_local_name_expression() {
    let source = source(
        r#"
        fn main(): null {
            var first = 1
            var second = first
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

    let expression = find_name_expression_by_text(syntax, &source, root, "first").unwrap();

    let symbol = resolution.reference_symbol(expression).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "first");
    assert_eq!(symbol.kind(), SymbolKind::Var);
}

#[test]
fn resolve_binds_name_from_parent_block_scope() {
    let source = source(
        r#"
        fn main(): null {
            var outer = 1

            if true {
                var inner = outer
            }

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

    let expression = find_name_expression_by_text(syntax, &source, root, "outer").unwrap();

    let symbol = resolution.reference_symbol(expression).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "outer");
    assert_eq!(symbol.kind(), SymbolKind::Var);
}

#[test]
fn resolve_prefers_nearest_scope_symbol() {
    let source = source(
        r#"
        fn main(): null {
            var value = 1

            if true {
                var value = 2
                var result = value
            }

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

    let expression = find_name_expression_by_text(syntax, &source, root, "value").unwrap();

    let referenced_symbol_id = resolution.reference_symbol(expression).unwrap();
    let referenced_symbol = resolution.symbol(referenced_symbol_id).unwrap();

    let function = syntax.first_child(root).unwrap();

    let outer_block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let if_statement = syntax
        .first_child_of_kind(outer_block, SyntaxNodeKind::IfStatement)
        .unwrap();

    let inner_block = syntax
        .first_child_of_kind(if_statement, SyntaxNodeKind::Block)
        .unwrap();

    let inner_block_scope = resolution.node_scope(inner_block).unwrap();

    assert_eq!(referenced_symbol.name(), "value");
    assert_eq!(referenced_symbol.kind(), SymbolKind::Var);
    assert_eq!(referenced_symbol.scope(), inner_block_scope);
}

#[test]
fn resolve_binds_import_namespace_name_expression() {
    let source = source(
        r#"
        import user from "./user"

        fn main(): null {
            var current = user
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

    let expression = find_name_expression_by_text(syntax, &source, root, "user").unwrap();

    let symbol = resolution.reference_symbol(expression).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "user");
    assert_eq!(symbol.kind(), SymbolKind::ImportNamespace);
}

#[test]
fn resolve_binds_import_namespace_path_expression_root() {
    let source = source(
        r#"
        import user from "./user"

        fn main(): null {
            var created = user::create()
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

    let expression = find_path_expression_by_text(syntax, &source, root, "user::create").unwrap();

    let symbol = resolution.reference_symbol(expression).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "user");
    assert_eq!(symbol.kind(), SymbolKind::ImportNamespace);
}

#[test]
fn resolve_binds_local_path_expression_root() {
    let source = source(
        r#"
        fn main(): null {
            var local = 0
            var current = local::member
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

    let expression = find_path_expression_by_text(syntax, &source, root, "local::member").unwrap();

    let symbol = resolution.reference_symbol(expression).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "local");
    assert_eq!(symbol.kind(), SymbolKind::Var);
}

#[test]
fn resolve_binds_enum_variant_path_expression_member() {
    let source = source(
        r#"
        enum Status {
            Off,
            On,
        }

        fn main(): null {
            var current = Status::On
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

    let expression = find_path_expression_by_text(syntax, &source, root, "Status::On").unwrap();

    let root_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();
    let member_symbol = resolution
        .symbol(resolution.path_reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(root_symbol.name(), "Status");
    assert_eq!(root_symbol.kind(), SymbolKind::Enum);

    assert_eq!(member_symbol.name(), "On");
    assert_eq!(member_symbol.kind(), SymbolKind::EnumVariant);

    assert_eq!(
        resolution.path_reference_kind(expression),
        Some(PathReferenceKind::EnumVariant)
    );
}

#[test]
fn resolve_binds_choice_variant_path_expression_member() {
    let source = source(
        r#"
        choice Result<V, F> {
            Ok(V),
            Err(F),
        }

        fn main(value: i32): null {
            var current = Result::Ok
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

    let expression = find_path_expression_by_text(syntax, &source, root, "Result::Ok").unwrap();

    let member_symbol = resolution
        .symbol(resolution.path_reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(member_symbol.name(), "Ok");
    assert_eq!(member_symbol.kind(), SymbolKind::ChoiceVariant);

    assert_eq!(
        resolution.path_reference_kind(expression),
        Some(PathReferenceKind::ChoiceVariant)
    );
}

#[test]
fn resolve_binds_constraint_function_path_expression_member() {
    let source = source(
        r#"
        constraint Stringable<T> {
            fn toString(self): [i8]
        }

        fn main(): null {
            var method = Stringable::toString
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

    let expression =
        find_path_expression_by_text(syntax, &source, root, "Stringable::toString").unwrap();

    let root_symbol = resolution
        .symbol(resolution.reference_symbol(expression).unwrap())
        .unwrap();
    let member_symbol = resolution
        .symbol(resolution.path_reference_symbol(expression).unwrap())
        .unwrap();

    assert_eq!(root_symbol.name(), "Stringable");
    assert_eq!(root_symbol.kind(), SymbolKind::Constraint);

    assert_eq!(member_symbol.name(), "toString");
    assert_eq!(member_symbol.kind(), SymbolKind::ConstraintFunction);

    assert_eq!(
        resolution.path_reference_kind(expression),
        Some(PathReferenceKind::ConstraintMember)
    );
}

#[test]
fn resolve_reports_unknown_path_expression_member_on_local_type() {
    let source = source(
        r#"
        enum Status {
            Off,
            On,
        }

        fn main(): null {
            var current = Status::Missing
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
        find_path_expression_by_text(syntax, &source, root, "Status::Missing").unwrap();

    assert!(resolution.reference_symbol(expression).is_some());
    assert!(resolution.path_reference_symbol(expression).is_none());

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message()
            .contains("unresolved path member `Missing`")
    }));
}
