use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_exhaustive_typeof_over_bounded_generic() {
    let (_source, _graph, result) = check_source(
        r#"
type Scalar = i32 | bool | null

fn dispatch<T: Scalar>(): i32 {
  return typeof T {
    i32 => 1,
    bool => 2,
    null => 3,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_typeof_wildcard_fallback() {
    let (_source, _graph, result) = check_source(
        r#"
type Scalar = i32 | bool | null

fn dispatch<T: Scalar>(): i32 {
  return typeof T {
    i32 => 1,
    _ => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_typeof_propagates_generic_type_to_arm_body() {
    let (_source, _graph, result) = check_source(
        r#"
type Scalar = i32 | bool

choice Box<T: Scalar> {
  Value(T),
}

fn make<T: Scalar>(): Box<T> {
  return typeof T {
    i32 => Box::Value(1),
    bool => Box::Value(true),
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_non_exhaustive_typeof_over_bounded_generic() {
    let source = source(
        r#"
type Scalar = i32 | bool | null

fn dispatch<T: Scalar>(): i32 {
  return typeof T {
    i32 => 1,
    bool => 2,
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
            && diagnostic.message().contains("typeof")
            && diagnostic.message().contains("missing `null`")
    }));
}

#[test]
fn check_reports_unbounded_typeof_without_wildcard() {
    let source = source(
        r#"
fn dispatch<T>(): i32 {
  return typeof T {
    i32 => 1,
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
            && diagnostic.message().contains("missing `_`")
    }));
}

#[test]
fn check_reports_incompatible_typeof_arm_body() {
    let source = source(
        r#"
type Scalar = i32 | bool

fn make<T: Scalar>(): T {
  return typeof T {
    i32 => 1,
    bool => 2,
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
            && diagnostic.message().contains("typeof arm body")
    }));
}
