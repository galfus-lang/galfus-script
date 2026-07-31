use super::*;

#[test]
fn resolve_declares_top_level_named_items() {
    let source = source(
        "
        fn main(): null { return }

        type UserId = i64

        struct User {
            id: UserId,
        }

        enum Status {
            Off,
            On,
        }

        choice Result<V, F> {
            Ok(V),
            Err(F),
        }

        constraint Stringable<T> {
            fn toString(self): [i8]
        }
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope_id = resolution.module_scope();
    let module_scope = resolution.scope(module_scope_id).unwrap();

    assert!(module_scope.symbol("main").is_some());
    assert!(module_scope.symbol("UserId").is_some());
    assert!(module_scope.symbol("User").is_some());
    assert!(module_scope.symbol("Status").is_some());
    assert!(module_scope.symbol("Result").is_some());
    assert!(module_scope.symbol("Stringable").is_some());

    let main = resolution
        .symbol(module_scope.symbol("main").unwrap())
        .unwrap();

    assert_eq!(main.kind(), SymbolKind::Function);

    let user = resolution
        .symbol(module_scope.symbol("User").unwrap())
        .unwrap();

    assert_eq!(user.kind(), SymbolKind::Struct);
}

#[test]
fn resolve_declares_top_level_var_and_const() {
    let source = source(
        "
        var counter = 0
        const version = \"0.1.0\"
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    let counter = resolution
        .symbol(module_scope.symbol("counter").unwrap())
        .unwrap();

    let version = resolution
        .symbol(module_scope.symbol("version").unwrap())
        .unwrap();

    assert_eq!(counter.kind(), SymbolKind::Var);
    assert_eq!(version.kind(), SymbolKind::Const);
}

#[test]
fn resolve_declares_top_level_destructuring_bindings() {
    let source = source(
        "
        var user = 0
        var point = 0
        var values = 0

        var { id, name: userName } = user
        var (x, y) = point
        var [first, ...rest] = values
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    assert!(module_scope.symbol("id").is_some());
    assert!(module_scope.symbol("userName").is_some());

    assert!(module_scope.symbol("x").is_some());
    assert!(module_scope.symbol("y").is_some());

    assert!(module_scope.symbol("first").is_some());
    assert!(module_scope.symbol("rest").is_some());

    assert!(module_scope.symbol("name").is_none());
}

#[test]
fn resolve_declares_exported_item_in_module_scope() {
    let source = source(
        "
        export fn main(): null {
            return
        }
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    assert!(module_scope.symbol("main").is_some());
}

#[test]
fn resolve_binds_declaration_identifier_to_symbol() {
    let source = source(
        "
        fn main(): null {
            return
        }
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let function = syntax.first_child(root).unwrap();

    let name = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Identifier)
        .unwrap();

    let symbol = resolution.declaration_symbol(name).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "main");
    assert_eq!(symbol.kind(), SymbolKind::Function);
}

#[test]
fn resolve_declares_anchored_function_by_qualified_name() {
    let source = source(
        "
        struct User {
            name: [i8],
        }

        fn User::rename(self, name: [i8]): User {
            return self
        }
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    assert!(module_scope.symbol("rename").is_none());

    let symbol = resolution
        .symbol(module_scope.symbol("User::rename").unwrap())
        .unwrap();

    assert_eq!(symbol.name(), "User::rename");
    assert_eq!(symbol.kind(), SymbolKind::Function);
}

#[test]
fn resolve_allows_same_anchored_function_name_on_different_structs() {
    let source = source(
        "
        struct User {
            name: [i8],
        }

        struct Post {
            title: [i8],
        }

        fn User::rename(self, name: [i8]): User {
            return self
        }

        fn Post::rename(self, title: [i8]): Post {
            return self
        }
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    assert!(module_scope.symbol("User::rename").is_some());
    assert!(module_scope.symbol("Post::rename").is_some());
}

#[test]
fn resolve_reports_duplicate_top_level_symbol() {
    let source = source(
        "
        fn main(): null {
            return
        }

        fn main(): null {
            return
        }
        ",
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.scope(resolution.module_scope()).unwrap();

    assert!(module_scope.symbol("main").is_some());

    let main_symbols = resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "main")
        .count();

    assert_eq!(main_symbols, 1);
}
