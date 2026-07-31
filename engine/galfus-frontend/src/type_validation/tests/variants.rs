use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_enum_variant_expression() {
    let (_source, _graph, result) = check_source(
        r#"
enum Direction {
  North,
  South,
}

var direction: Direction = Direction::North
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_enum_variant_expression_type() {
    let (source, graph, result) = check_source(
        r#"
enum Direction {
  North,
  South,
}

var direction: Direction = Direction::North
"#,
    );

    let path = find_node_by_kind_and_text(
        &source,
        &graph,
        SyntaxNodeKind::PathExpression,
        "Direction::North",
    )
    .unwrap();

    let direction_symbol = symbol_by_name_and_kind(&graph, "Direction", SymbolKind::Enum);
    let ty = result.layer().node_type(path).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Named {
            symbol: direction_symbol
        })
    );
}

#[test]
fn check_accepts_integer_enum_base_type() {
    let (_source, _graph, result) = check_source(
        r#"
enum(u8) Mode {
  Off(0),
  On(1),
}

var mode: Mode = Mode::On
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_non_integer_enum_base_type() {
    let source = source(
        r#"
enum(bool) Mode {
  Off,
  On,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidEnumBaseType.as_code()
            && diagnostic
                .message()
                .contains("enum base type must be an integer")
    }));
}

#[test]
fn check_reports_enum_discriminant_type_mismatch() {
    let source = source(
        r#"
enum(u8) Mode {
  Off(true),
  On(1),
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
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic.message().contains("expected `u8`, got `bool`")
    }));
}

#[test]
fn check_accepts_choice_variant_without_payload() {
    let (_source, _graph, result) = check_source(
        r#"
choice Asset {
  None,
  Texture([u8]),
}

var asset: Asset = Asset::None
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_choice_variant_with_payload() {
    let (_source, _graph, result) = check_source(
        r#"
choice Asset {
  None,
  Texture([u8]),
}

var asset: Asset = Asset::Texture("grass.png")
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_choice_variant_with_multiple_payload_items() {
    let (_source, _graph, result) = check_source(
        r#"
choice Asset {
  Image([u8], i32, i32),
}

var asset: Asset = Asset::Image("grass.png", 64, 64)
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_generic_choice_variant_inferred_from_payload() {
    let (source, graph, result) = check_source(
        r#"
choice Outcome<T> {
  Ok(T),
  Err([u8]),
}

var value = Outcome::Ok(42)
"#,
    );

    assert!(!result.has_errors());

    let call = find_node_by_kind_and_text(
        &source,
        &graph,
        SyntaxNodeKind::CallExpression,
        "Outcome::Ok(42)",
    )
    .unwrap();
    let ty = result.layer().node_type(call).unwrap();
    let outcome_symbol = symbol_by_name_and_kind(&graph, "Outcome", SymbolKind::Choice);
    let outcome_type = result.layer().symbol_type(outcome_symbol).unwrap();
    let int32_type = result.layer().table().primitive(PrimitiveType::Int32);

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::GenericInstance {
            base: outcome_type,
            arguments: vec![int32_type],
        })
    );
}

#[test]
fn check_accepts_generic_choice_variant_inferred_from_expected() {
    let (_source, _graph, result) = check_source(
        r#"
choice Outcome<T> {
  Ok(T),
  Err([u8]),
}

fn make(): Outcome<i32> {
  return Outcome::Ok(42)
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_choice_payload_required() {
    let source = source(
        r#"
choice Asset {
  Texture([u8]),
}

var asset: Asset = Asset::Texture
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ChoicePayloadRequired.as_code()
            && diagnostic.message().contains("requires a payload")
    }));
}

#[test]
fn check_reports_choice_payload_not_allowed() {
    let source = source(
        r#"
choice Asset {
  None,
}

var asset: Asset = Asset::None(1)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ChoicePayloadNotAllowed.as_code()
            && diagnostic.message().contains("does not accept a payload")
    }));
}

#[test]
fn check_reports_choice_payload_argument_count_mismatch() {
    let source = source(
        r#"
choice Asset {
  Image([u8], i32, i32),
}

var asset: Asset = Asset::Image("grass.png", 64)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ArgumentCountMismatch.as_code()
            && diagnostic.message().contains("expected 3 arguments, got 2")
    }));
}

#[test]
fn check_reports_choice_payload_type_mismatch() {
    let source = source(
        r#"
choice Asset {
  Image([u8], i32, i32),
}

var asset: Asset = Asset::Image("grass.png", true, 64)
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
