use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_function_stamp_without_stamp_recursion() {
    let (_source, _graph, result) = check_source(
        r#"
fn base(value: i32): i32 {
  return value
}

fn(stamp) doubled(value: i32): i32 {
  return base(value) + base(value)
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_regular_recursive_function() {
    let (_source, _graph, result) = check_source(
        r#"
fn repeat(value: i32): i32 {
  return repeat(value)
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_direct_recursive_function_stamp() {
    let source = source(
        r#"
fn(stamp) repeat(value: i32): i32 {
  return repeat(value)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::RecursiveFunctionStamp.as_code()
            && diagnostic.message().contains("repeat -> repeat")
    }));
}

#[test]
fn check_reports_direct_recursive_generic_function_stamp() {
    let source = source(
        r#"
fn(stamp) repeat<T>(value: T): T {
  return repeat<T>(value)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::RecursiveFunctionStamp.as_code()
            && diagnostic.message().contains("repeat -> repeat")
    }));
}

#[test]
fn check_reports_indirect_recursive_function_stamp() {
    let source = source(
        r#"
fn(stamp) first(value: i32): i32 {
  return second(value)
}

fn(stamp) second(value: i32): i32 {
  return first(value)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::RecursiveFunctionStamp.as_code()
            && diagnostic.message().contains("first -> second -> first")
    }));
}
