use super::{
    decode_surface_from_thread_heap, encode_future_value_into_thread_heap, execution_stack,
    with_execution_stack,
};
use crate::event::FutureValue;
use galfus_bytecode::instruction::{FuncIdx, TypeIdx};
use galfus_bytecode::{BytecodeModule, BytecodeType, ConstantPool};
use galfus_contract::{SurfaceContract, SurfaceDirection, SurfaceSchema, SurfaceValue};
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
        },
        TypeIdx(0),
        galfus_core::ModuleId::new(1),
        &module,
    )
    .expect("surface value materializes without a legacy boundary conversion");

    assert_eq!(value, VmValue::Int64(42));
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
