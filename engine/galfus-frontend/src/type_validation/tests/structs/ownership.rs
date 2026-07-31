use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_struct_literal_spread_from_same_struct() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
  name: [u8],
}

var base: User = new(User) {
  id: 1,
  name: "Ana",
}

var renamed: User = new(User) {
  ...base,
  name: "Bia",
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_invalid_struct_literal_spread_target() {
    let source = source(
        r#"
struct User {
  id: i32,
}

var base: i32 = 1
var user: User = new(User) {
  ...base,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidStructSpreadTarget.as_code()
            && diagnostic
                .message()
                .contains("struct literal spread target must be a struct")
    }));
}

#[test]
fn check_reports_invalid_struct_expansion_target() {
    let source = source(
        r#"
struct User {
  ...i32,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidStructExpansionTarget.as_code()
            && diagnostic
                .message()
                .contains("struct expansion target must be a struct")
    }));
}

#[test]
fn check_accepts_struct_expansion_fields_in_literal() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
  name: [u8],
}

struct Admin {
  ...User,
  role: [u8],
}

var admin: Admin = new(Admin) {
  id: 1,
  name: "Ana",
  role: "root",
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_member_access_to_struct_expansion_field() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i32,
}

struct Admin {
  ...User,
  role: [u8],
}

var admin: Admin = new(Admin) {
  id: 1,
  role: "root",
}

var id: i32 = admin.id
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_copy_expression_to_original_value_type() {
    let (source, graph, result) = check_source(
        r#"
struct User {
  id: i32,
}

var user: User = new(User) { id: 1 }
var cloned: User = copy user
"#,
    );

    let copy =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::CopyExpression, "copy user")
            .unwrap();

    let user_symbol = symbol_by_name_and_kind(&graph, "User", SymbolKind::Struct);
    let ty = result.layer().node_type(copy).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Named {
            symbol: user_symbol
        })
    );
}

#[test]
fn check_collects_weak_field_ownership_metadata() {
    let (_source, graph, result) = check_source(
        r#"
struct Node {
  value: i32,
  weak parent: Node | null,
}
"#,
    );

    let weak_fields = result.ownership_metadata().weak_fields();

    assert_eq!(weak_fields.len(), 1);

    let metadata = weak_fields[0];
    let node_symbol = symbol_by_name_and_kind(&graph, "Node", SymbolKind::Struct);
    let parent_symbol = symbol_by_name_and_kind(&graph, "parent", SymbolKind::StructField);

    assert_eq!(metadata.struct_symbol(), node_symbol);
    assert_eq!(metadata.field_symbol(), parent_symbol);

    assert!(matches!(
        result.layer().table().kind(metadata.field_type()),
        Some(TypeKind::Union { .. })
    ));
}

#[test]
fn check_collects_edge_and_weak_observer_ownership_metadata() {
    let (_source, graph, result) = check_source(
        r#"
struct Node {
  child: Node | null,
  weak parent: Node | null,
}
"#,
    );

    let node_symbol = symbol_by_name_and_kind(&graph, "Node", SymbolKind::Struct);
    let child_symbol = symbol_by_name_and_kind(&graph, "child", SymbolKind::StructField);
    let parent_symbol = symbol_by_name_and_kind(&graph, "parent", SymbolKind::StructField);

    assert!(
        result.ownership_metadata().edges().iter().any(|edge| {
            edge.owner_symbol() == node_symbol && edge.field_symbol() == child_symbol
        })
    );

    assert!(
        result
            .ownership_metadata()
            .weak_observers()
            .iter()
            .any(|observer| {
                observer.owner_symbol() == node_symbol && observer.field_symbol() == parent_symbol
            })
    );
}

#[test]
fn check_collects_strong_ownership_cycle_metadata() {
    let (_source, graph, result) = check_source(
        r#"
struct Parent {
  child: Child | null,
}

struct Child {
  parent: Parent | null,
}
"#,
    );

    let parent_symbol = symbol_by_name_and_kind(&graph, "Parent", SymbolKind::Struct);
    let child_symbol = symbol_by_name_and_kind(&graph, "Child", SymbolKind::Struct);

    assert!(result.ownership_metadata().cycles().iter().any(|cycle| {
        cycle.structs().contains(&parent_symbol) && cycle.structs().contains(&child_symbol)
    }));
}

#[test]
fn check_ignores_weak_fields_for_ownership_cycle_metadata() {
    let (_source, _graph, result) = check_source(
        r#"
struct Parent {
  child: Child | null,
}

struct Child {
  weak parent: Parent | null,
}
"#,
    );

    assert!(result.ownership_metadata().cycles().is_empty());
}

#[test]
fn check_reports_copy_of_fieldless_struct() {
    let source = source(
        r#"
struct RuntimeToken {}

var token: RuntimeToken = new(RuntimeToken) {}
var cloned: RuntimeToken = copy token
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidCopyTarget.as_code()
            && diagnostic
                .message()
                .contains("fieldless structs are not copyable")
    }));
}
