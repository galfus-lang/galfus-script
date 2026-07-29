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
    let mut thread = thread::VirtualThread::new();
    vm.prepare_function(&mut thread, module_id, FuncIdx(0), vec![])
        .expect("function is valid");

    let continuation = Continuation::for_provider(Reg(0), module_id, TypeIdx(0));
    let error = vm
        .resume(&mut thread, continuation.clone(), Value::Bool(true))
        .expect_err("bool does not satisfy the declared int64 result type");
    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::InvalidContinuation
    );

    let duplicate = vm
        .resume(&mut thread, continuation, Value::Int64(1))
        .expect_err("a failed resume attempt consumes the continuation");
    assert_eq!(
        duplicate.kind,
        galfus_contract::ExecutionFailureKind::DuplicateCompletion
    );
}
