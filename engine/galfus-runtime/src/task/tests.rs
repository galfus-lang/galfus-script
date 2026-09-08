use super::{
    decode_surface_from_thread_heap, encode_future_value_into_thread_heap, execution_stack,
    with_execution_stack,
};
use crate::event::FutureValue;
use galfus_bytecode::instruction::{ChoiceLayoutIdx, FuncIdx, StructLayoutIdx, TypeIdx};
use galfus_bytecode::{
    BytecodeModule, BytecodeType, ChoiceLayout, ChoiceVariantLayout, ConstantPool, FieldLayout,
    OwnershipKind, StructLayout,
};
use galfus_contract::{
    SurfaceContract, SurfaceDirection, SurfaceHandle, SurfaceSchema, SurfaceValue,
};
use galfus_vm::{HeapObject, VmValue};

#[test]
fn execution_stack_preserves_the_suspended_call_chain() {
    let mut thread = galfus_vm::thread::VmThreadState::test_new();
    thread.call_stack = vec![
        galfus_vm::runtime::CallFrame {
            module_id: galfus_core::ModuleId::new(1),
            func_idx: FuncIdx(2),
            pc: 4,
            register_base: 0,
            return_dest: None,
            cached_instructions: &[] as *const [galfus_bytecode::Instruction],
            has_objects: false,
        },
        galfus_vm::runtime::CallFrame {
            module_id: galfus_core::ModuleId::new(3),
            func_idx: FuncIdx(5),
            pc: 0,
            register_base: 0,
            return_dest: None,
            cached_instructions: &[] as *const [galfus_bytecode::Instruction],
            has_objects: false,
        },
    ];

    assert_eq!(
        execution_stack(&thread),
        vec![
            galfus_contract::ExecutionFrame {
                module_id: 3,
                function_id: 5,
                instruction_offset: 0,
            },
            galfus_contract::ExecutionFrame {
                module_id: 1,
                function_id: 2,
                instruction_offset: 3,
            },
        ]
    );
}

#[test]
fn execution_stack_does_not_replace_a_failure_stack() {
    let original = vec![galfus_contract::ExecutionFrame {
        module_id: 7,
        function_id: 8,
        instruction_offset: 9,
    }];
    let failure = galfus_contract::ExecutionFailure::new(
        galfus_contract::ExecutionFailureKind::ProviderFailure,
        "provider failed",
    )
    .with_stack(original.clone());

    assert_eq!(
        with_execution_stack(
            failure,
            vec![galfus_contract::ExecutionFrame {
                module_id: 1,
                function_id: 2,
                instruction_offset: 3,
            }],
        )
        .stack,
        original,
    );
}

fn module(types: Vec<BytecodeType>) -> BytecodeModule {
    BytecodeModule {
        name: "test".to_string(),
        global_count: 0,
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
fn surface_future_value_materializes_directly_in_the_waiting_heap() {
    let module = module(vec![BytecodeType::Int64]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    let contract = SurfaceContract::new(
        "std/time::__provider_time_now:return",
        1,
        SurfaceDirection::FromProvider,
        SurfaceSchema::I64,
    );

    let value = encode_future_value_into_thread_heap(
        &mut heap,
        FutureValue::Surface {
            contract,
            value: SurfaceValue::I64(42),
            adapter_binding_id: None,
        },
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("surface value materializes without a legacy boundary conversion");

    assert_eq!(value, VmValue::Int64(42));
}

#[test]
fn adapter_handle_surface_value_keeps_its_adapter_binding_in_the_waiting_heap() {
    let type_id = galfus_core::OpaqueTypeId::new("graphics", "Texture").unwrap();
    let module = module(vec![BytecodeType::AdapterHandle(type_id.clone())]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    let contract = SurfaceContract::new(
        "graphics.gfp::acquire:return",
        1,
        SurfaceDirection::FromProvider,
        SurfaceSchema::Handle {
            resource: "graphics::Texture".to_string(),
        },
    );

    let value = encode_future_value_into_thread_heap(
        &mut heap,
        FutureValue::Surface {
            contract,
            value: SurfaceValue::Handle(SurfaceHandle {
                type_id: type_id.clone(),
                id: galfus_core::HandleId::new(7),
            }),
            adapter_binding_id: Some(galfus_core::BindingId::new(3)),
        },
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("adapter handle surface value materializes in the waiting heap");

    let VmValue::Object(reference) = value else {
        panic!("adapter handle must be represented by a heap object");
    };
    assert!(matches!(
        heap.get_object(reference),
        Ok(HeapObject::AdapterHandle {
            binding_id,
            type_id: actual_type_id,
            id,
        }) if *binding_id == galfus_core::BindingId::new(3)
            && actual_type_id == &type_id
            && *id == galfus_core::HandleId::new(7)
    ));
}

#[test]
fn surface_argument_reads_directly_from_the_calling_heap() {
    let module = module(vec![BytecodeType::Uint8, BytecodeType::Array(TypeIdx(0))]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    let reference = heap
        .alloc(HeapObject::Array {
            module_id: galfus_core::ModuleId::new(1),
            element_ty: TypeIdx(0),
            elements: vec![VmValue::Uint8(b'o'), VmValue::Uint8(b'k')],
        })
        .expect("test heap accepts bytes");

    let value = decode_surface_from_thread_heap(
        &heap,
        &SurfaceSchema::Bytes,
        VmValue::Object(reference),
        TypeIdx(1),
        &module,
    )
    .expect("surface argument decodes without a legacy boundary conversion");

    assert_eq!(value, SurfaceValue::Bytes(b"ok".to_vec()));
}

#[test]
fn future_array_materializes_inside_a_nullable_return_type() {
    let module = module(vec![
        BytecodeType::Uint8,
        BytecodeType::Array(TypeIdx(0)),
        BytecodeType::Nullable(TypeIdx(1)),
    ]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

    let value = encode_future_value_into_thread_heap(
        &mut heap,
        FutureValue::Array(vec![FutureValue::Uint8(b'o'), FutureValue::Uint8(b'k')]),
        TypeIdx(2),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("a non-null future array must materialize through Nullable<Array<u8>>");

    let VmValue::Object(reference) = value else {
        panic!("future array must be represented by a heap object");
    };
    assert!(matches!(
        heap.get_object(reference),
        Ok(HeapObject::Array { element_ty, elements, .. })
            if *element_ty == TypeIdx(0)
                && elements == &vec![VmValue::Uint8(b'o'), VmValue::Uint8(b'k')]
    ));
}

#[test]
fn future_value_materializes_each_scalar_and_function_variant() {
    let module = module(vec![
        BytecodeType::Int8,
        BytecodeType::Int16,
        BytecodeType::Int32,
        BytecodeType::Int64,
        BytecodeType::Uint8,
        BytecodeType::Uint16,
        BytecodeType::Uint32,
        BytecodeType::Uint64,
        BytecodeType::Float32,
        BytecodeType::Float64,
        BytecodeType::Bool,
        BytecodeType::Null,
        BytecodeType::Function {
            params: vec![],
            ret: TypeIdx(11),
        },
    ]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    let values = vec![
        (FutureValue::Int8(-8), TypeIdx(0), VmValue::Int8(-8)),
        (FutureValue::Int16(-16), TypeIdx(1), VmValue::Int16(-16)),
        (FutureValue::I32(-32), TypeIdx(2), VmValue::Int32(-32)),
        (FutureValue::I64(-64), TypeIdx(3), VmValue::Int64(-64)),
        (FutureValue::Uint8(8), TypeIdx(4), VmValue::Uint8(8)),
        (FutureValue::Uint16(16), TypeIdx(5), VmValue::Uint16(16)),
        (FutureValue::Uint32(32), TypeIdx(6), VmValue::Uint32(32)),
        (FutureValue::Uint64(64), TypeIdx(7), VmValue::Uint64(64)),
        (FutureValue::F32(1.5), TypeIdx(8), VmValue::Float32(1.5)),
        (FutureValue::F64(2.5), TypeIdx(9), VmValue::Float64(2.5)),
        (FutureValue::Bool(true), TypeIdx(10), VmValue::Bool(true)),
        (FutureValue::Null, TypeIdx(11), VmValue::Null),
        (
            FutureValue::Function {
                module_id: 7,
                func_idx: 9,
            },
            TypeIdx(12),
            VmValue::Function {
                module_id: galfus_core::ModuleId::new(7),
                func_idx: FuncIdx(9),
            },
        ),
    ];

    for (value, expected_type, expected) in values {
        assert_eq!(
            encode_future_value_into_thread_heap(
                &mut heap,
                value,
                expected_type,
                galfus_core::ModuleId::new(1),
                &module,
            )
            .expect("future scalar must materialize into its matching bytecode type"),
            expected,
        );
    }
}

#[test]
fn future_handle_materializes_with_its_adapter_binding() {
    let type_id = galfus_core::OpaqueTypeId::new("graphics", "Texture").unwrap();
    let module = module(vec![BytecodeType::AdapterHandle(type_id.clone())]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

    let value = encode_future_value_into_thread_heap(
        &mut heap,
        FutureValue::Handle {
            binding_id: galfus_core::BindingId::new(3),
            type_id: type_id.clone(),
            id: galfus_core::HandleId::new(7),
        },
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("future handle must materialize into an adapter handle");

    let VmValue::Object(reference) = value else {
        panic!("future handle must be represented by a heap object");
    };
    assert!(matches!(
        heap.get_object(reference),
        Ok(HeapObject::AdapterHandle {
            binding_id,
            type_id: actual_type_id,
            id,
        }) if *binding_id == galfus_core::BindingId::new(3)
            && actual_type_id == &type_id
            && *id == galfus_core::HandleId::new(7)
    ));
}

#[test]
fn future_value_materializes_nested_choice_struct_tuple_and_array() {
    let mut module = module(vec![
        BytecodeType::Int32,
        BytecodeType::Uint8,
        BytecodeType::Array(TypeIdx(1)),
        BytecodeType::Tuple(vec![TypeIdx(0), TypeIdx(2)]),
        BytecodeType::Struct(StructLayoutIdx(0)),
        BytecodeType::Choice(ChoiceLayoutIdx(0)),
    ]);
    module.struct_layouts.push(StructLayout {
        name: "Envelope".to_string(),
        fields: vec![FieldLayout {
            name: "payload".to_string(),
            ty: TypeIdx(3),
            offset: 0,
            ownership: OwnershipKind::Value,
        }],
        constraints: vec![],
    });
    module.choice_layouts.push(ChoiceLayout {
        name: "Result".to_string(),
        variants: vec![
            ChoiceVariantLayout {
                name: "Empty".to_string(),
                payload_ty: None,
            },
            ChoiceVariantLayout {
                name: "Data".to_string(),
                payload_ty: Some(TypeIdx(4)),
            },
        ],
    });
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

    let value = encode_future_value_into_thread_heap(
        &mut heap,
        FutureValue::Choice {
            variant_idx: 1,
            payload: Some(Box::new(FutureValue::Struct(vec![FutureValue::Tuple(
                vec![
                    FutureValue::I32(7),
                    FutureValue::Array(vec![FutureValue::Uint8(b'x')]),
                ],
            )]))),
        },
        TypeIdx(5),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("nested future values must materialize from their expected bytecode layouts");

    let VmValue::Object(choice_reference) = value else {
        panic!("future choice must be represented by a heap object");
    };
    let Ok(HeapObject::Choice {
        layout_idx,
        variant_idx,
        payload: VmValue::Object(struct_reference),
        ..
    }) = heap.get_object(choice_reference)
    else {
        panic!("future value must preserve the choice layout and payload");
    };
    assert_eq!(*layout_idx, ChoiceLayoutIdx(0));
    assert_eq!(*variant_idx, 1);

    let Ok(HeapObject::Struct {
        layout_idx, fields, ..
    }) = heap.get_object(*struct_reference)
    else {
        panic!("choice payload must be the expected struct");
    };
    assert_eq!(*layout_idx, StructLayoutIdx(0));
    let [VmValue::Object(tuple_reference)] = fields.as_slice() else {
        panic!("struct payload field must be a tuple");
    };
    let Ok(HeapObject::Tuple { elements }) = heap.get_object(*tuple_reference) else {
        panic!("struct payload field must materialize as a tuple");
    };
    assert_eq!(elements[0], VmValue::Int32(7));
    let VmValue::Object(array_reference) = elements[1] else {
        panic!("tuple second element must be an array");
    };
    assert!(matches!(
        heap.get_object(array_reference),
        Ok(HeapObject::Array { elements, .. }) if elements == &vec![VmValue::Uint8(b'x')]
    ));
}
