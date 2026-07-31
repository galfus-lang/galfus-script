#[test]
fn check_accepts_struct_satisfies_constraint_field() {
    let (_source, _graph, result) = check_source(
        r#"
constraint Named {
  name: [u8],
}

struct User satisfies Named {
  name: [u8],
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_satisfies_non_constraint_target() {
    let source = source(
        r#"
struct Other {
  name: [u8],
}

struct User satisfies Other {
  name: [u8],
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidSatisfiesTarget.as_code()
            && diagnostic
                .message()
                .contains("satisfies target `Other` is not a constraint")
    }));
}

#[test]
fn check_reports_missing_constraint_field() {
    let source = source(
        r#"
constraint Named {
  name: [u8],
}

struct User satisfies Named {
  id: i32,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::MissingConstraintField.as_code()
            && diagnostic.message().contains("missing field `name`")
    }));
}

#[test]
fn check_reports_constraint_field_type_mismatch() {
    let source = source(
        r#"
constraint Named {
  name: [u8],
}

struct User satisfies Named {
  name: i32,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFieldTypeMismatch.as_code()
            && diagnostic
                .message()
                .contains("field `name` expected `[u8]`, got `i32`")
    }));
}

#[test]
fn check_accepts_struct_satisfies_multiple_constraints() {
    let (_source, _graph, result) = check_source(
        r#"
constraint Named {
  name: [u8],
}

constraint Identified {
  id: i32,
}

struct User satisfies Named, Identified {
  id: i32,
  name: [u8],
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_constraint_field_type_alias() {
    let (_source, _graph, result) = check_source(
        r#"
type Bytes = [u8]

constraint Named {
  name: Bytes,
}

struct User satisfies Named {
  name: [u8],
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_struct_satisfies_constraint_function() {
    let (_source, _graph, result) = check_source(
        r#"
constraint Named {
  fn name(): [u8],
}

struct User satisfies Named {
  name: [u8],
}

fn User::name(): [u8] {
  return "Ana"
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_constraint_function_value_anchor_call() {
    let (_source, _graph, result) = check_source(
        r#"
constraint Stringable {
  fn stringify(): [u8],
}

struct User satisfies Stringable {
  name: [u8],
}

fn User::stringify(): [u8] {
  return "Ana"
}

fn show(value: Stringable): [u8] {
  return value::stringify()
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_constraint_function_dot_call_as_unknown_member() {
    let source = source(
        r#"
constraint Stringable {
  fn stringify(): [u8],
}

fn show(value: Stringable): [u8] {
  return value.stringify()
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
        diagnostic.code().as_str() == TypeDiagnosticCode::UnknownMember.as_code()
            && diagnostic.message().contains("has no member `stringify`")
    }));
}

#[test]
fn check_reports_missing_constraint_function() {
    let source = source(
        r#"
constraint Named {
  fn name(): [u8],
}

struct User satisfies Named {
  name: [u8],
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
        diagnostic.code().as_str() == TypeDiagnosticCode::MissingConstraintFunction.as_code()
            && diagnostic.message().contains("missing function `name`")
    }));
}

#[test]
fn check_reports_constraint_function_return_type_mismatch() {
    let source = source(
        r#"
constraint Named {
  fn name(): [u8],
}

struct User satisfies Named {
  name: [u8],
}

fn User::name(): i32 {
  return 1
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFunctionTypeMismatch.as_code()
            && diagnostic.message().contains("function `name`")
    }));
}

#[test]
fn check_reports_constraint_function_parameter_type_mismatch() {
    let source = source(
        r#"
constraint Setter {
  fn set(value: [u8]): null,
}

struct User satisfies Setter {
  name: [u8],
}

fn User::set(value: i32): null {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFunctionTypeMismatch.as_code()
            && diagnostic.message().contains("function `set`")
    }));
}

#[test]
fn check_accepts_generic_constraint_function_explicit_argument() {
    let (_source, _graph, result) = check_source(
        r#"
constraint Stringable<T> {
  fn toString(self): [u8],
}

struct User satisfies Stringable<User> {
  name: [u8],
}

fn User::toString(self): [u8] {
  return self.name
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_anchored_function_using_struct_generic_parameter() {
    let (_source, _graph, result) = check_source(
        r#"
constraint Unwrap<Self, Value> {
  fn unwrap(self): Self,
}

struct Box<T: i32 | f64> satisfies Unwrap<Box<T>, T> {
  value: T,
}

fn Box::unwrap(self): Box<T> {
  return self
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_generic_constraint_function_return_type_mismatch() {
    let source = source(
        r#"
constraint Stringable<T> {
  fn toString(self): [u8],
}

struct User satisfies Stringable<User> {
  name: [u8],
}

fn User::toString(self): i32 {
  return 1
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFunctionTypeMismatch.as_code()
            && diagnostic.message().contains("function `toString`")
    }));
}

#[test]
fn check_accepts_generic_constraint_field() {
    let (_source, _graph, result) = check_source(
        r#"
constraint HasValue<T> {
  value: T,
}

struct IntBox satisfies HasValue<i32> {
  value: i32,
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_generic_constraint_field_type_mismatch() {
    let source = source(
        r#"
constraint HasValue<T> {
  value: T,
}

struct IntBox satisfies HasValue<i32> {
  value: bool,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFieldTypeMismatch.as_code()
            && diagnostic.message().contains("field `value`")
    }));
}

