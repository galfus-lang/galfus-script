use super::*;
use crate::{SymbolKind, parse, resolve};

#[test]
fn resolve_records_exported_function() {
    let source = source(
        r#"
        export fn create(): null {
            return
        }

        fn helper(): null {
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    assert_eq!(resolution.exports().len(), 1);

    let export = &resolution.exports()[0];

    assert_eq!(export.name(), "create");
    assert_eq!(export.kind(), SymbolKind::Function);

    let symbol = resolution.symbol(export.symbol()).unwrap();

    assert_eq!(symbol.name(), "create");
    assert_eq!(symbol.kind(), SymbolKind::Function);

    assert!(resolution.export_by_name("create").is_some());
    assert!(resolution.export_by_name("helper").is_none());
}

#[test]
fn resolve_records_exported_anchored_function_by_qualified_name() {
    let source = source(
        r#"
        struct User {
            name: [i8],
        }

        export fn User::rename(self, name: [i8]): User {
            return self
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    assert_eq!(resolution.exports().len(), 1);

    let export = &resolution.exports()[0];

    assert_eq!(export.name(), "User::rename");
    assert_eq!(export.kind(), SymbolKind::Function);
    assert!(resolution.export_by_name("User::rename").is_some());
    assert!(resolution.export_by_name("rename").is_none());
}

#[test]
fn resolve_records_exported_type_items() {
    let source = source(
        r#"
        export type UserId = i64

        export struct User {
            id: UserId,
        }

        export enum Status {
            Off,
            On,
        }

        export choice Result<V, F> {
            Ok(V),
            Err(F),
        }

        export constraint Stringable<T> {
            fn toString(self): [i8]
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    assert_eq!(resolution.exports().len(), 5);

    assert!(resolution.export_by_name("UserId").is_some());
    assert!(resolution.export_by_name("User").is_some());
    assert!(resolution.export_by_name("Status").is_some());
    assert!(resolution.export_by_name("Result").is_some());
    assert!(resolution.export_by_name("Stringable").is_some());

    let user = resolution
        .export_record(resolution.export_by_name("User").unwrap())
        .unwrap();

    assert_eq!(user.kind(), SymbolKind::Struct);
}

#[test]
fn resolve_records_exported_var_and_const() {
    let source = source(
        r#"
        export var counter = 0
        export const version = "0.1.0"
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    assert_eq!(resolution.exports().len(), 2);

    let counter = resolution
        .export_record(resolution.export_by_name("counter").unwrap())
        .unwrap();

    let version = resolution
        .export_record(resolution.export_by_name("version").unwrap())
        .unwrap();

    assert_eq!(counter.kind(), SymbolKind::Var);
    assert_eq!(version.kind(), SymbolKind::Const);
}

#[test]
fn resolve_records_exported_destructuring_bindings() {
    let source = source(
        r#"
        const user = 0

        export var { id, name: userName } = user
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    assert_eq!(resolution.exports().len(), 2);

    assert!(resolution.export_by_name("id").is_some());
    assert!(resolution.export_by_name("userName").is_some());

    assert!(resolution.export_by_name("name").is_none());
}

#[test]
fn resolve_does_not_export_non_exported_symbols() {
    let source = source(
        r#"
        fn privateFunction(): null {
            return
        }

        struct PrivateUser {
            name: [i8],
        }

        const privateVersion = "0.1.0"
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    assert_eq!(resolution.exports().len(), 0);

    assert!(resolution.export_by_name("privateFunction").is_none());
    assert!(resolution.export_by_name("PrivateUser").is_none());
    assert!(resolution.export_by_name("privateVersion").is_none());
}
