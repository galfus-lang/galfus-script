use super::*;

#[test]
fn type_table_preloads_primitive_types() {
    let table = TypeTable::new();

    assert_eq!(table.len(), PrimitiveType::ALL.len());

    for primitive in PrimitiveType::ALL {
        let id = table.primitive(primitive);
        assert_eq!(table.kind(id), Some(&TypeKind::Primitive(primitive)));
    }
}

#[test]
fn type_table_reuses_named_types() {
    let mut table = TypeTable::new();
    let symbol = SymbolId::new(10);

    let first = table.intern_named(symbol);
    let second = table.intern_named(symbol);

    assert_eq!(first, second);
    assert_eq!(table.kind(first), Some(&TypeKind::Named { symbol }));
}

#[test]
fn type_table_reuses_array_types() {
    let mut table = TypeTable::new();
    let i32 = table.primitive(PrimitiveType::Int32);

    let first = table.intern_array(i32);
    let second = table.intern_array(i32);

    assert_eq!(first, second);
    assert_eq!(table.kind(first), Some(&TypeKind::Array { element: i32 }));
}

#[test]
fn type_table_reuses_tuple_types() {
    let mut table = TypeTable::new();
    let i32 = table.primitive(PrimitiveType::Int32);
    let bool_type = table.primitive(PrimitiveType::Bool);

    let first = table.intern_tuple(vec![i32, bool_type]);
    let second = table.intern_tuple(vec![i32, bool_type]);

    assert_eq!(first, second);
}

#[test]
fn type_table_normalizes_union_types() {
    let mut table = TypeTable::new();

    let null = table.primitive(PrimitiveType::Null);
    let i32 = table.primitive(PrimitiveType::Int32);
    let bool_type = table.primitive(PrimitiveType::Bool);

    let first = table.intern_union([i32, null, i32]);
    let second = table.intern_union([null, i32]);

    assert_eq!(first, second);

    let nested = table.intern_union([first, bool_type, i32]);
    let direct = table.intern_union([null, i32, bool_type]);

    assert_eq!(nested, direct);
}

#[test]
fn type_table_collapses_single_member_union() {
    let mut table = TypeTable::new();
    let i32 = table.primitive(PrimitiveType::Int32);

    let union = table.intern_union([i32, i32]);

    assert_eq!(union, i32);
}

#[test]
fn type_table_reuses_function_types() {
    let mut table = TypeTable::new();

    let i32 = table.primitive(PrimitiveType::Int32);
    let null = table.primitive(PrimitiveType::Null);

    let parameters = vec![
        FunctionParameterType::new(i32),
        FunctionParameterType::with_default(i32),
    ];

    let first = table.intern_function(parameters.clone(), null);
    let second = table.intern_function(parameters, null);

    assert_eq!(first, second);
}

#[test]
fn type_table_reuses_generic_instances() {
    let mut table = TypeTable::new();

    let result_symbol = SymbolId::new(20);
    let value_symbol = SymbolId::new(21);
    let error_symbol = SymbolId::new(22);

    let result = table.intern_named(result_symbol);
    let value = table.intern_generic_parameter(value_symbol);
    let error = table.intern_generic_parameter(error_symbol);

    let first = table.intern_generic_instance(result, vec![value, error]);
    let second = table.intern_generic_instance(result, vec![value, error]);

    assert_eq!(first, second);
}

#[test]
fn type_layer_binds_node_and_symbol_types() {
    let mut layer = TypeLayer::new();

    let node = NodeId::new(1);
    let symbol = SymbolId::new(2);
    let i32 = layer.table().primitive(PrimitiveType::Int32);

    layer.bind_node_type(node, i32);
    layer.bind_symbol_type(symbol, i32);

    assert_eq!(layer.node_type(node), Some(i32));
    assert_eq!(layer.symbol_type(symbol), Some(i32));
}

#[test]
fn type_table_describes_types() {
    let mut table = TypeTable::new();

    let i32 = table.primitive(PrimitiveType::Int32);
    let null = table.primitive(PrimitiveType::Null);
    let array = table.intern_array(i32);
    let union = table.intern_union([array, null]);

    assert_eq!(table.describe(i32), "i32");
    assert!(table.describe(union).contains("null"));
    assert!(table.describe(union).contains("[i32]"));
}
