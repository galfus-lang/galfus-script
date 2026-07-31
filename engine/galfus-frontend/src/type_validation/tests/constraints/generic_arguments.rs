#[test]
fn check_reports_generic_constraint_missing_argument() {
    let source = source(
        r#"
constraint Stringable<T> {
  fn toString(self): [u8],
}

struct User satisfies Stringable {
  name: [u8],
}

fn User::toString(self): [u8] {
  return self.name
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
        diagnostic.code().as_str()
            == TypeDiagnosticCode::ConstraintGenericArgumentCountMismatch.as_code()
            && diagnostic
                .message()
                .contains("constraint `Stringable` expects 1 generic argument")
    }));
}

#[test]
fn check_accepts_generic_parameter_constraint_bound() {
    let (_source, _graph, result) = check_source(
        r#"
        constraint HasValue<T> {
          value: T,
        }

        fn read<T: HasValue<i32>>(value: T): null {
          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_generic_parameter_constraint_non_constraint_target() {
    let source = source(
        r#"
        struct Other {
          value: i32,
        }

        fn read<T: Other>(value: T): null {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidSatisfiesTarget.as_code()
            && diagnostic
                .message()
                .contains("satisfies target `Other` is not a constraint")
    }));
}

#[test]
fn check_reports_generic_parameter_constraint_missing_argument() {
    let source = source(
        r#"
        constraint HasValue<T> {
          value: T,
        }

        fn read<T: HasValue>(value: T): null {
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
        diagnostic.code().as_str()
            == TypeDiagnosticCode::ConstraintGenericArgumentCountMismatch.as_code()
            && diagnostic
                .message()
                .contains("constraint `HasValue` expects 1 generic argument")
    }));
}

#[test]
fn check_reports_generic_parameter_constraint_extra_argument() {
    let source = source(
        r#"
        constraint HasValue<T> {
          value: T,
        }

        fn read<T: HasValue<i32, bool>>(value: T): null {
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
        diagnostic.code().as_str()
            == TypeDiagnosticCode::ConstraintGenericArgumentCountMismatch.as_code()
            && diagnostic
                .message()
                .contains("constraint `HasValue` expects 1 generic argument")
    }));
}
