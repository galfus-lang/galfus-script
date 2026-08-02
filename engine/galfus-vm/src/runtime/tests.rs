mod arithmetic_and_control;
mod io_and_arrays;
mod module_state;
mod objects_and_types;
mod ownership;

use super::*;
use galfus_bytecode::BytecodeModule;
use galfus_bytecode::instruction::{ConstIdx, FieldIdx};
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeNode, ChoiceLayout, ChoiceVariantLayout, ConstantPool,
    FieldLayout, OwnershipKind, StructLayout,
};

fn graph_with_node(node: BytecodeNode) -> BytecodeGraph {
    graph_with_nodes(node.semantic_revision, vec![node])
}

fn graph_with_nodes(
    semantic_revision: galfus_core::SemanticRevision,
    nodes: Vec<BytecodeNode>,
) -> BytecodeGraph {
    BytecodeGraph::from_modules(semantic_revision, nodes, Vec::new())
        .expect("test module must form a valid bytecode graph")
}

fn create_test_module(instructions: Vec<Instruction>, constants: Vec<Constant>) -> BytecodeModule {
    BytecodeModule {
        name: "test".to_string(),
        constants: ConstantPool { constants },
        functions: vec![BytecodeFunction {
            name: "main".to_string(),
            param_count: 0,
            local_count: 8,
            temp_count: 8,
            return_ty: TypeIdx(0),
            instructions,
        }],
        types: vec![
            BytecodeType::Int64,                               // TypeIdx(0)
            BytecodeType::Bool,                                // TypeIdx(1)
            BytecodeType::Null,                                // TypeIdx(2)
            BytecodeType::Struct(StructLayoutIdx(0)),          // TypeIdx(3)
            BytecodeType::Array(TypeIdx(0)),                   // TypeIdx(4)
            BytecodeType::Tuple(vec![TypeIdx(0), TypeIdx(1)]), // TypeIdx(5)
            BytecodeType::Choice(ChoiceLayoutIdx(0)),          // TypeIdx(6)
            BytecodeType::Uint8,                               // TypeIdx(7)
        ],
        struct_layouts: vec![StructLayout {
            name: "Point".to_string(),
            fields: vec![
                FieldLayout {
                    name: "x".to_string(),
                    ty: TypeIdx(0),
                    offset: 0,
                    ownership: OwnershipKind::Value,
                },
                FieldLayout {
                    name: "y".to_string(),
                    ty: TypeIdx(0),
                    offset: 8,
                    ownership: OwnershipKind::Value,
                },
            ],
            constraints: vec![],
        }],
        choice_layouts: vec![ChoiceLayout {
            name: "OptionInt".to_string(),
            variants: vec![
                ChoiceVariantLayout {
                    name: "None".to_string(),
                    payload_ty: None,
                },
                ChoiceVariantLayout {
                    name: "Some".to_string(),
                    payload_ty: Some(TypeIdx(0)),
                },
            ],
        }],
        imports: Vec::new(),
        exports: Vec::new(),
        init_func_idx: None,
    }
}

#[test]
fn provider_continuation_rejects_a_result_that_violates_its_declared_type() {
    let module_id = galfus_core::ModuleId::new(1);
    let graph = graph_with_node(BytecodeNode {
        id: module_id,
        path: galfus_core::ModulePath::new("test.gfs").expect("valid module path"),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: create_test_module(vec![Instruction::RetNull], vec![]),
        metadata: None,
    });
    let vm = VirtualMachine::new(std::sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::new();
    vm.prepare_function(&mut thread, module_id, FuncIdx(0), vec![])
        .expect("function is valid");

    let continuation = Continuation::for_provider(Reg(0), module_id, TypeIdx(0)).with_origin(1);
    let error = vm
        .resume(1, &mut thread, continuation.clone(), Value::Bool(true))
        .expect_err("bool does not satisfy the declared int64 result type");
    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::InvalidContinuation
    );

    let duplicate = vm
        .resume(1, &mut thread, continuation, Value::Int64(1))
        .expect_err("a failed resume attempt consumes the continuation");
    assert_eq!(
        duplicate.kind,
        galfus_contract::ExecutionFailureKind::DuplicateCompletion
    );
}

#[test]
fn await_future_suspends_and_resumes_through_a_vm_owned_continuation() {
    let module_id = galfus_core::ModuleId::new(1);
    let graph = graph_with_node(BytecodeNode {
        id: module_id,
        path: galfus_core::ModulePath::new("test.gfs").expect("valid module path"),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: create_test_module(
            vec![
                Instruction::AwaitFuture {
                    dest: Reg(1),
                    future_id: Reg(0),
                    return_type: TypeIdx(0),
                },
                Instruction::Ret { src: Reg(1) },
            ],
            vec![],
        ),
        metadata: None,
    });
    let vm = VirtualMachine::new(std::sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::new();
    vm.prepare_function(&mut thread, module_id, FuncIdx(0), vec![])
        .expect("function is valid");
    thread
        .write_reg(Reg(0), Value::Future(42))
        .expect("future handle fits in the register");

    let VmStep::Suspend {
        effect:
            VmEffect::FutureWait {
                future_id,
                module_id: effect_module_id,
                return_type,
            },
        continuation,
    } = vm
        .execute_with_budget(&mut thread, 1)
        .expect("await reaches the suspension point")
    else {
        panic!("await instruction must suspend as a future effect");
    };
    assert_eq!(future_id, 42);
    assert_eq!(effect_module_id, module_id);
    assert_eq!(return_type, TypeIdx(0));

    vm.resume(1, &mut thread, continuation.with_origin(1), Value::Int64(7))
        .expect("future result resumes the continuation");
    assert!(matches!(
        vm.execute_with_budget(&mut thread, 1),
        Ok(VmStep::Return {
            value: Value::Int64(7),
            ..
        })
    ));
}

#[test]
fn dropping_the_last_future_handle_notifies_the_orchestrator() {
    let module_id = galfus_core::ModuleId::new(1);
    let graph = graph_with_node(BytecodeNode {
        id: module_id,
        path: galfus_core::ModulePath::new("future_drop.gfs").expect("valid module path"),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: create_test_module(
            vec![Instruction::Drop { reg: Reg(0) }, Instruction::RetNull],
            vec![],
        ),
        metadata: None,
    });
    let vm = VirtualMachine::new(std::sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::new();
    vm.prepare_function(&mut thread, module_id, FuncIdx(0), vec![])
        .expect("function is valid");
    thread
        .write_reg(Reg(0), Value::Future(7))
        .expect("register exists");

    let VmStep::Suspend {
        effect: VmEffect::FutureDropped { future_id },
        continuation,
    } = vm
        .execute_with_budget(&mut thread, 1)
        .expect("drop reaches the notification point")
    else {
        panic!("last future handle must notify the runtime");
    };
    assert_eq!(future_id, 7);
    vm.resume(1, &mut thread, continuation.with_origin(1), Value::Null)
        .expect("drop continuation resumes");
}

#[test]
fn dropping_one_of_multiple_future_handles_keeps_the_future_alive() {
    let module_id = galfus_core::ModuleId::new(1);
    let graph = graph_with_node(BytecodeNode {
        id: module_id,
        path: galfus_core::ModulePath::new("future_drop_retained.gfs").expect("valid module path"),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: create_test_module(vec![Instruction::Drop { reg: Reg(0) }], vec![]),
        metadata: None,
    });
    let vm = VirtualMachine::new(std::sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::new();
    vm.prepare_function(&mut thread, module_id, FuncIdx(0), vec![])
        .expect("function is valid");
    thread
        .write_reg(Reg(0), Value::Future(7))
        .expect("register exists");
    thread
        .write_reg(Reg(1), Value::Future(7))
        .expect("register exists");

    assert!(matches!(
        vm.execute_with_budget(&mut thread, 1),
        Ok(VmStep::Continue)
    ));
}
