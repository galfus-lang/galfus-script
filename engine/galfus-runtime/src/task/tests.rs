use super::{
    decode_from_thread_heap, encode_into_thread_heap, execution_stack, with_execution_stack,
};
use galfus_bytecode::instruction::{FuncIdx, TypeIdx};
use galfus_bytecode::{
    BytecodeModule, BytecodeType, ChoiceLayout, ChoiceVariantLayout, ConstantPool,
};
use galfus_contract::{BoundaryType, BoundaryValue};
use galfus_core::{BindingId, HandleId, OpaqueTypeId};
use galfus_vm::{HeapObject, VmValue};

#[test]
fn execution_stack_preserves_the_suspended_call_chain() {
    let mut thread = galfus_vm::thread::VmThreadState::test_new();
    thread.call_stack = vec![
        galfus_vm::runtime::CallFrame {
            module_id: galfus_core::ModuleId::new(1),
            func_idx: FuncIdx(2),
            pc: 4,
            registers: vec![],
            return_dest: None,
        },
        galfus_vm::runtime::CallFrame {
            module_id: galfus_core::ModuleId::new(3),
            func_idx: FuncIdx(5),
            pc: 0,
            registers: vec![],
            return_dest: None,
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
fn codec_preserves_the_declared_type_of_an_empty_array() {
    let module = module(vec![BytecodeType::Int32, BytecodeType::Array(TypeIdx(0))]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    let reference = heap.alloc(HeapObject::Array {
        element_ty: TypeIdx(0),
        elements: vec![],
    });

    let value = decode_from_thread_heap(
        &heap,
        VmValue::Object(reference.unwrap()),
        TypeIdx(1),
        &module,
    )
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
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

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
fn codec_rejects_an_array_with_a_different_declared_element_type() {
    let module = module(vec![BytecodeType::Int32, BytecodeType::Array(TypeIdx(0))]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

    assert!(
        encode_into_thread_heap(
            &mut heap,
            BoundaryValue::Array {
                element_type: BoundaryType::U8,
                values: vec![],
            },
            TypeIdx(1),
            galfus_core::ModuleId::new(1),
            &module,
        )
        .is_err()
    );
}

#[test]
fn codec_round_trips_nullable_values() {
    let module = module(vec![
        BytecodeType::Int32,
        BytecodeType::Nullable(TypeIdx(0)),
    ]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

    for value in [BoundaryValue::I32(7), BoundaryValue::Null] {
        let encoded = encode_into_thread_heap(
            &mut heap,
            value.clone(),
            TypeIdx(1),
            galfus_core::ModuleId::new(1),
            &module,
        )
        .expect("nullable value encodes");

        assert_eq!(
            decode_from_thread_heap(&heap, encoded, TypeIdx(1), &module),
            Ok(value)
        );
    }
}

#[test]
fn codec_normalizes_float_values_at_the_host_boundary() {
    let module = module(vec![BytecodeType::Float64]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

    let negative_zero = encode_into_thread_heap(
        &mut heap,
        BoundaryValue::F64(-0.0),
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("negative zero encodes");
    assert_eq!(negative_zero, VmValue::Float64(0.0));

    let non_canonical_nan = encode_into_thread_heap(
        &mut heap,
        BoundaryValue::F64(f64::from_bits(0xFFF8_0000_0000_0001)),
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("NaN encodes");
    let BoundaryValue::F64(value) =
        decode_from_thread_heap(&heap, non_canonical_nan, TypeIdx(0), &module)
            .expect("NaN decodes")
    else {
        panic!("float codec must return an f64");
    };
    assert_eq!(value.to_bits(), galfus_core::CANONICAL_F64_NAN);
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
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();

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
fn codec_round_trips_nominal_adapter_handles() {
    let type_id = OpaqueTypeId::new("file", "File").unwrap();
    let module = module(vec![BytecodeType::AdapterHandle(type_id.clone())]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    let value = BoundaryValue::Handle {
        type_id,
        binding_id: Some(BindingId::new(1)),
        id: HandleId::new(9),
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

#[test]
fn codec_round_trips_function_references() {
    let module = module(vec![BytecodeType::Function {
        params: vec![],
        ret: TypeIdx(0),
    }]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    let value = BoundaryValue::Function {
        module_id: 3,
        func_idx: 7,
    };

    let encoded = encode_into_thread_heap(
        &mut heap,
        value.clone(),
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("function reference encodes");
    assert_eq!(
        decode_from_thread_heap(&heap, encoded, TypeIdx(0), &module),
        Ok(value)
    );
}

#[test]
fn codec_rejects_adapter_handles_with_the_wrong_kind() {
    let module = module(vec![BytecodeType::AdapterHandle(
        OpaqueTypeId::new("file", "File").unwrap(),
    )]);
    let mut heap = galfus_vm::thread::PrivateHeap::test_new();
    assert!(
        encode_into_thread_heap(
            &mut heap,
            BoundaryValue::Handle {
                type_id: OpaqueTypeId::new("socket", "Socket").unwrap(),
                binding_id: Some(BindingId::new(1)),
                id: HandleId::new(9),
            },
            TypeIdx(0),
            galfus_core::ModuleId::new(1),
            &module,
        )
        .is_err()
    );
}

#[test]
fn boundary_type_preserves_nullable_wrapper() {
    let module = module(vec![
        BytecodeType::Int32,
        BytecodeType::Nullable(TypeIdx(0)),
    ]);

    let result = module.boundary_type(TypeIdx(1));
    assert_eq!(
        result,
        Ok(galfus_contract::BoundaryType::Nullable(Box::new(
            galfus_contract::BoundaryType::I32
        )))
    );
}
