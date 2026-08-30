use super::*;

#[test]
fn resolve_reports_unknown_type_path_root() {
    let source = source(
        r#"
        type LocalUser = missing::User
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let path_type = find_path_type_by_text(syntax, &source, root, "missing::User").unwrap();

    assert!(resolution.type_reference_symbol(path_type).is_none());
}

#[test]
fn resolve_reports_value_symbol_as_invalid_type_path_root() {
    let source = source(
        r#"
        const user = 0

        type LocalUser = user::User
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let path_type = find_path_type_by_text(syntax, &source, root, "user::User").unwrap();

    assert!(resolution.type_reference_symbol(path_type).is_none());
}

#[test]
fn resolve_reports_unknown_named_type() {
    let source = source(
        r#"
        fn main(value: MissingType): null {
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);

    assert!(resolve_result.has_errors());

    let graph = resolve_result.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution().unwrap();

    let root = syntax.root().unwrap();

    let named_type = find_named_type_by_text(syntax, &source, root, "MissingType").unwrap();

    assert!(resolution.type_reference_symbol(named_type).is_none());
}

#[test]
fn resolve_reports_excluded_primitive_names_as_unknown_types() {
    let source = source(
        r#"
        fn main(text: String, ch: char): null {
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);

    assert!(resolve_result.has_errors());
}

#[test]
fn resolve_reports_f16_as_an_unknown_type() {
    let source = source(
        r#"
        fn main(value: f16): null {
            return
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);

    assert!(resolve_result.has_errors());
}
