use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_comparable_constraint() {
    let (_source, _graph, result, _string_table) = check_source(
        r#"
        constraint Comparable<Pattern, Value> {
          fn compare(self, value: Value): bool
        }

        struct Pattern satisfies Comparable<Pattern, [u8]> {}

        fn Pattern::compare(self, value: [u8]): bool {
          return true
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_builtin_comparable_return_type_mismatch() {
    let source = source(
        r#"
constraint Comparable<Pattern, Value> {
  fn compare(self, value: Value): bool
}

struct Pattern satisfies Comparable<Pattern, [u8]> {}

fn Pattern::compare(self, value: [u8]): i32 {
  return 0
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table, false);
    let result = check_definition_types(&source, &graph, result, &string_table, false);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFunctionTypeMismatch.as_code()
            && diagnostic.message().contains("function `compare` expected")
    }));
}
