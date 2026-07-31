use super::*;

#[test]
fn resolve_creates_function_scope() {
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
    let function_scope = resolution.scope(function_scope).unwrap();

    assert_eq!(function_scope.kind(), ScopeKind::Function);
    assert_eq!(function_scope.parent(), Some(resolution.module_scope()));
}

#[test]
fn resolve_declares_function_parameters() {
    let source = source(
        r#"
        fn sum(a: i32, b: i32): i32 {
            return a + b
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

    let function_scope_id = resolution.node_scope(function).unwrap();
    let function_scope = resolution.scope(function_scope_id).unwrap();

    let a = resolution
        .symbol(function_scope.symbol("a").unwrap())
        .unwrap();
    let b = resolution
        .symbol(function_scope.symbol("b").unwrap())
        .unwrap();

    assert_eq!(a.kind(), SymbolKind::Parameter);
    assert_eq!(b.kind(), SymbolKind::Parameter);
    assert_eq!(a.name(), "a");
    assert_eq!(b.name(), "b");
}

#[test]
fn resolve_declares_rest_parameter() {
    let source = source(
        r#"
        fn log(...messages: [[i8]]): null {
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

    let function_scope_id = resolution.node_scope(function).unwrap();
    let function_scope = resolution.scope(function_scope_id).unwrap();

    let messages = resolution
        .symbol(function_scope.symbol("messages").unwrap())
        .unwrap();

    assert_eq!(messages.kind(), SymbolKind::RestParameter);
    assert_eq!(messages.name(), "messages");
}

#[test]
fn resolve_binds_parameter_declaration_to_symbol() {
    let source = source(
        r#"
        fn main(value: i32): null {
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

    let parameters = syntax
        .first_child_of_kind(function, SyntaxNodeKind::ParameterList)
        .unwrap();

    let parameter = syntax.first_child(parameters).unwrap();

    let binding = syntax
        .first_child_of_kind(parameter, SyntaxNodeKind::BindingPattern)
        .unwrap();

    let name = syntax.first_child(binding).unwrap();

    let symbol = resolution.declaration_symbol(name).unwrap();
    let symbol = resolution.symbol(symbol).unwrap();

    assert_eq!(symbol.name(), "value");
    assert_eq!(symbol.kind(), SymbolKind::Parameter);
}

#[test]
fn resolve_reports_duplicate_parameter() {
    let source = source(
        r#"
        fn main(value: i32, value: i32): null {
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

    let function_scope_id = resolution.node_scope(function).unwrap();
    let function_scope = resolution.scope(function_scope_id).unwrap();

    let value = resolution
        .symbol(function_scope.symbol("value").unwrap())
        .unwrap();

    assert_eq!(value.kind(), SymbolKind::Parameter);

    let value_count = resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "value")
        .count();

    assert_eq!(value_count, 1);
}

#[test]
fn resolve_creates_function_scope_for_exported_function() {
    let source = source(
        r#"
        export fn main(value: i32): null {
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
    let export_item = syntax.first_child(root).unwrap();
    let function = syntax.first_child(export_item).unwrap();

    let function_scope_id = resolution.node_scope(function).unwrap();
    let function_scope = resolution.scope(function_scope_id).unwrap();

    assert_eq!(function_scope.kind(), ScopeKind::Function);

    let value = resolution
        .symbol(function_scope.symbol("value").unwrap())
        .unwrap();

    assert_eq!(value.kind(), SymbolKind::Parameter);
}
