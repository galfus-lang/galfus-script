use super::*;
use crate::ResolverDiagnosticCode;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_for_over_dynamic_array() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(values: [i32]): null {
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
fn check_reports_for_over_non_array() {
    let source = source(
        r#"
fn main(): null {
  for value in 10 {
    var copied: i32 = value
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidIterableType.as_code()
            && diagnostic
                .message()
                .contains("for iterable must satisfy `Iterable`, got `i32`")
    }));
}

#[test]
fn check_binds_for_binding_type_from_dynamic_array() {
    let (source, graph, result) = check_source(
        r#"
fn main(values: [i32]): null {
  for value in values {
    var copied: i32 = value
  }

  return
}
"#,
    );

    let binding =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::ForBinding, "value").unwrap();

    let ty = result.layer().node_type(binding).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_reports_for_binding_type_mismatch_in_body() {
    let source = source(
        r#"
fn main(values: [i32]): null {
  for value in values {
    var copied: bool = value
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
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic.message().contains("expected `bool`, got `i32`")
    }));
}

#[test]
fn check_reports_direct_builtin_constraint_without_import() {
    let source = source(
        r#"
struct Pattern satisfies Comparable<Pattern, [u8]> {}

fn Pattern::compare(self, value: [u8]): bool {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::RestrictedBuiltinSymbol.as_code()
            && diagnostic.message().contains("Comparable")
    }));
}

#[test]
fn check_accepts_ignored_for_binding() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(values: [i32]): null {
  for _ in values {
    var copied: i32 = 1
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_for_index_as_int32() {
    let (source, graph, result) = check_source(
        r#"
fn main(values: [i32]): null {
  for value, index in values {
    var copied: i32 = value
    var position: i32 = index
  }

  return
}
"#,
    );

    let index =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::Identifier, "index").unwrap();

    let ty = result.layer().node_type(index).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_ignored_for_binding_does_not_create_referenceable_symbol() {
    let source = source(
        r#"
fn main(values: [i32]): null {
  for _ in values {
    var copied: i32 = _
  }

  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.has_errors());
    assert!(resolve_result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == ResolverDiagnosticCode::UnresolvedName.as_code()
            && diagnostic.message().contains("unresolved name `_`")
    }));
}
