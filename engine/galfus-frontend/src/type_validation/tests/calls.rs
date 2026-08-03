use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_call_with_matching_arguments() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
fn add(a: i32, b: i32): i32 {
  return a
}

var value: i32 = add(1, 2)
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_contextual_integer_call_argument_type() {
    let (source, graph, result, string_table) = check_source(
        r#"
fn push(byte: u8): null {
  return
}

var value = push(27)
"#,
    );

    assert!(!result.has_errors());

    let literal =
        find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::IntegerLiteral, "27").unwrap();
    let ty = result.layer().node_type(literal).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Uint8))
    );
}

#[test]
fn check_binds_call_expression_return_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
fn one(): i32 {
  return 1
}

var value: i32 = one()
"#,
    );

    let source = SourceFile::new(
        galfus_core::SourceId::new(0),
        "test.gfs".to_string(),
        r#"
fn one(): i32 {
  return 1
}

var value: i32 = one()
"#
        .to_string(),
    );

    let call = find_node_by_kind_and_text(&source, &graph, SyntaxNodeKind::CallExpression, "one()")
        .unwrap();

    let ty = result.layer().node_type(call).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_reports_await_of_non_future() {
    let (_source, _graph, result, string_table) = check_source_named(
        "await-non-future.gfs",
        "fn main(): i32 {\n  const value = await 1\n  return value\n}\n",
    );

    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::AwaitRequiresFuture.as_code()
            && diagnostic.message().contains("await requires `Future<T>`")
    }));
}

#[test]
fn check_reports_call_argument_type_mismatch() {
    let source = source(
        r#"
fn add(a: i32, b: i32): i32 {
  return a
}

var value: i32 = add(true, 2)
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
            && diagnostic.message().contains("expected `i32`, got `bool`")
    }));
}

#[test]
fn check_reports_too_few_call_arguments() {
    let source = source(
        r#"
fn add(a: i32, b: i32): i32 {
  return a
}

var value: i32 = add(1)
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::ArgumentCountMismatch.as_code()
            && diagnostic.message().contains("expected 2 arguments, got 1")
    }));
}

#[test]
fn check_reports_too_many_call_arguments() {
    let source = source(
        r#"
fn add(a: i32, b: i32): i32 {
  return a
}

var value: i32 = add(1, 2, 3)
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::ArgumentCountMismatch.as_code()
            && diagnostic.message().contains("expected 2 arguments, got 3")
    }));
}

#[test]
fn check_reports_calling_non_function() {
    let source = source(
        r#"
var age: i32 = 10
var result: i32 = age()
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::NotCallable.as_code()
            && diagnostic.message().contains("is not callable")
    }));
}

#[test]
fn check_accepts_default_parameter_argument_count() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
fn add(a: i32, b: i32 = 1): i32 {
  return a
}

var value: i32 = add(1)
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_rest_parameter_argument_count() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
fn sum(...values: [i32]): i32 {
  return 1
}

var value: i32 = sum(1, 2, 3)
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_omitted_default_argument() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
        fn call(a: i32, b: i32 = 2, c: i32 = 3): i32 {
          return a + b + c
        }

        var value: i32 = call(1, _, 3)
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_omitted_required_argument() {
    let source = source(
        r#"
        fn call(a: i32, b: i32 = 2): i32 {
          return a + b
        }

        var value: i32 = call(_, 2)
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
        diagnostic.code().as_str() == TypeDiagnosticCode::ArgumentCountMismatch.as_code()
            && diagnostic.message().contains("argument cannot be omitted")
    }));
}

#[test]
fn check_accepts_individual_arguments_for_rest_parameter() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
        fn sum(...values: [i32]): i32 {
          return 0
        }

        fn call(...values: [i32]): null {
          return
        }

        var value: i32 = sum(1, 2, 3)

        fn main(): null {
          call()
          call(1)
          call(1, 2)
          call(1, 2, 3)
          return
        }
        "#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_anchor_function_path_call() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
struct User {
  name: [u8],
}

fn User::rename(user: User, name: [u8]): User {
  return new(User) { name }
}

var user: User = new(User) { name: "Ana" }
var renamed: User = User::rename(user, "Lia")
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_await_of_anchored_async_call() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
struct Future<T> { id: i64 }
struct User { id: i32 }

fn(async) User::load(user: User): i32 {
  return user.id
}

fn main(user: User): i32 {
  const future = User::load(user)
  return await future
}
"#,
    );

    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

#[test]
fn check_binds_anchor_function_path_type() {
    let (source, graph, result, string_table) = check_source(
        r#"
struct User {
  name: [u8],
}

fn User::rename(user: User, name: [u8]): User {
  return new(User) { name }
}

var rename = User::rename
"#,
    );

    let path = find_node_by_kind_and_text(
        &source,
        &graph,
        SyntaxNodeKind::PathExpression,
        "User::rename",
    )
    .unwrap();

    let ty = result.layer().node_type(path).unwrap();

    assert!(matches!(
        result.layer().table().kind(ty),
        Some(TypeKind::Function(_))
    ));
}

#[test]
fn check_reports_expression_statement_type_mismatch() {
    let source = source(
        r#"
fn print(text: [u8]): null {
  return null
}

fn main(): null {
  print(123)
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::TypeMismatch.as_code()
    }));
}

#[test]
fn check_reports_restricted_builtin_symbol_declaration() {
    let source = source(
        r#"
fn __provider_write(text: [u8]): null {
  return null
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::RestrictedBuiltinSymbol.as_code()
    }));
}

#[test]
fn check_reports_restricted_internal_symbol_declaration() {
    let source = source(
        r#"
fn __internal_thread_create(text: [u8]): null {
  return null
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::RestrictedBuiltinSymbol.as_code()
    }));
}

#[test]
fn check_reports_restricted_builtin_symbol_reference() {
    let source = source(
        r#"
fn __provider_write(text: [u8]): null {
  return null
}

fn main(): null {
  var x = __provider_write
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_graph(), &mut string_table);
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph, &string_table);
    let result = check_definition_types(&source, &graph, result, &string_table);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::RestrictedBuiltinSymbol.as_code()
    }));
}
