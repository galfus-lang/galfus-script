use super::*;

#[test]
fn resolve_binds_choice_generic_parameter_type_references() {
    let source = source(
        r#"
        choice Result<V, F> {
            Ok(V),
            Err(F),
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

    let v_type = find_named_type_by_text(syntax, &source, root, "V").unwrap();
    let f_type = find_named_type_by_text(syntax, &source, root, "F").unwrap();

    let v_symbol = resolution
        .symbol(resolution.type_reference_symbol(v_type).unwrap())
        .unwrap();

    let f_symbol = resolution
        .symbol(resolution.type_reference_symbol(f_type).unwrap())
        .unwrap();

    assert_eq!(v_symbol.kind(), SymbolKind::GenericParameter);
    assert_eq!(v_symbol.name(), "V");

    assert_eq!(f_symbol.kind(), SymbolKind::GenericParameter);
    assert_eq!(f_symbol.name(), "F");
}

#[test]
fn resolve_binds_constraint_generic_parameter_type_references() {
    let source = source(
        r#"
        constraint Stringable<T> {
            fn toString(self): T
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

    let t_type = find_named_type_by_text(syntax, &source, root, "T").unwrap();

    let t_symbol = resolution
        .symbol(resolution.type_reference_symbol(t_type).unwrap())
        .unwrap();

    assert_eq!(t_symbol.kind(), SymbolKind::GenericParameter);
    assert_eq!(t_symbol.name(), "T");
}
