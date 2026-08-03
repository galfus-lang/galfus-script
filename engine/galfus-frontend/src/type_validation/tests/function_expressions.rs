use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_infers_expression_function_body_type() {
    let (source, graph, result, _string_table) = check_source(
        r#"
        var double = fn (value: i32): i32 => value * 2
        "#,
    );

    let function = find_node_by_kind(&graph, SyntaxNodeKind::ExpressionFunction).unwrap();

    let ty = result.layer().node_type(function).unwrap();

    let TypeKind::Function(function_type) = result.layer().table().kind(ty).unwrap() else {
        panic!("expected function type");
    };

    assert_eq!(function_type.parameters().len(), 1);

    assert_eq!(
        result
            .layer()
            .table()
            .kind(function_type.parameters()[0].ty()),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );

    assert_eq!(
        result.layer().table().kind(function_type.return_type()),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );

    assert_eq!(
        source.slice(graph.syntax().node(function).unwrap().span()),
        Some("fn (value: i32): i32 => value * 2")
    );
}

#[test]
fn check_infers_expression_function_return_type_without_annotation() {
    let (_source, graph, result, _string_table) = check_source(
        r#"
        var double = fn (value: i32) => value * 2
        "#,
    );

    let function = find_node_by_kind(&graph, SyntaxNodeKind::ExpressionFunction).unwrap();

    let ty = result.layer().node_type(function).unwrap();

    let TypeKind::Function(function) = result.layer().table().kind(ty).unwrap() else {
        panic!("expected function type");
    };

    assert_eq!(
        result.layer().table().kind(function.return_type()),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_binds_async_function_expression_as_a_future() {
    let (_source, graph, result, _string_table) = check_source(
        r#"
struct Future<T> {
  id: i64,
}

var load = fn(async) (): i32 => 1
"#,
    );

    let function = find_node_by_kind(&graph, SyntaxNodeKind::ExpressionFunction).unwrap();
    let ty = result.layer().node_type(function).unwrap();
    let TypeKind::Function(function) = result.layer().table().kind(ty).unwrap() else {
        panic!("expected function type");
    };
    assert!(matches!(
        result.layer().table().kind(function.return_type()),
        Some(TypeKind::GenericInstance { .. })
    ));
}

#[test]
fn check_accepts_block_function_body() {
    let (_source, _graph, result, _string_table) = check_source(
        r#"
        var printer = fn (value: i32): null {
          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_missing_return_for_block_function() {
    let source = source(
        r#"
        var callback = fn (): i32 {
          var value = 1
        }
        "#,
    );

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::MissingReturn.as_code()
    }));
}

#[test]
fn check_accepts_function_metadata_on_function_expression() {
    let (_source, _graph, result, _string_table) = check_source(
        r#"
        var callback = fn(stamp) (): i32 => 1
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_expression_function_as_call_argument() {
    let (_source, _graph, result, _string_table) = check_source(
        r#"
        fn apply(callback: fn(i32): i32): i32 {
          return callback(1)
        }

        var result = apply(fn (value: i32): i32 => value * 2)
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_collects_closure_capture_ownership_metadata() {
    let (_source, graph, result, string_table) = check_source(
        r#"
        struct Box {
          value: i32,
        }

        var captured: Box = new(Box) { value: 2 }
        var make = fn (): Box => captured
        "#,
    );

    let function = find_node_by_kind(&graph, SyntaxNodeKind::ExpressionFunction).unwrap();
    let captured = symbol_by_name_and_kind(&graph, "captured", SymbolKind::Var, &string_table);

    let captures = result.ownership_metadata().captures();

    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].closure(), function);
    assert_eq!(captures[0].symbol(), captured);

    assert!(
        result
            .ownership_metadata()
            .release_eligibilities()
            .iter()
            .any(|eligibility| {
                eligibility.kind() == ReleaseEligibilityKind::Capture
                    && eligibility.symbol() == Some(captured)
            })
    );
}

#[test]
fn check_does_not_capture_function_expression_local_parameter() {
    let (_source, _graph, result, _string_table) = check_source(
        r#"
        var double = fn (value: i32): i32 => value * 2
        "#,
    );

    assert!(result.ownership_metadata().captures().is_empty());
}

#[test]
fn check_does_not_leak_block_function_return_to_outer_function() {
    let (_source, _graph, result, _string_table) = check_source(
        r#"
        fn main(): null {
          var callback = fn (value: i32): i32 {
            return value
          }

          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_expression_function_body_return_mismatch() {
    let source = source(
        r#"
        var bad = fn (value: i32): bool => value * 2
        "#,
    );

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic.message().contains("expected `bool`, got `i32`")
    }));
}

#[test]
fn check_reports_expression_function_assignment_mismatch() {
    let source = source(
        r#"
        var callback: fn(i32): bool = fn (value: i32): i32 => value * 2
        "#,
    );

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
    }));
}
