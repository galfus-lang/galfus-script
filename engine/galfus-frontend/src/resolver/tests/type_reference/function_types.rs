use super::*;

#[test]
fn resolve_binds_function_parameter_named_type() {
    let source = source(
        r#"
        struct User {
            name: [i8],
        }

        fn greet(user: User): null {
            return
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

    let named_type = find_named_type_by_text(syntax, &source, root, "User").unwrap();

    let symbol = resolution.type_reference_symbol(named_type).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(string_table.resolve(symbol.name()).unwrap_or(""), "User");
    assert_eq!(symbol.kind(), SymbolKind::Struct);
}

#[test]
fn resolve_accepts_struct_function_anchor() {
    let source = source(
        r#"
        struct User {
            name: [i8],
        }

        fn User::rename(self, name: [i8]): User {
            return self
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
    let anchor = find_function_anchor_by_text(syntax, &source, root, "User").unwrap();
    let anchor_type = syntax.first_child(anchor).unwrap();

    let symbol = resolution
        .symbol(resolution.type_reference_symbol(anchor_type).unwrap())
        .unwrap();

    assert_eq!(string_table.resolve(symbol.name()).unwrap_or(""), "User");
    assert_eq!(symbol.kind(), SymbolKind::Struct);
}

#[test]
fn resolve_reports_non_struct_function_anchor() {
    let source = source(
        r#"
        choice Result<V, F> {
            Ok(V),
            Err(F),
        }

        fn Result::map(self): Result<i32, [i8]> {
            return self
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
        diagnostic.code().as_str() == "S0004"
            && diagnostic
                .message()
                .contains("function anchor `Result` must be a struct")
    }));
}

#[test]
fn resolve_binds_function_return_named_type() {
    let source = source(
        r#"
        struct User {
            name: [i8],
        }

        fn create(): User {
            return new(User) { name: "Ana" }
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

    let named_type = find_named_type_by_text(syntax, &source, root, "User").unwrap();

    let symbol = resolution.type_reference_symbol(named_type).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(string_table.resolve(symbol.name()).unwrap_or(""), "User");
    assert_eq!(symbol.kind(), SymbolKind::Struct);
}

#[test]
fn resolve_binds_function_expression_signature_named_types() {
    let source = source(
        r#"
        struct User {
            name: [i8],
        }

        fn main(): null {
            const identity = fn (user: User): User => user
            return
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
    let mut user_types = Vec::new();
    collect_named_types_by_text(syntax, &source, root, "User", &mut user_types);

    assert_eq!(user_types.len(), 2);

    for named_type in user_types {
        let symbol = resolution
            .symbol(resolution.type_reference_symbol(named_type).unwrap())
            .unwrap();

        assert_eq!(string_table.resolve(symbol.name()).unwrap_or(""), "User");
        assert_eq!(symbol.kind(), SymbolKind::Struct);
    }
}
