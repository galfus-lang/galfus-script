use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_function_decorator() {
    let (_source, _graph, result) = check_source(
        r#"
        fn log(target: fn(): null): fn(): null {
          return target
        }

        @log
        fn save(): null {
          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_decorator_call_with_arguments() {
    let (_source, _graph, result) = check_source(
        r#"
        fn tag(target: fn(): null, name: [u8], value: i32, enabled: bool): fn(): null {
          return target
        }

        @tag("stable", 1, true)
        fn save(): null {
          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_struct_and_field_decorators() {
    let (_source, _graph, result) = check_source(
        r#"
        fn frozen(target: User): User {
          return target
        }

        fn trim(target: [u8]): [u8] {
          return target
        }

        @frozen
        struct User {
          @trim
          name: [u8],
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_choice_payload_item_decorator() {
    let (_source, _graph, result) = check_source(
        r#"
        fn path(target: [u8]): [u8] {
          return target
        }

        choice Asset {
          Texture(@path [u8]),
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_weak_struct_field_decorator() {
    let (_source, _graph, result) = check_source(
        r#"
        fn nullable(target: Node | null): Node | null {
          return target
        }

        struct Node {
          @nullable
          weak parent: Node | null,
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_omitted_decorator_argument() {
    let source = source(
        r#"
        fn tag(target: fn(): null, name: [u8], priority: i32): fn(): null {
          return target
        }

        @tag(_, 1)
        fn save(): null {
          return
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidDecoratorUsage.as_code()
            && diagnostic
                .message()
                .contains("decorator arguments cannot be omitted")
    }));
}

#[test]
fn check_accepts_omitted_default_decorator_argument() {
    let (_source, _graph, result) = check_source(
        r#"
        fn tag(target: fn(): null, name: [u8] = "stable", priority: i32): fn(): null {
          return target
        }

        @tag(_, 1)
        fn save(): null {
          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn parse_accepts_decorated_weak_struct_field() {
    let source = source(
        r#"
        struct Node {
          @nullable
          weak parent: Node | null,
        }
        "#,
    );

    let result = parse(&source);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    let graph = result.into_graph();
    let weak_field = find_node_by_kind(&graph, SyntaxNodeKind::WeakStructField).unwrap();

    assert!(
        graph
            .syntax()
            .first_child_of_kind(weak_field, SyntaxNodeKind::DecoratorList)
            .is_some()
    );
}

#[test]
fn check_accepts_parameter_and_rest_parameter_decorators() {
    let (_source, _graph, result) = check_source(
        r#"
        fn trim(target: [u8]): [u8] {
          return target
        }

        fn nonempty(target: [i32]): [i32] {
          return target
        }

        fn save(@trim name: [u8], @nonempty ...values: [i32]): null {
          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_unresolved_decorator_function() {
    let source = source(
        r#"
        fn save(@trim name: [u8]): null {
          return
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

    assert!(resolve_result.has_errors());
    assert!(
        resolve_result
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.message().contains("unresolved name `trim`") })
    );
}

#[test]
fn check_reports_decorator_target_type_mismatch() {
    let source = source(
        r#"
        fn trim(target: i32): i32 {
          return target
        }

        fn save(@trim name: [u8]): null {
          return
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
            && diagnostic.message().contains("expected `i32`, got `[u8]`")
    }));
}

#[test]
fn check_reports_decorator_return_type_mismatch() {
    let source = source(
        r#"
        fn trim(target: [u8]): bool {
          return true
        }

        fn save(@trim name: [u8]): null {
          return
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
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidDecoratorUsage.as_code()
            && diagnostic
                .message()
                .contains("decorator return type must match decorated target type")
    }));
}

#[test]
fn check_reports_decorator_explicit_argument_type_mismatch() {
    let source = source(
        r#"
        fn min(target: i32, value: i32): i32 {
          return target
        }

        struct User {
          @min("zero")
          age: i32,
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
            && diagnostic.message().contains("expected `i32`, got `[u8]`")
    }));
}

#[test]
fn check_accepts_function_decorator_transformer() {
    let (_source, _graph, result) = check_source(
        r#"
        fn log(target: fn(i32): bool): fn(i32): bool {
          return target
        }

        @log
        fn save(value: i32): bool {
          return true
        }
        "#,
    );

    assert!(!result.has_errors());
}
