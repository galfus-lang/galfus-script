use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_primitive_type_alias_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
type UserId = i32

var id: UserId = 1
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_primitive_type_alias_assignment_mismatch() {
    let source = source(
        r#"
type UserId = i32

var id: UserId = true
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
fn check_accepts_union_type_alias_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
type MaybeInt = i32 | null

var value: MaybeInt = null
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_dynamic_array_type_alias_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
type Bytes = [u8]

var name: Bytes = ""
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_array_type_alias_assignment() {
    let (_source, _graph, result) = check_source(
        r#"
type Ints = [i32]

var values: Ints = [1, 2, 3]
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_for_over_dynamic_array_type_alias() {
    let (_source, _graph, result) = check_source(
        r#"
type Ints = [i32]

fn main(values: Ints): null {
  for value in values {
    var copied: i32 = value
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_struct_field_type_alias() {
    let (_source, _graph, result) = check_source(
        r#"
type Bytes = [u8]

struct User {
  name: Bytes,
}

var user: User = new(User) {
  name: "Ana",
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_instanceof_with_union_type_alias() {
    let (_source, _graph, result) = check_source(
        r#"
type MaybeInt = i32 | null

fn normalize(value: MaybeInt): i32 {
  return instanceof value {
    i32 number => number,
    _ => 0,
  }
}
"#,
    );

    assert!(!result.has_errors());
}
