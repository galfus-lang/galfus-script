use super::{decode_from_thread_heap, encode_into_thread_heap};
use galfus_bytecode::instruction::TypeIdx;
use galfus_bytecode::{
    BytecodeModule, BytecodeType, ChoiceLayout, ChoiceVariantLayout, ConstantPool,
};
use galfus_contract::{BoundaryType, BoundaryValue};
use galfus_vm::{HeapObject, VmValue};

fn module(types: Vec<BytecodeType>) -> BytecodeModule {
    BytecodeModule {
        name: "test".to_string(),
        constants: ConstantPool::default(),
        functions: vec![],
        types,
        struct_layouts: vec![],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![],
        init_func_idx: None,
    }
}

#[test]
fn codec_preserves_the_declared_type_of_an_empty_array() {
    let module = module(vec![BytecodeType::Int32, BytecodeType::Array(TypeIdx(0))]);
    let mut heap = galfus_vm::thread::PrivateHeap::new();
    let reference = heap.alloc(HeapObject::Array {
        element_ty: TypeIdx(0),
        elements: vec![],
    });

    let value = decode_from_thread_heap(&heap, VmValue::Object(reference), TypeIdx(1), &module)
        .expect("empty array decodes with its declared element type");

    assert_eq!(
        value,
        BoundaryValue::Array {
            element_type: BoundaryType::I32,
            values: vec![],
        }
    );
}

#[test]
fn codec_encodes_an_array_with_its_expected_element_type() {
    let module = module(vec![BytecodeType::Int32, BytecodeType::Array(TypeIdx(0))]);
    let mut heap = galfus_vm::thread::PrivateHeap::new();

    let VmValue::Object(reference) = encode_into_thread_heap(
        &mut heap,
        BoundaryValue::Array {
            element_type: BoundaryType::I32,
            values: vec![BoundaryValue::I32(7)],
        },
        TypeIdx(1),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("array encodes with the expected element type") else {
        panic!("array codec must allocate an object");
    };
    let HeapObject::Array {
        element_ty,
        elements,
    } = heap.get_object(reference).expect("array exists")
    else {
        panic!("codec must allocate an array");
    };
    assert_eq!(*element_ty, TypeIdx(0));
    assert_eq!(elements, &vec![VmValue::Int32(7)]);
}

#[test]
fn codec_encodes_a_choice_with_its_declared_variant_payload() {
    let module = BytecodeModule {
        choice_layouts: vec![ChoiceLayout {
            name: "Result".to_string(),
            variants: vec![ChoiceVariantLayout {
                name: "Value".to_string(),
                payload_ty: Some(TypeIdx(0)),
            }],
        }],
        ..module(vec![
            BytecodeType::Int32,
            BytecodeType::Choice(galfus_bytecode::ChoiceLayoutIdx(0)),
        ])
    };
    let mut heap = galfus_vm::thread::PrivateHeap::new();

    let VmValue::Object(reference) = encode_into_thread_heap(
        &mut heap,
        BoundaryValue::Choice {
            variant: 0,
            payload: Some(Box::new(BoundaryValue::I32(7))),
        },
        TypeIdx(1),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("choice encodes with the expected layout") else {
        panic!("choice codec must allocate an object");
    };
    let HeapObject::Choice {
        module_id,
        layout_idx,
        variant_idx,
        payload,
    } = heap.get_object(reference).expect("choice exists")
    else {
        panic!("codec must allocate a choice");
    };
    assert_eq!(*module_id, galfus_core::ModuleId::new(1));
    assert_eq!(*layout_idx, galfus_bytecode::ChoiceLayoutIdx(0));
    assert_eq!(*variant_idx, 0);
    assert_eq!(*payload, VmValue::Int32(7));
}

#[test]
fn codec_round_trips_nominal_external_handles() {
    let module = module(vec![BytecodeType::ExternalHandle("file".to_string())]);
    let mut heap = galfus_vm::thread::PrivateHeap::new();
    let value = BoundaryValue::Handle {
        kind: "file".to_string(),
        id: 9,
    };

    let encoded = encode_into_thread_heap(
        &mut heap,
        value.clone(),
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("handle encodes with its declared kind");
    assert_eq!(
        decode_from_thread_heap(&heap, encoded, TypeIdx(0), &module),
        Ok(value)
    );
}
