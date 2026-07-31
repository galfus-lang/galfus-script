use super::*;
use crate::ImportKind;

#[test]
fn resolve_declares_namespace_import_binding() {
    let source = source(
        r#"
        import user from "./user"

        fn main(): null {
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
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    let symbol = module_scope.symbol("user").unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.kind(), SymbolKind::ImportNamespace);
    assert_eq!(symbol.name(), "user");
}

#[test]
fn resolve_declares_named_import_bindings() {
    let source = source(
        r#"
        import { User, create } from "./user"

        fn main(): null {
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
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    let user = resolution
        .symbol(module_scope.symbol("User").unwrap())
        .unwrap();

    let create = resolution
        .symbol(module_scope.symbol("create").unwrap())
        .unwrap();

    assert_eq!(user.kind(), SymbolKind::ImportBinding);
    assert_eq!(create.kind(), SymbolKind::ImportBinding);
}

#[test]
fn resolve_declares_named_import_alias_binding() {
    let source = source(
        r#"
        import { User as LocalUser } from "./user"

        fn main(): null {
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
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    assert!(module_scope.symbol("User").is_none());

    let local_user = resolution
        .symbol(module_scope.symbol("LocalUser").unwrap())
        .unwrap();

    assert_eq!(local_user.kind(), SymbolKind::ImportBinding);
    assert_eq!(local_user.name(), "LocalUser");
}

#[test]
fn resolve_reports_duplicate_import_and_local_symbol() {
    let source = source(
        r#"
        fn user(): null {
            return
        }

        import user from "./user"
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    let symbol = resolution
        .symbol(module_scope.symbol("user").unwrap())
        .unwrap();

    assert_eq!(symbol.kind(), SymbolKind::Function);
}

#[test]
fn resolve_records_namespace_import() {
    let source = source(
        r#"
        import user from "./user"

        fn main(): null {
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

    assert_eq!(resolution.imports().len(), 1);

    let import = &resolution.imports()[0];

    assert_eq!(import.kind(), ImportKind::Namespace);
    assert_eq!(import.source(), "./user");
    assert_eq!(import.local_name(), "user");
    assert_eq!(import.imported_name(), None);

    let symbol = resolution.symbol(import.local_symbol()).unwrap();

    assert_eq!(symbol.kind(), SymbolKind::ImportNamespace);
    assert_eq!(symbol.name(), "user");

    assert_eq!(resolution.import_for_symbol(symbol.id()), Some(import.id()));
}

#[test]
fn resolve_records_named_imports() {
    let source = source(
        r#"
        import { User, create } from "./user"

        fn main(): null {
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

    assert_eq!(resolution.imports().len(), 2);

    let first = &resolution.imports()[0];
    let second = &resolution.imports()[1];

    assert_eq!(first.kind(), ImportKind::Named);
    assert_eq!(first.source(), "./user");
    assert_eq!(first.local_name(), "User");
    assert_eq!(first.imported_name(), Some("User"));

    assert_eq!(second.kind(), ImportKind::Named);
    assert_eq!(second.source(), "./user");
    assert_eq!(second.local_name(), "create");
    assert_eq!(second.imported_name(), Some("create"));
}

#[test]
fn resolve_records_named_import_alias() {
    let source = source(
        r#"
        import { User as LocalUser } from "./user"

        fn main(): null {
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

    assert_eq!(resolution.imports().len(), 1);

    let import = &resolution.imports()[0];

    assert_eq!(import.kind(), ImportKind::Named);
    assert_eq!(import.source(), "./user");
    assert_eq!(import.local_name(), "LocalUser");
    assert_eq!(import.imported_name(), Some("User"));

    let symbol = resolution.symbol(import.local_symbol()).unwrap();

    assert_eq!(symbol.kind(), SymbolKind::ImportBinding);
    assert_eq!(symbol.name(), "LocalUser");
}

#[test]
fn resolve_does_not_record_import_when_local_symbol_is_duplicate() {
    let source = source(
        r#"
        fn user(): null {
            return
        }

        import user from "./user"
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    assert_eq!(resolution.imports().len(), 0);

    let module_scope = resolution.scope(resolution.module_scope()).unwrap();
    let user_symbol = module_scope.symbol("user").unwrap();
    let user_symbol = resolution.symbol(user_symbol).unwrap();

    assert_eq!(user_symbol.kind(), SymbolKind::Function);
}
