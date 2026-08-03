use super::*;

#[test]
fn check_binds_function_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
fn main(value: i32): null {
  return
}

"#,
    );

    let main = symbol_by_name_and_kind(&graph, "main", SymbolKind::Function, &string_table);
    let ty = result.layer().symbol_type(main).unwrap();

    match result.layer().table().kind(ty) {
        Some(TypeKind::Function(function)) => {
            assert_eq!(function.parameters().len(), 1);

            let parameter = function.parameters()[0].ty();
            assert_eq!(
                result.layer().table().kind(parameter),
                Some(&TypeKind::Primitive(PrimitiveType::Int32))
            );

            assert_eq!(
                result.layer().table().kind(function.return_type()),
                Some(&TypeKind::Primitive(PrimitiveType::Null))
            );
        }
        other => panic!("expected function type, got {other:?}"),
    }
}

#[test]
fn check_binds_async_function_as_a_future_of_its_payload() {
    let (_source, graph, result, string_table) =
        check_source("struct Future<T> { id: i64 }\nfn(async) load(): i32 { return 1 }\n");

    let load = symbol_by_name_and_kind(&graph, "load", SymbolKind::Function, &string_table);
    let ty = result.layer().symbol_type(load).unwrap();
    let TypeKind::Function(function) = result.layer().table().kind(ty).unwrap() else {
        panic!("expected function type");
    };
    let TypeKind::GenericInstance { arguments, .. } =
        result.layer().table().kind(function.return_type()).unwrap()
    else {
        panic!("expected Future payload type");
    };
    assert_eq!(arguments.len(), 1);
    assert_eq!(
        result.layer().table().kind(arguments[0]),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_binds_parameter_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
fn main(value: i32): null {
  return
}
"#,
    );

    let value = symbol_by_name_and_kind(&graph, "value", SymbolKind::Parameter, &string_table);
    let ty = result.layer().symbol_type(value).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_binds_rest_parameter_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
fn main(...values: [i32]): null {
  return
}
"#,
    );

    let values =
        symbol_by_name_and_kind(&graph, "values", SymbolKind::RestParameter, &string_table);
    let ty = result.layer().symbol_type(values).unwrap();

    assert!(matches!(
        result.layer().table().kind(ty),
        Some(TypeKind::Array { .. })
    ));
}

#[test]
fn check_binds_struct_field_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
struct User {
  id: i64,
}
"#,
    );

    let id = symbol_by_name_and_kind(&graph, "id", SymbolKind::StructField, &string_table);
    let ty = result.layer().symbol_type(id).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int64))
    );
}

#[test]
fn check_binds_var_annotation_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
var age: i32 = 10
"#,
    );

    let age = symbol_by_name_and_kind(&graph, "age", SymbolKind::Var, &string_table);
    let ty = result.layer().symbol_type(age).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn check_binds_const_annotation_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
const enabled: bool = true
"#,
    );

    let enabled = symbol_by_name_and_kind(&graph, "enabled", SymbolKind::Const, &string_table);
    let ty = result.layer().symbol_type(enabled).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Bool))
    );
}

#[test]
fn check_binds_type_alias_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
type MaybeInt = i32 | null
"#,
    );

    let alias = symbol_by_name_and_kind(&graph, "MaybeInt", SymbolKind::TypeAlias, &string_table);
    let ty = result.layer().symbol_type(alias).unwrap();

    assert!(matches!(
        result.layer().table().kind(ty),
        Some(TypeKind::Union { .. })
    ));
}

#[test]
fn check_binds_named_type_definition_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
struct User {
  id: i64,
}
"#,
    );

    let user = symbol_by_name_and_kind(&graph, "User", SymbolKind::Struct, &string_table);
    let ty = result.layer().symbol_type(user).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Named { symbol: user })
    );
}

#[test]
fn check_binds_generic_parameter_symbol_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
fn identity<T: int>(value: T): T {
  return value
}
"#,
    );

    let generic = symbol_by_name_and_kind(&graph, "T", SymbolKind::GenericParameter, &string_table);
    let ty = result.layer().symbol_type(generic).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::GenericParameter { symbol: generic })
    );
}

#[test]
fn check_binds_struct_destructuring_field_types() {
    let (_source, graph, result, string_table) = check_source(
        r#"
struct Point {
  x: i32,
  y: bool,
}

var { x, y } = new(Point) { x: 1, y: true }
"#,
    );

    let x = symbol_by_name_and_kind(&graph, "x", SymbolKind::Var, &string_table);
    let y = symbol_by_name_and_kind(&graph, "y", SymbolKind::Var, &string_table);

    assert_eq!(
        result
            .layer()
            .table()
            .kind(result.layer().symbol_type(x).unwrap()),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
    assert_eq!(
        result
            .layer()
            .table()
            .kind(result.layer().symbol_type(y).unwrap()),
        Some(&TypeKind::Primitive(PrimitiveType::Bool))
    );
}

#[test]
fn check_binds_tuple_destructuring_element_types() {
    let (_source, graph, result, string_table) = check_source(
        r#"
var (count, enabled) = (1, true)
"#,
    );

    let count = symbol_by_name_and_kind(&graph, "count", SymbolKind::Var, &string_table);
    let enabled = symbol_by_name_and_kind(&graph, "enabled", SymbolKind::Var, &string_table);

    assert_eq!(
        result
            .layer()
            .table()
            .kind(result.layer().symbol_type(count).unwrap()),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
    assert_eq!(
        result
            .layer()
            .table()
            .kind(result.layer().symbol_type(enabled).unwrap()),
        Some(&TypeKind::Primitive(PrimitiveType::Bool))
    );
}

#[test]
fn check_binds_array_destructuring_rest_type() {
    let (_source, graph, result, string_table) = check_source(
        r#"
var [head, ...tail] = [1, 2, 3]
"#,
    );

    let head = symbol_by_name_and_kind(&graph, "head", SymbolKind::Var, &string_table);
    let tail = symbol_by_name_and_kind(&graph, "tail", SymbolKind::Var, &string_table);

    assert_eq!(
        result
            .layer()
            .table()
            .kind(result.layer().symbol_type(head).unwrap()),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
    assert!(matches!(
        result
            .layer()
            .table()
            .kind(result.layer().symbol_type(tail).unwrap()),
        Some(TypeKind::Array { .. })
    ));
}

#[test]
fn check_binds_self_symbol_type_in_simple_anchored_function() {
    let (_source, graph, result, string_table) = check_source(
        r#"
struct Point {
  x: i32,
  y: i32,
}

fn Point::x(self): i32 {
  return self.x
}
"#,
    );

    let self_symbol = symbol_by_name_and_kind(&graph, "self", SymbolKind::Parameter, &string_table);
    let point_symbol = symbol_by_name_and_kind(&graph, "Point", SymbolKind::Struct, &string_table);
    let ty = result.layer().symbol_type(self_symbol).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Named {
            symbol: point_symbol
        })
    );
}

#[test]
fn check_binds_self_symbol_type_in_generic_anchored_function() {
    let (_source, graph, result, string_table) = check_source(
        r#"
struct Box<T: i32 | bool> {
  value: T,
}

fn Box<T>::value(self): T {
  return self.value
}
"#,
    );

    let self_symbol = symbol_by_name_and_kind(&graph, "self", SymbolKind::Parameter, &string_table);
    let ty = result.layer().symbol_type(self_symbol).unwrap();

    assert!(
        matches!(
            result.layer().table().kind(ty),
            Some(TypeKind::GenericInstance { .. })
        ),
        "expected GenericInstance for self, got {:?}",
        result.layer().table().kind(ty)
    );
}

#[test]
fn check_accepts_match_on_self_field_in_anchored_function() {
    let (_source, _graph, result, string_table) = check_source(
        r#"
struct Range {
  start: i32,
  end: i32,
  current: i32,
}

fn Range::next(self): i32 | null {
  const direction: i32 = match self.start < self.end {
    true => 1,
    false => -1,
  }
  const value = self.start + self.current * direction

  if value == self.end {
    return null
  }

  self.current += 1
  return value
}
"#,
    );

    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

#[test]
fn check_binds_self_and_annotated_params_in_multi_param_anchored_function() {
    let (_source, graph, result, string_table) = check_source(
        r#"
struct Point {
  x: i32,
  y: i32,
}

fn Point::move(self, dx: i32, dy: i32): Point {
  return new {
    x: self.x + dx,
    y: self.y + dy,
  }
}
"#,
    );

    let self_symbol = symbol_by_name_and_kind(&graph, "self", SymbolKind::Parameter, &string_table);
    let point_symbol = symbol_by_name_and_kind(&graph, "Point", SymbolKind::Struct, &string_table);
    let self_ty = result.layer().symbol_type(self_symbol).unwrap();

    assert_eq!(
        result.layer().table().kind(self_ty),
        Some(&TypeKind::Named {
            symbol: point_symbol
        })
    );

    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

#[test]
fn check_user_module_rejects_internal_function_declaration() {
    let (_source, _graph, result, string_table) = check_source_named(
        "test.gfs",
        r#"
fn __internal_custom(): null {
  return
}
"#,
    );

    assert!(result.has_errors());
}

#[test]
fn check_user_module_rejects_provider_function_declaration() {
    let (_source, _graph, result, string_table) = check_source_named(
        "test.gfs",
        r#"
fn(async) __provider_gpio_read(): [u8] {
  return ""
}
"#,
    );

    assert!(result.has_errors());
}

#[test]
fn check_provider_function_without_async_metadata_is_rejected() {
    let (_source, _graph, result, string_table) = check_source_named(
        "std/gpio",
        r#"
fn __provider_gpio_read(): [u8] {
  return ""
}
"#,
    );

    assert!(result.has_errors());
}
