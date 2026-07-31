use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_int_array_literal() {
    let (_source, _graph, result) = check_source(
        r#"
var values: [i32] = [1, 2, 3]
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_contextual_integer_array_element_type() {
    let (source, graph, result) = check_source(
        r#"
var bytes: [u8] = [27, 91]
"#,
    );

    assert!(!result.has_errors());

    let literal =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::IntegerLiteral, "27").unwrap();
    let ty = result.layer().node_type(literal).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Uint8))
    );
}

#[test]
fn check_reports_integer_array_element_out_of_range() {
    let source = source(
        r#"
var bytes: [u8] = [27, 300]
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic
                .message()
                .contains("integer literal `300` does not fit `u8`")
    }));
}

#[test]
fn check_binds_array_literal_type() {
    let (source, graph, result) = check_source(
        r#"
var values: [i32] = [1, 2, 3]
"#,
    );

    let array =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::ArrayLiteral, "[1, 2, 3]")
            .unwrap();

    let ty = result.layer().node_type(array).unwrap();

    match result.layer().table().kind(ty) {
        Some(TypeKind::Array { element }) => {
            assert_eq!(
                result.layer().table().kind(*element),
                Some(&TypeKind::Primitive(PrimitiveType::Int32))
            );
        }

        other => panic!("expected array type, got {other:?}"),
    }
}

#[test]
fn check_reports_mixed_array_literal_element_type() {
    let source = source(
        r#"
var values: [i32] = [1, true]
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic.message().contains("expected `i32`, got `bool`")
    }));
}

#[test]
fn check_accepts_bool_array_literal() {
    let (_source, _graph, result) = check_source(
        r#"
var values: [bool] = [true, false]
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_string_literal_as_uint8_array() {
    let (_source, _graph, result) = check_source(
        r#"
var name: [u8] = "Renato"
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_string_literal_type_as_uint8_array() {
    let (source, graph, result) = check_source(
        r#"
var name: [u8] = "Renato"
"#,
    );

    let string =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::StringLiteral, "\"Renato\"")
            .unwrap();

    let ty = result.layer().node_type(string).unwrap();

    match result.layer().table().kind(ty) {
        Some(TypeKind::Array { element }) => {
            assert_eq!(
                result.layer().table().kind(*element),
                Some(&TypeKind::Primitive(PrimitiveType::Uint8))
            );
        }

        other => panic!("expected [u8], got {other:?}"),
    }
}

#[test]
fn check_accepts_tuple_literal() {
    let (_source, _graph, result) = check_source(
        r#"
var point: (i32, bool) = (1, true)
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_contextual_integer_tuple_element_type() {
    let (source, graph, result) = check_source(
        r#"
var pair: (u8, u16) = (27, 300)
"#,
    );

    assert!(!result.has_errors());

    let first =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::IntegerLiteral, "27").unwrap();
    let first_ty = result.layer().node_type(first).unwrap();
    assert_eq!(
        result.layer().table().kind(first_ty),
        Some(&TypeKind::Primitive(PrimitiveType::Uint8))
    );

    let second =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::IntegerLiteral, "300").unwrap();
    let second_ty = result.layer().node_type(second).unwrap();
    assert_eq!(
        result.layer().table().kind(second_ty),
        Some(&TypeKind::Primitive(PrimitiveType::Uint16))
    );
}

#[test]
fn check_accepts_array_literal_spread() {
    let (_source, _graph, result) = check_source(
        r#"
var base: [i32] = [1, 2]
var values: [i32] = [0, ...base, 3]
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_string_literal_spread() {
    let (_source, _graph, result) = check_source(
        r#"
var values: [u8] = [..."Hello", ..."Galfus!", ..."\n"]
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_invalid_array_literal_spread_target() {
    let source = source(
        r#"
var base: i32 = 1
var values: [i32] = [0, ...base, 3]
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidSpreadTarget.as_code()
            && diagnostic
                .message()
                .contains("spread target must be an array")
    }));
}

#[test]
fn check_reports_empty_array_literal() {
    let source = source(
        r#"
var values = []
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::EmptyArrayLiteral.as_code()
            && diagnostic
                .message()
                .contains("empty array literal is not allowed")
    }));
}

#[test]
fn check_accepts_empty_string_literal_as_dynamic_uint8_array() {
    let (_source, _graph, result) = check_source(
        r#"
var value: [u8] = ""
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_dynamic_array_literal_spread() {
    let (_source, _graph, result) = check_source(
        r#"
var base: [i32] = [1, 2]
var values: [i32] = [0, ...base, 3]
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_builtin_int_and_float_union_arrays() {
    let (_source, _graph, result) = check_source(
        r#"
var ints: [int] = [1, 2, 3]
var uints: [uint] = [<u32>1, <u32>2, <u32>3]
var floats: [float] = [1.0, 2.0, 3.0]
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_union_array_literal_with_expected_type() {
    let (_source, _graph, result) = check_source(
        r#"
var values: [i32] = [1, 2, 3]
"#,
    );

    assert!(!result.has_errors());
}
