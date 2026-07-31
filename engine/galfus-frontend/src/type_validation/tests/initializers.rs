use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_matching_var_initializer_type() {
    let (_source, _graph, result) = check_source(
        r#"
var age: i32 = 10
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_contextual_integer_initializer_type() {
    let (source, graph, result) = check_source(
        r#"
var byte: u8 = 27
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
fn check_reports_integer_initializer_out_of_range() {
    let source = source(
        r#"
var byte: u8 = 300
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
fn check_accepts_signed_integer_initializer_lower_bound() {
    let (_source, _graph, result) = check_source(
        r#"
var byte: i8 = -128
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_signed_integer_initializer_below_lower_bound() {
    let source = source(
        r#"
var byte: i8 = -129
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
                .contains("integer literal `-129` does not fit `i8`")
    }));
}

#[test]
fn check_reports_var_initializer_type_mismatch() {
    let source = source(
        r#"
var age: i32 = true
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
fn check_accepts_matching_const_initializer_type() {
    let (_source, _graph, result) = check_source(
        r#"
const enabled: bool = true
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_null_initializer_for_nullable_union() {
    let (_source, _graph, result) = check_source(
        r#"
var maybe: i32 | null = null
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_null_initializer_for_non_nullable_type() {
    let source = source(
        r#"
var age: i32 = null
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
            && diagnostic.message().contains("expected `i32`, got `null`")
    }));
}

#[test]
fn check_reports_top_level_initialization_cycle() {
    let source = source(
        r#"
var a = b
var b = a
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InitializationCycle.as_code()
            && diagnostic.message().contains("initialization cycle")
    }));
}

#[test]
fn check_reports_local_initialization_cycle() {
    let source = source(
        r#"
fn main(): null {
  var a = b
  var b = a
  return
}
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InitializationCycle.as_code()
    }));
}

#[test]
fn check_accepts_name_initializer_with_matching_symbol_type() {
    let (_source, _graph, result) = check_source(
        r#"
var first: i32 = 10
var second: i32 = first
"#,
    );

    assert!(!result.has_errors());
}
