use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_for_over_exclusive_range() {
    let (_source, _graph, result) = check_source(
        r#"
        fn main(): null {
          for value in 1..10 {
            var copied: i32 = value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_for_over_quantity_range_with_step() {
    let (_source, _graph, result) = check_source(
        r#"
        fn main(): null {
          for value in 1::10%2 {
            var copied: i32 = value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_for_binding_type_from_range() {
    let (source, graph, result) = check_source(
        r#"
        fn main(): null {
          for value in 1..10 {
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
fn check_reports_range_assigned_to_int() {
    let source = source(
        r#"
        fn main(): null {
          var seq: i32 = 1::10
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
        diagnostic.message().contains("expected `i32`")
            && diagnostic.message().contains("range<i32>")
    }));
}

#[test]
fn check_accepts_for_over_quantity_range() {
    let (_source, _graph, result) = check_source(
        r#"
        fn main(): null {
          for value in 1::10 {
            var copied: i32 = value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_for_over_float_quantity_range() {
    let (_source, _graph, result) = check_source(
        r#"
        fn main(): null {
          for value in 1.0::3%0.5 {
            var copied: f32 = value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_descending_exclusive_range() {
    let (_source, _graph, result) = check_source(
        r#"
        fn main(): null {
          for value in 10..1 {
            var copied: i32 = value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_negative_range_step() {
    let (_source, _graph, result) = check_source(
        r#"
        fn main(): null {
          for value in 10::4%-2 {
            var copied: i32 = value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_float_exclusive_range() {
    let source = source(
        r#"
        fn main(): null {
          for value in 1.0..10.0 {
            var copied: f32 = value
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidRangeOperandType.as_code()
            && diagnostic
                .message()
                .contains("range operand must be integer literal")
    }));
}

#[test]
fn check_reports_empty_exclusive_range() {
    let source = source(
        r#"
        fn main(): null {
          for value in 1..1 {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidRangeOperandType.as_code()
            && diagnostic.message().contains("range must not be empty")
    }));
}

#[test]
fn check_reports_non_positive_range_count() {
    let source = source(
        r#"
        fn main(): null {
          for value in 1::0 {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidRangeOperandType.as_code()
            && diagnostic
                .message()
                .contains("range count must be greater than zero")
    }));
}

#[test]
fn check_reports_negative_range_count() {
    let source = source(
        r#"
        fn main(): null {
          for value in 1::-1 {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidRangeOperandType.as_code()
            && diagnostic
                .message()
                .contains("range count must be greater than zero")
    }));
}

#[test]
fn check_reports_range_overflow() {
    let source = source(
        r#"
        fn main(): null {
          for value in 2147483640::20 {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidRangeOperandType.as_code()
            && diagnostic.message().contains("range end overflows i32")
    }));
}

#[test]
fn check_reports_zero_range_step() {
    let source = source(
        r#"
        fn main(): null {
          for value in 1::2%0 {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidRangeOperandType.as_code()
            && diagnostic.message().contains("range step must not be zero")
    }));
}

#[test]
fn check_reports_mismatched_range_step_family() {
    let source = source(
        r#"
        fn main(): null {
          for value in 1::2%0.5 {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidRangeOperandType.as_code()
            && diagnostic
                .message()
                .contains("range step must have the same numeric family as start")
    }));
}
