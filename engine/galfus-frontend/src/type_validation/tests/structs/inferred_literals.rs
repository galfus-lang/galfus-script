use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_collects_anchor_and_temporary_ownership_metadata() {
    let (_source, graph, result) = check_source(
        r#"
struct User {
  name: [u8],
}

var global_user: User = new(User) { name: "Ana" }

fn User::rename(user: User, name: [u8]): User {
  var local_user: User = new(User) { name }
  return local_user
}
"#,
    );

    let global_user = symbol_by_name_and_kind(&graph, "global_user", SymbolKind::Var);
    let local_user = symbol_by_name_and_kind(&graph, "local_user", SymbolKind::Var);
    let user_parameter = symbol_by_name_and_kind(&graph, "user", SymbolKind::Parameter);

    let anchors = result.ownership_metadata().anchors();

    assert!(anchors.iter().any(|anchor| {
        anchor.kind() == AnchorKind::ModuleState && anchor.symbol() == Some(global_user)
    }));

    assert!(anchors.iter().any(|anchor| {
        anchor.kind() == AnchorKind::BlockLocal && anchor.symbol() == Some(local_user)
    }));

    assert!(anchors.iter().any(|anchor| {
        anchor.kind() == AnchorKind::FunctionParameter && anchor.symbol() == Some(user_parameter)
    }));

    assert!(
        anchors
            .iter()
            .any(|anchor| anchor.kind() == AnchorKind::FunctionAnchor)
    );

    assert!(!result.ownership_metadata().temporaries().is_empty());
    assert!(
        anchors
            .iter()
            .any(|anchor| anchor.kind() == AnchorKind::Temporary)
    );

    assert!(
        result
            .ownership_metadata()
            .release_eligibilities()
            .iter()
            .any(|eligibility| {
                eligibility.kind() == ReleaseEligibilityKind::Anchor
                    && eligibility.symbol() == Some(global_user)
            })
    );

    assert!(
        result
            .ownership_metadata()
            .release_eligibilities()
            .iter()
            .any(|eligibility| eligibility.kind() == ReleaseEligibilityKind::Temporary)
    );
}

#[test]
fn check_reports_non_nullable_weak_field_type() {
    let source = source(
        r#"
struct Node {
  weak parent: Node,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidWeakFieldType.as_code()
            && diagnostic
                .message()
                .contains("weak field type must be nullable")
    }));
    assert_eq!(result.ownership_metadata().weak_fields().len(), 1);
}

#[test]
fn check_accepts_nullable_alias_weak_field_type() {
    let (_source, _graph, result) = check_source(
        r#"
type MaybeNode = Node | null

struct Node {
  weak parent: MaybeNode,
}
"#,
    );

    assert_eq!(result.ownership_metadata().weak_fields().len(), 1);
}

#[test]
fn check_accepts_inferred_struct_literal_with_expected_type() {
    let (_source, _graph, result) = check_source(
        r#"
        struct User {
          id: i32,
          name: [u8],
        }

        var user: User = new {
          id: 1,
          name: "Ana",
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_inferred_struct_literal_with_default_field() {
    let (_source, _graph, result) = check_source(
        r#"
        struct User {
          name: [u8],
          age: i32 = 0,
        }

        var user: User = new {
          name: "Ana",
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_inferred_struct_literal_without_expected_type() {
    let source = source(
        r#"
        var user = new {
          id: 1,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::CannotInferType.as_code()
            && diagnostic
                .message()
                .contains("inferred struct literal requires an expected struct type")
    }));
}

#[test]
fn check_reports_inferred_struct_literal_with_non_struct_expected_type() {
    let source = source(
        r#"
        var value: i32 = new {
          id: 1,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::CannotInferType.as_code()
            && diagnostic
                .message()
                .contains("inferred struct literal requires an expected struct type")
    }));
}

#[test]
fn check_reports_inferred_struct_literal_unknown_field() {
    let source = source(
        r#"
        struct User {
          id: i32,
        }

        var user: User = new {
          id: 1,
          name: "Ana",
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
        diagnostic.code().as_str() == TypeDiagnosticCode::UnknownStructField.as_code()
            && diagnostic.message().contains("has no field `name`")
    }));
}

#[test]
fn check_reports_inferred_struct_literal_field_type_mismatch() {
    let source = source(
        r#"
        struct User {
          id: i32,
        }

        var user: User = new {
          id: true,
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
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic.message().contains("expected `i32`, got `bool`")
    }));
}
