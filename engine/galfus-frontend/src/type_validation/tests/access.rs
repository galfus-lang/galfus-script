use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_struct_member_access() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i64,
}

fn getId(user: User): i64 {
  return user.id
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_struct_member_expression_type() {
    let (source, graph, result) = check_source(
        r#"
struct User {
  id: i64,
}

fn getId(user: User): i64 {
  return user.id
}
"#,
    );

    let member =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::MemberExpression, "user.id")
            .unwrap();

    let ty = result.layer().node_type(member).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int64))
    );
}

#[test]
fn check_reports_unknown_struct_member() {
    let source = source(
        r#"
struct User {
  id: i64,
}

fn getName(user: User): i64 {
  return user.name
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
            && diagnostic.message().contains("has no member `name`")
    }));
}

#[test]
fn check_accepts_null_safe_member_access_for_nullable_target() {
    let (_source, _graph, result) = check_source(
        r#"
struct User {
  id: i64,
}

fn getId(user: User | null): i64 | null {
  return user?.id
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_direct_member_access_on_nullable_target() {
    let source = source(
        r#"
struct User {
  id: i64,
}

fn getId(user: User | null): i64 {
  return user.id
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
            && diagnostic.message().contains("has no member `id`")
    }));
}

#[test]
fn check_accepts_array_index_expression() {
    let (_source, _graph, result) = check_source(
        r#"
fn get(values: [i32]): i32 | null {
  return values[0]
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_array_index_expression_type_as_nullable_element() {
    let (source, graph, result) = check_source(
        r#"
fn get(values: [i32]): i32 | null {
  return values[0]
}
"#,
    );

    let index = find_node_by_kind_and_text(
        &source,
        &graph,
        SyntaxNodeKind::IndexExpression,
        "values[0]",
    )
    .unwrap();

    let ty = result.layer().node_type(index).unwrap();

    match result.layer().table().kind(ty) {
        Some(TypeKind::Union { members }) => {
            assert!(members.iter().any(|member| {
                result.layer().table().kind(*member)
                    == Some(&TypeKind::Primitive(PrimitiveType::Int32))
            }));

            assert!(members.iter().any(|member| {
                result.layer().table().kind(*member)
                    == Some(&TypeKind::Primitive(PrimitiveType::Null))
            }));
        }

        other => panic!("expected nullable index result, got {other:?}"),
    }
}

#[test]
fn check_reports_invalid_index_target() {
    let source = source(
        r#"
fn get(value: i32): i32 | null {
  return value[0]
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidIndexTarget.as_code()
            && diagnostic.message().contains("cannot be indexed")
    }));
}

#[test]
fn check_reports_invalid_index_type() {
    let source = source(
        r#"
fn get(values: [i32]): i32 | null {
  return values[true]
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidIndexType.as_code()
            && diagnostic.message().contains("index must be an integer")
    }));
}
