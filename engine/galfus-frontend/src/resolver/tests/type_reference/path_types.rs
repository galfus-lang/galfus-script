use super::*;

#[test]
fn resolve_binds_struct_field_named_type() {
    let source = source(
        r#"
        struct Profile {
            bio: [i8],
        }

        struct User {
            profile: Profile,
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

    let named_type = find_named_type_by_text(syntax, &source, root, "Profile").unwrap();

    let symbol = resolution.type_reference_symbol(named_type).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "Profile");
    assert_eq!(symbol.kind(), SymbolKind::Struct);
}

#[test]
fn resolve_binds_builtin_named_types() {
    let source = source(
        r#"
        fn main(value: i32): [i8] {
            return "ok"
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

    let int_type = find_named_type_by_text(syntax, &source, root, "i32").unwrap();
    let int8_type = find_named_type_by_text(syntax, &source, root, "i8").unwrap();

    let int_symbol = resolution
        .symbol(resolution.type_reference_symbol(int_type).unwrap())
        .unwrap();

    let int8_symbol = resolution
        .symbol(resolution.type_reference_symbol(int8_type).unwrap())
        .unwrap();

    assert_eq!(int_symbol.kind(), SymbolKind::BuiltinType);
    assert_eq!(int_symbol.name(), "i32");

    assert_eq!(int8_symbol.kind(), SymbolKind::BuiltinType);
    assert_eq!(int8_symbol.name(), "i8");
}

#[test]
fn resolve_binds_import_namespace_type_path_root() {
    let source = source(
        r#"
        import user from "./user"

        type LocalUser = user::User
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

    let path_type = find_path_type_by_text(syntax, &source, root, "user::User").unwrap();

    let symbol = resolution.type_reference_symbol(path_type).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "user");
    assert_eq!(symbol.kind(), SymbolKind::ImportNamespace);
}

#[test]
fn resolve_binds_local_type_path_root() {
    let source = source(
        r#"
        struct User {
            Id: i32,
        }

        type LocalUserId = User::Id
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

    let path_type = find_path_type_by_text(syntax, &source, root, "User::Id").unwrap();

    let symbol = resolution.type_reference_symbol(path_type).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "User");
    assert_eq!(symbol.kind(), SymbolKind::Struct);
}

#[test]
fn resolve_binds_local_type_path_member() {
    let source = source(
        r#"
        struct User {
            Id: i32,
        }

        type LocalUserId = User::Id
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

    let path_type = find_path_type_by_text(syntax, &source, root, "User::Id").unwrap();

    let root_symbol = resolution
        .symbol(resolution.type_reference_symbol(path_type).unwrap())
        .unwrap();
    let member_symbol = resolution
        .symbol(resolution.type_path_reference_symbol(path_type).unwrap())
        .unwrap();

    assert_eq!(root_symbol.name(), "User");
    assert_eq!(root_symbol.kind(), SymbolKind::Struct);

    assert_eq!(member_symbol.name(), "Id");
    assert_eq!(member_symbol.kind(), SymbolKind::StructField);
}

#[test]
fn resolve_binds_constraint_field_type_path_member() {
    let source = source(
        r#"
        constraint Entity {
            Id: i64,
        }

        type EntityId = Entity::Id
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

    let path_type = find_path_type_by_text(syntax, &source, root, "Entity::Id").unwrap();

    let root_symbol = resolution
        .symbol(resolution.type_reference_symbol(path_type).unwrap())
        .unwrap();
    let member_symbol = resolution
        .symbol(resolution.type_path_reference_symbol(path_type).unwrap())
        .unwrap();

    assert_eq!(root_symbol.name(), "Entity");
    assert_eq!(root_symbol.kind(), SymbolKind::Constraint);

    assert_eq!(member_symbol.name(), "Id");
    assert_eq!(member_symbol.kind(), SymbolKind::ConstraintField);
}

#[test]
fn resolve_reports_unknown_local_type_path_member() {
    let source = source(
        r#"
        struct User {
            Id: i32,
        }

        type LocalUserName = User::Name
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

    let path_type = find_path_type_by_text(syntax, &source, root, "User::Name").unwrap();

    assert!(resolution.type_reference_symbol(path_type).is_some());
    assert!(resolution.type_path_reference_symbol(path_type).is_none());

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message()
            .contains("unresolved type path member `Name`")
    }));
}
