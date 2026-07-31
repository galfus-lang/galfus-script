use super::*;
use crate::ResolverDiagnosticCode;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_instanceof_type_pattern_binding() {
    let (_source, _graph, result) = check_source(
        r#"
fn normalize(value: i32 | null): i32 {
  return instanceof value {
    i32 number => number,
    _ => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_instanceof_type_pattern_binding_type() {
    let (source, graph, result) = check_source(
        r#"
fn normalize(value: i32 | null): i32 {
  return instanceof value {
    i32 number => number,
    _ => 0,
  }
}
"#,
    );

    let binding = find_node_by_kind_and_text(
        &source,
        &graph,
        SyntaxNodeKind::TypePatternBinding,
        "number",
    )
    .unwrap();

    let ty = result.layer().node_type(binding).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_narrows_instanceof_alias_union_member_inside_larger_union() {
    let (source, graph, result) = check_source(
        r#"
type Parsed = i32 | bool | null

constraint Stringable {
  fn stringify(): [u8]
}

fn stringifyBool(value: bool): [u8] {
  return "bool"
}

fn stringify(value: Parsed | [u8] | Stringable): [u8] {
  return instanceof value {
    [u8] text => text,
    bool flag => stringifyBool(flag),
    null => "null",
    i32 n => "int",
    Stringable item => item::stringify(),
    _ => "unknown",
  }
}
"#,
    );

    let binding =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::TypePatternBinding, "flag")
            .unwrap();

    let ty = result.layer().node_type(binding).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Bool))
    );
}

#[test]
fn check_narrows_instanceof_subject_in_type_pattern_arm() {
    let (_source, _graph, result) = check_source(
        r#"
fn normalize(value: i32 | null): i32 {
  return instanceof value {
    i32 number => value,
    null => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_instanceof_type_pattern_binding_with_adjacent_name() {
    let (_source, _graph, result) = check_source(
        r#"
fn normalize(value: i32 | null): i32 {
  return instanceof value {
    i32 number => number,
    _ => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_instanceof_wildcard_pattern() {
    let (_source, _graph, result) = check_source(
        r#"
fn normalize(value: i32): i32 {
  return instanceof value {
    _ => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_instanceof_pattern_type_mismatch() {
    let source = source(
        r#"
fn normalize(value: i32): i32 {
  return instanceof value {
    bool flag => 1,
    _ => 0,
  }
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidInstanceofPatternType.as_code()
            && diagnostic.message().contains("got `bool`")
    }));
}

#[test]
fn check_reports_incompatible_instanceof_arm_type() {
    let source = source(
        r#"
fn normalize(value: i32 | null): i32 {
  return instanceof value {
    i32 number => true,
    _ => 0,
  }
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
        diagnostic.code().as_str() == TypeDiagnosticCode::IncompatibleInstanceofArmType.as_code()
            && diagnostic
                .message()
                .contains("instanceof arm body must be compatible with `i32`, got `bool`")
    }));
}

#[test]
fn check_accepts_instanceof_binding_pattern() {
    let (_source, _graph, result) = check_source(
        r#"
fn normalize(value: i32): i32 {
  return instanceof value {
    other => other,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_instanceof_fallback_binding_to_remaining_type() {
    let (source, graph, result) = check_source(
        r#"
fn normalize(value: i32 | i16 | null): null {
  return instanceof value {
    i32 number => null,
    i16 short => null,
    rest => rest,
  }
}
"#,
    );

    assert!(!result.has_errors());

    let binding =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::BindingPattern, "rest")
            .unwrap();

    let ty = result.layer().node_type(binding).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Null))
    );
}

#[test]
fn check_instanceof_wildcard_fallback_does_not_bind() {
    let (source, graph, result) = check_source(
        r#"
fn normalize(value: i32 | null): i32 {
  return instanceof value {
    i32 number => number,
    _ => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
    assert!(
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::BindingPattern, "_").is_none()
    );
}

#[test]
fn check_reports_instanceof_wildcard_subject() {
    let source = source(
        r#"
fn normalize(): i32 {
  return instanceof _ {
    _ => 0,
  }
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());
    assert!(resolve_result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == ResolverDiagnosticCode::UnresolvedName.as_code()
            && diagnostic.message().contains("unresolved name `_`")
    }));
}

#[test]
fn check_reports_non_exhaustive_instanceof() {
    let source = source(
        r#"
fn normalize(value: i32 | null): i32 {
  return instanceof value {
    i32 number => number,
  }
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
        diagnostic.code().as_str() == TypeDiagnosticCode::NonExhaustiveMatch.as_code()
            && diagnostic.message().contains("missing `null`")
    }));
}

#[test]
fn check_reports_catch_all_instanceof_pattern_before_final_arm() {
    let source = source(
        r#"
fn normalize(value: i32): i32 {
  return instanceof value {
    other => other,
    i32 number => number,
  }
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidPatternOrder.as_code()
    }));
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::UnreachablePattern.as_code()
    }));
}
