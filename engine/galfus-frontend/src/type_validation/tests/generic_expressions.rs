use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_binds_generic_cast_type_in_anchored_function() {
    let (_source, graph, result) = check_source(
        r#"
struct Range<T: i32 | f32> {
  value: T,
  index: i32,
}

fn Range<T>::next(self): T {
  return self.value + <T>self.index * self.value
}
"#,
    );

    let cast = find_node_by_kind(&graph, SyntaxNodeKind::CastExpression).unwrap();
    let target = graph.syntax().child(cast, 0).unwrap();
    let cast_type = result.layer().node_type(cast).unwrap();
    let target_type = result.layer().node_type(target).unwrap();

    assert_eq!(cast_type, target_type);
    assert!(matches!(
        result.layer().table().kind(cast_type),
        Some(TypeKind::GenericParameter { .. })
    ));
}

#[test]
fn check_accepts_explicit_generic_function_call() {
    let (_source, _graph, result) = check_source(
        r#"
        fn identity<T: int>(value: T): T {
          return value
        }

        var value: i32 = identity<i32>(1)
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_binds_explicit_generic_call_return_type() {
    let (source, graph, result) = check_source(
        r#"
        fn identity<T: int>(value: T): T {
          return value
        }

        var value = identity<i32>(1)
        "#,
    );

    let call = find_node_by_kind_and_text(
        &source,
        &graph,
        SyntaxNodeKind::CallExpression,
        "identity<i32>(1)",
    )
    .unwrap();

    let ty = result.layer().node_type(call).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_accepts_explicit_generic_function_call_with_array_type_argument() {
    let (_source, _graph, result) = check_source(
        r#"
        fn identity<T: [u8]>(value: T): T {
          return value
        }

        var bytes: [u8] = "Ana"
        var value: [u8] = identity<[u8]>(bytes)
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_explicit_generic_function_call_with_multiple_arguments() {
    let (_source, _graph, result) = check_source(
        r#"
        fn first<A: int, B: bool>(left: A, right: B): A {
          return left
        }

        var value: i32 = first<i32, bool>(1, true)
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_generic_bound_union_with_constraint_member() {
    let (_source, _graph, result) = check_source(
        r#"
        constraint Stringable {
          fn text(): [u8],
        }

        struct User satisfies Stringable {
          name: [u8],
        }

        fn User::text(): [u8] {
          return "Ana"
        }

        fn stringify<T: int | Stringable>(value: T): [u8] {
          return "value"
        }

        var user = new(User) { name: "Ana" }
        var text: [u8] = stringify<User>(user)
        "#,
    );

    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

#[test]
fn check_reports_explicit_generic_argument_count_mismatch_for_missing_argument() {
    let source = source(
        r#"
        fn pair<A: int, B: bool>(left: A, right: B): A {
          return left
        }

        var value = pair<i32>(1, true)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::GenericArgumentCountMismatch.as_code()
            && diagnostic
                .message()
                .contains("expected 2 generic argument(s), got 1")
    }));
}

#[test]
fn check_reports_explicit_generic_argument_count_mismatch_for_extra_argument() {
    let source = source(
        r#"
        fn identity<T: int>(value: T): T {
          return value
        }

        var value = identity<i32, bool>(1)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::GenericArgumentCountMismatch.as_code()
            && diagnostic
                .message()
                .contains("expected 1 generic argument(s), got 2")
    }));
}

#[test]
fn check_reports_explicit_generic_call_argument_type_mismatch_after_substitution() {
    let source = source(
        r#"
        fn identity<T: int>(value: T): T {
          return value
        }

        var value = identity<i32>(true)
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

#[test]
fn check_reports_generic_arguments_on_non_generic_function() {
    let source = source(
        r#"
        fn one(): i32 {
          return 1
        }

        var value = one<i32>()
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
        diagnostic.code().as_str() == TypeDiagnosticCode::GenericArgumentCountMismatch.as_code()
            && diagnostic
                .message()
                .contains("expected 0 generic argument(s), got 1")
    }));
}

#[test]
fn check_allows_unbounded_generic_function_parameter() {
    let source = source(
        r#"
        fn identity<T>(value: T): T {
          return value
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

    assert!(!result.has_errors());
}

#[test]
fn check_reports_generic_argument_outside_declared_bound() {
    let source = source(
        r#"
        fn identity<T: int>(value: T): T {
          return value
        }

        var value = identity<u32>(1)
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
            && diagnostic
                .message()
                .contains("generic argument for `T` must satisfy")
    }));
}
