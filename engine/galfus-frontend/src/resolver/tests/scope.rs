use super::*;

#[test]
fn resolve_creates_module_scope() {
    let source = source("const version = \"0.1.0\"");

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();

    assert_eq!(graph.phase(), GraphPhase::Resolved);

    let resolution = graph.resolution().unwrap();
    let module_scope = resolution.module_scope();

    assert!(!resolution.scopes().is_empty());
    assert_eq!(
        resolution.scope(module_scope).unwrap().kind(),
        ScopeKind::Module
    );
}

#[test]
fn resolve_creates_builtin_scope() {
    let source = source("const version = \"0.1.0\"");

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.graph();
    let resolution = graph.resolution().unwrap();

    let builtin_scope = resolution.builtin_scope().unwrap();
    let builtin_scope = resolution.scope(builtin_scope).unwrap();

    assert_eq!(builtin_scope.kind(), ScopeKind::Builtin);
    assert!(builtin_scope.symbol("i8").is_some());
    assert!(builtin_scope.symbol("i32").is_some());
    assert!(builtin_scope.symbol("f16").is_some());
    assert!(builtin_scope.symbol("String").is_none());
    assert!(builtin_scope.symbol("char").is_none());
}
