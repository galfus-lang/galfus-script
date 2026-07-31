use super::*;

#[test]
fn resolve_creates_function_body_block_scope() {
    let source = source(
        r#"
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
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();
    let function = syntax.first_child(root).unwrap();

    let function_scope = resolution.node_scope(function).unwrap();

    let block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let block_scope = resolution.node_scope(block).unwrap();
    let block_scope = resolution.scope(block_scope).unwrap();

    assert_eq!(block_scope.kind(), ScopeKind::Block);
    assert_eq!(block_scope.parent(), Some(function_scope));
}

#[test]
fn resolve_declares_var_and_const_in_block_scope() {
    let source = source(
        r#"
        fn main(): null {
            var total = 0
            const label = "value"
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
    let function = syntax.first_child(root).unwrap();

    let block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let block_scope_id = resolution.node_scope(block).unwrap();
    let block_scope = resolution.scope(block_scope_id).unwrap();

    let total = resolution
        .symbol(block_scope.symbol("total").unwrap())
        .unwrap();

    let label = resolution
        .symbol(block_scope.symbol("label").unwrap())
        .unwrap();

    assert_eq!(total.kind(), SymbolKind::Var);
    assert_eq!(label.kind(), SymbolKind::Const);
}

#[test]
fn resolve_declares_destructuring_bindings_in_block_scope() {
    let source = source(
        r#"
        fn main(): null {
            var user = 0
            var point = 0
            var values = 0

            var { id, name: userName } = user
            var (x, y) = point
            var [first, ...rest] = values

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
    let function = syntax.first_child(root).unwrap();

    let block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let block_scope = resolution
        .scope(resolution.node_scope(block).unwrap())
        .unwrap();

    assert!(block_scope.symbol("id").is_some());
    assert!(block_scope.symbol("userName").is_some());
    assert!(block_scope.symbol("x").is_some());
    assert!(block_scope.symbol("y").is_some());
    assert!(block_scope.symbol("first").is_some());
    assert!(block_scope.symbol("rest").is_some());

    assert!(block_scope.symbol("name").is_none());
}

#[test]
fn resolve_creates_nested_block_scope() {
    let source = source(
        r#"
        fn main(): null {
            if true {
                var inside = 1
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
    let function = syntax.first_child(root).unwrap();

    let outer_block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let outer_block_scope = resolution.node_scope(outer_block).unwrap();

    let if_statement = syntax.first_child(outer_block).unwrap();

    let inner_block = syntax
        .first_child_of_kind(if_statement, SyntaxNodeKind::Block)
        .unwrap();

    let inner_block_scope_id = resolution.node_scope(inner_block).unwrap();
    let inner_block_scope = resolution.scope(inner_block_scope_id).unwrap();

    assert_eq!(inner_block_scope.kind(), ScopeKind::Block);
    assert_eq!(inner_block_scope.parent(), Some(outer_block_scope));

    let inside = resolution
        .symbol(inner_block_scope.symbol("inside").unwrap())
        .unwrap();

    assert_eq!(inside.kind(), SymbolKind::Var);
}

#[test]
fn resolve_reports_duplicate_block_binding() {
    let source = source(
        r#"
        fn main(): null {
            var value = 1
            const value = 2
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
    let function = syntax.first_child(root).unwrap();

    let block = syntax
        .first_child_of_kind(function, SyntaxNodeKind::Block)
        .unwrap();

    let block_scope = resolution
        .scope(resolution.node_scope(block).unwrap())
        .unwrap();

    let value = resolution
        .symbol(block_scope.symbol("value").unwrap())
        .unwrap();

    assert_eq!(value.kind(), SymbolKind::Var);

    let count = resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "value")
        .count();

    assert_eq!(count, 1);
}
