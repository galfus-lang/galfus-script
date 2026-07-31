use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_match_literal_patterns() {
    let (_source, _graph, result) = check_source(
        r#"
fn code(value: i32): i32 {
  return match value {
    1 => 10,
    2 => 20,
    fallback => fallback,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_match_literal_pattern_type_mismatch() {
    let source = source(
        r#"
fn code(value: i32): i32 {
  return match value {
    true => 1,
    fallback => fallback,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidMatchPatternType.as_code()
            && diagnostic.message().contains("got `bool`")
    }));
}

#[test]
fn check_reports_incompatible_match_arm_type() {
    let source = source(
        r#"
fn code(value: i32): i32 {
  return match value {
    1 => 10,
    fallback => true,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::IncompatibleMatchArmType.as_code()
            && diagnostic
                .message()
                .contains("match arm body must be compatible with `i32`, got `bool`")
    }));
}

#[test]
fn check_accepts_enum_variant_patterns() {
    let (_source, _graph, result) = check_source(
        r#"
enum Direction {
  North,
  South,
}

fn code(direction: Direction): i32 {
  return match direction {
    Direction::North => 1,
    Direction::South => 2,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_choice_payload_pattern() {
    let (_source, _graph, result) = check_source(
        r#"
choice Result {
  Ok(i32),
  Err([u8]),
}

fn unwrap(result: Result): i32 {
  return match result {
    Result::Ok(value) => value,
    Result::Err(message) => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_generic_choice_payload_pattern_from_subject() {
    let (_source, _graph, result) = check_source(
        r#"
choice Outcome<T> {
  Ok(T),
  Err([u8]),
}

fn unwrap(value: Outcome<i32>): i32 {
  return match value {
    Outcome::Ok(result) => result,
    Outcome::Err(message) => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_infers_match_literal_from_generic_choice_payload_type() {
    let (_source, graph, result) = check_source(
        r#"
choice Outcome<T> {
  Ok(T),
  Err([u8]),
}

fn unwrap(value: Outcome<i32>): i32 {
  var result = match value {
    Outcome::Ok(result) => result,
    Outcome::Err(_) => 0,
  }
  return result
}
"#,
    );

    let literal = find_node_by_kind(&graph, SyntaxNodeKind::IntegerLiteral).unwrap();
    let literal_type = result.layer().node_type(literal).unwrap();

    assert_eq!(
        result.layer().table().kind(literal_type),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_reports_choice_payload_pattern_count_mismatch() {
    let source = source(
        r#"
choice Result {
  Ok(i32),
}

fn unwrap(result: Result): i32 {
  return match result {
    Result::Ok() => 0,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ArgumentCountMismatch.as_code()
            && diagnostic.message().contains("expected 1 arguments, got 0")
    }));
}

#[test]
fn check_reports_choice_payload_pattern_type_mismatch() {
    let source = source(
        r#"
choice Result {
  Ok(i32),
}

fn unwrap(result: Result): i32 {
  return match result {
    Result::Ok(true) => 0,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidMatchPatternType.as_code()
            && diagnostic.message().contains("got `bool`")
    }));
}

#[test]
fn check_accepts_exhaustive_choice_match() {
    let (_source, _graph, result) = check_source(
        r#"
        choice Result {
          Ok(i32),
          Err([u8]),
        }

        fn unwrap(result: Result): i32 {
          return match result {
            Result::Ok(value) => value,
            Result::Err(message) => 0,
          }
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_choice_match_with_wildcard_default() {
    let (_source, _graph, result) = check_source(
        r#"
        choice Result {
          Ok(i32),
          Err([u8]),
        }

        fn unwrap(result: Result): i32 {
          return match result {
            Result::Ok(value) => value,
            _ => 0,
          }
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_choice_match_with_binding_default() {
    let (_source, _graph, result) = check_source(
        r#"
        choice Result {
          Ok(i32),
          Err([u8]),
        }

        fn unwrap(result: Result): i32 {
          return match result {
            Result::Ok(value) => value,
            fallback => 0,
          }
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_non_exhaustive_choice_match() {
    let source = source(
        r#"
        choice Result {
          Ok(i32),
          Err([u8]),
        }

        fn unwrap(result: Result): i32 {
          return match result {
            Result::Ok(value) => value,
          }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::NonExhaustiveMatch.as_code()
            && diagnostic.message().contains("missing `Err`")
    }));
}

#[test]
fn check_reports_multiple_missing_choice_match_variants() {
    let source = source(
        r#"
        choice State {
          Loading,
          Ready,
          Failed([u8]),
        }

        fn code(state: State): i32 {
          return match state {
            State::Loading => 0,
          }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::NonExhaustiveMatch.as_code()
            && diagnostic.message().contains("`Ready`")
            && diagnostic.message().contains("`Failed`")
    }));
}

#[test]
fn check_reports_non_exhaustive_enum_match() {
    let source = source(
        r#"
        enum Direction {
          North,
          South,
        }

        fn code(direction: Direction): i32 {
          return match direction {
            Direction::North => 1,
          }
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::NonExhaustiveMatch.as_code()
            && diagnostic.message().contains("missing `South`")
    }));
}

#[test]
fn check_reports_catch_all_match_pattern_before_final_arm() {
    let source = source(
        r#"
fn code(value: i32): i32 {
  return match value {
    fallback => fallback,
    1 => 10,
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
