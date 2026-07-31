use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_empty_return_for_null_function() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_matching_return_type() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): i32 {
  return 10
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_return_type_mismatch() {
    let source = source(
        r#"
fn main(): i32 {
  return true
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
            && diagnostic.message().contains("expected `i32`, got `bool`")
    }));
}

#[test]
fn check_reports_empty_return_for_non_null_function() {
    let source = source(
        r#"
fn main(): i32 {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic.message().contains("expected `i32`, got `null`")
    }));
}

#[test]
fn check_accepts_nullable_return_type() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): i32 | null {
  return null
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_name_return_with_matching_type() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(value: i32): i32 {
  return value
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_inferred_struct_literal_return_with_expected_type() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
}

fn main(): User {
  return new {
    id: 1,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_inferred_generic_struct_literal_return_with_expected_type() {
    let (_source, _graph, result) = check_source(
        r#"
struct Box<T: i64 | f64> {
  value: T,
}

fn main<T: i64 | f64>(): Box<T> {
  return new {
    value: 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_missing_return_for_non_null_function() {
    let source = source(
        r#"
fn one(): i32 {
  var value = 1
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
        diagnostic.code().as_str() == TypeDiagnosticCode::MissingReturn.as_code()
            && diagnostic
                .message()
                .contains("function must return `i32` on every path")
    }));
}

#[test]
fn check_accepts_if_else_when_both_paths_return() {
    let (_source, _graph, result) = check_source(
        r#"
fn one(flag: bool): i32 {
  if flag {
    return 1
  } else {
    return 2
  }
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_missing_return_when_if_has_no_else_return_path() {
    let source = source(
        r#"
fn one(flag: bool): i32 {
  if flag {
    return 1
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
        diagnostic.code().as_str() == TypeDiagnosticCode::MissingReturn.as_code()
    }));
}

#[test]
fn check_warns_unreachable_statement_after_return() {
    let source = source(
        r#"
fn main(): null {
  return
  var value = 1
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

    assert!(!result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::UnreachableCode.as_code()
    }));
}
