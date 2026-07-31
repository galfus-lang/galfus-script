use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_for_over_array_literal() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  for value in [1, 2, 3] {
    var copied: i32 = value
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_for_over_string_literal_as_uint8_array() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  for byte in "Ana" {
    var copied: u8 = byte
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_for_over_empty_string_literal() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  for byte in "" {
    var copied: u8 = byte
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_for_over_iterable_struct() {
    let (_source, _graph, result) = check_source(
        r#"
        constraint Iterator<T, Item> {
          fn next(self): Item | null
        }

        constraint Iterable<T, Item, Iter> {
          fn iter(self): Iter
        }

        struct Numbers satisfies Iterable<Numbers, i32, NumbersIterator> {
          values: [i32],
        }

        struct NumbersIterator satisfies Iterator<NumbersIterator, i32> {
          values: [i32],
          index: i32,
        }

        fn Numbers::iter(self): NumbersIterator {
          return new(NumbersIterator) {
            values: self.values,
            index: 0,
          }
        }

        fn NumbersIterator::next(self): i32 | null {
          return self.values[self.index]
        }

        fn main(): null {
          var nums = new(Numbers) {
            values: [1, 2, 3],
          }

          for value in nums {
            var copied: i32 = value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_for_binding_type_from_iterable_struct() {
    let (source, graph, result) = check_source(
        r#"
        constraint Iterator<T, Item> {
          fn next(self): Item | null
        }

        constraint Iterable<T, Item, Iter> {
          fn iter(self): Iter
        }

        struct Numbers satisfies Iterable<Numbers, i32, NumbersIterator> {
          values: [i32],
        }

        struct NumbersIterator satisfies Iterator<NumbersIterator, i32> {
          values: [i32],
          index: i32,
        }

        fn Numbers::iter(self): NumbersIterator {
          return new(NumbersIterator) {
            values: self.values,
            index: 0,
          }
        }

        fn NumbersIterator::next(self): i32 | null {
          return self.values[self.index]
        }

        fn main(): null {
          var nums = new(Numbers) {
            values: [1, 2, 3],
          }

          for value in nums {
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
fn check_reports_iterable_with_non_iterator_iter_type() {
    let source = source(
        r#"
constraint Iterator<T, Item> {
  fn next(self): Item | null
}

constraint Iterable<T, Item, Iter> {
  fn iter(self): Iter
}

struct Iter {}

struct Source satisfies Iterable<Source, i32, Iter> {}

fn Source::iter(self): Iter {
  return new(Iter) {}
}

fn main(): null {
  var source = new(Source) {}

  for item in source {
    var copied: i32 = item
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
                .contains("for iterable must satisfy `Iterable`")
    }));
}

#[test]
fn check_reports_iterator_next_return_type_mismatch() {
    let source = source(
        r#"
        constraint Iterator<T, Item> {
          fn next(self): Item | null
        }

        struct BadIterator satisfies Iterator<BadIterator, i32> {}

        fn BadIterator::next(self): bool {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFunctionTypeMismatch.as_code()
            && diagnostic.message().contains("function `next` expected")
    }));
}

#[test]
fn check_reports_iterable_iter_return_type_mismatch() {
    let source = source(
        r#"
constraint Iterator<T, Item> {
  fn next(self): Item | null
}

constraint Iterable<T, Item, Iter> {
  fn iter(self): Iter
}

struct Iter {}

struct Source satisfies Iterable<Source, i32, Iter> {}

fn Source::iter(self): bool {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFunctionTypeMismatch.as_code()
            && diagnostic.message().contains("function `iter` expected")
    }));
}

#[test]
fn check_reports_iterator_next_item_type_mismatch() {
    let source = source(
        r#"
constraint Iterator<T, Item> {
  fn next(self): Item | null
}

struct BadIterator satisfies Iterator<BadIterator, i32> {}

fn BadIterator::next(self): bool | null {
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ConstraintFunctionTypeMismatch.as_code()
            && diagnostic.message().contains("function `next` expected")
    }));
}

#[test]
fn check_reports_for_over_iterator_without_iterable() {
    let source = source(
        r#"
constraint Iterator<T, Item> {
  fn next(self): Item | null
}

struct Counter satisfies Iterator<Counter, i32> {}

fn Counter::next(self): i32 | null {
  return 1
}

fn main(): null {
  var counter = new(Counter) {}

  for value in counter {
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
                .contains("for iterable must satisfy `Iterable`")
    }));
}
