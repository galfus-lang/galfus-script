use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_struct_literal() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
  name: [u8],
}

var user: User = new(User) {
  id: 1,
  name: "Ana",
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_contextual_integer_struct_field_type() {
    let (source, graph, result) = check_source(
        r#"
struct Color {
  r: u8,
}

var color: Color = new(Color) {
  r: 220,
}
"#,
    );

    assert!(!result.has_errors());

    let literal =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::IntegerLiteral, "220").unwrap();
    let ty = result.layer().node_type(literal).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Uint8))
    );
}

#[test]
fn check_binds_struct_literal_type() {
    let (_, graph, result) = check_source(
        r#"
struct User {
  id: i32,
  name: [u8],
}

var user: User = new(User) {
  id: 1,
  name: "Ana",
}
"#,
    );

    let literal = find_node_by_kind(&graph, SyntaxNodeKind::StructLiteral).unwrap();

    let user_symbol = symbol_by_name_and_kind(&graph, "User", SymbolKind::Struct);
    let ty = result.layer().node_type(literal).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Named {
            symbol: user_symbol
        })
    );
}

#[test]
fn check_reports_unknown_struct_field() {
    let source = source(
        r#"
struct User {
  id: i32,
}

var user: User = new(User) {
  id: 1,
  name: "Ana",
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
        diagnostic.code().as_str() == TypeDiagnosticCode::UnknownStructField.as_code()
            && diagnostic.message().contains("has no field `name`")
    }));
}

#[test]
fn check_reports_duplicate_struct_field() {
    let source = source(
        r#"
struct User {
  id: i32,
}

var user: User = new(User) {
  id: 1,
  id: 2,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::DuplicateStructField.as_code()
            && diagnostic.message().contains("duplicate field `id`")
    }));
}

#[test]
fn check_reports_missing_required_struct_field() {
    let source = source(
        r#"
struct User {
  id: i32,
  name: [u8],
}

var user: User = new(User) {
  id: 1,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::MissingStructField.as_code()
            && diagnostic
                .message()
                .contains("missing required field `name`")
    }));
}

#[test]
fn check_accepts_missing_default_struct_field() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
  age: i32 = 0,
}

var user: User = new(User) {
  id: 1,
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_assignment_to_mutable_struct_field() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
}

var user: User = new(User) { id: 1 }

fn update(): null {
  user.id = 2
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_assignment_to_const_struct_field() {
    let source = source(
        r#"
struct User {
  const id: i32,
}

var user: User = new(User) { id: 1 }

fn update(): null {
  user.id = 2
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
                .contains("cannot assign to immutable binding `id`")
    }));
}

#[test]
fn check_reports_struct_field_type_mismatch() {
    let source = source(
        r#"
struct User {
  id: i32,
}

var user: User = new(User) {
  id: true,
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
fn check_accepts_struct_literal_shorthand() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
  name: [u8],
}

var id: i32 = 1
var name: [u8] = "Ana"

var user: User = new(User) {
  id,
  name,
}
"#,
    );

    assert!(!result.has_errors());
}
