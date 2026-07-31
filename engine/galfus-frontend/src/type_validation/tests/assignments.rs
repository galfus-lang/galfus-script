use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_matching_assignment_type() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  var age: i32 = 10
  age = 20
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_assignment_type_mismatch() {
    let source = source(
        r#"
fn main(): null {
  var age: i32 = 10
  age = true
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
            && diagnostic.message().contains("expected `i32`, got `bool`")
    }));
}

#[test]
fn check_reports_assignment_to_const() {
    let source = source(
        r#"
fn main(): null {
  const age: i32 = 10
  age = 20
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
        diagnostic.code().as_str() == TypeDiagnosticCode::AssignmentToImmutable.as_code()
            && diagnostic
                .message()
                .contains("cannot assign to immutable binding `age`")
    }));
}

#[test]
fn check_reports_assignment_to_parameter() {
    let source = source(
        r#"
fn main(age: i32): null {
  age = 20
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
        diagnostic.code().as_str() == TypeDiagnosticCode::AssignmentToImmutable.as_code()
            && diagnostic
                .message()
                .contains("cannot assign to immutable binding `age`")
    }));
}

#[test]
fn check_reports_assignment_to_for_binding() {
    let source = source(
        r#"
fn main(values: [i32]): null {
  for value in values {
    value = 20
  }
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
        diagnostic.code().as_str() == TypeDiagnosticCode::AssignmentToImmutable.as_code()
            && diagnostic
                .message()
                .contains("cannot assign to immutable binding `value`")
    }));
}

#[test]
fn check_accepts_nullable_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  var maybe: i32 | null = null
  maybe = 10
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_numeric_compound_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  var age: i32 = 10
  age += 1
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_numeric_compound_assignment_type_error() {
    let source = source(
        r#"
fn main(): null {
  var age: i32 = 10
  age += true
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
        diagnostic.code().as_str() == TypeDiagnosticCode::UnsupportedOperator.as_code()
            && diagnostic.message().contains("numeric operands")
    }));
}

#[test]
fn check_accepts_bitwise_compound_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  var flags: i32 = 1
  flags |= 2
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_shift_compound_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  var flags: i32 = 1
  flags <<= 2
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_null_fallback_assignment_for_nullable_target() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  var maybe: i32 | null = null
  maybe ??= 1
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_null_fallback_assignment_for_non_nullable_target() {
    let source = source(
        r#"
fn main(): null {
  var value: i32 = 1
  value ??= 2
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
        diagnostic.code().as_str() == TypeDiagnosticCode::UnsupportedOperator.as_code()
            && diagnostic.message().contains("nullable target")
    }));
}
