use std::sync::Arc;

use galfus_bytecode::{
    BytecodeGraph, BytecodeModule, BytecodeNode, Constant, ExportKind, ExportSlot, ImportEdge,
    ImportKind, ImportSlot,
};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};

use super::*;

fn node(id: ModuleId, path: &str, module: BytecodeModule) -> BytecodeNode {
    BytecodeNode {
        id,
        path: ModulePath::new(path).expect("valid module path"),
        semantic_revision: SemanticRevision::new(0),
        module,
        metadata: None,
    }
}

#[test]
fn nested_module_calls_keep_each_frame_code_alive() {
    let caller_id = ModuleId::new(7);
    let callee_id = ModuleId::new(19);
    let mut caller = create_test_module(
        vec![
            Instruction::Call {
                dest: Reg(0),
                func: FuncIdx(1),
                args_start: Reg(0),
                arg_count: 0,
            },
            Instruction::Ret { src: Reg(0) },
        ],
        Vec::new(),
    );
    caller.imports = vec![ImportSlot {
        module_name: "callee.gfs".to_string(),
        symbol_name: "value".to_string(),
        ty: TypeIdx(0),
        kind: ImportKind::Function,
        target_module_id: None,
        target_export_id: None,
    }];
    let mut callee = create_test_module(
        vec![
            Instruction::LoadConst {
                dest: Reg(0),
                const_idx: ConstIdx(0),
            },
            Instruction::Ret { src: Reg(0) },
        ],
        vec![Constant::Int64(41)],
    );
    callee.exports = vec![ExportSlot {
        symbol_name: "value".to_string(),
        kind: ExportKind::Function(FuncIdx(0)),
    }];
    let graph = Arc::new(
        BytecodeGraph::from_modules(
            SemanticRevision::new(0),
            vec![
                node(caller_id, "caller.gfs", caller),
                node(callee_id, "callee.gfs", callee),
            ],
            vec![ImportEdge {
                from: caller_id,
                to: callee_id,
            }],
        )
        .expect("valid graph"),
    );
    let caller_code = graph.node_handle(caller_id).expect("caller exists");
    let callee_code = graph.node_handle(callee_id).expect("callee exists");
    let vm = VirtualMachine::new(graph);
    let mut thread = thread::VmThreadState::test_new();

    vm.prepare_function(&mut thread, caller_id, FuncIdx(0), Vec::new())
        .expect("caller prepares");
    assert!(Arc::ptr_eq(&thread.call_stack[0].code, &caller_code));

    assert!(matches!(
        vm.execute_with_budget(&mut thread, 1),
        Ok(VmStep::Continue)
    ));
    assert_eq!(thread.call_stack.len(), 2);
    assert!(Arc::ptr_eq(&thread.call_stack[0].code, &caller_code));
    assert!(Arc::ptr_eq(&thread.call_stack[1].code, &callee_code));

    assert!(matches!(
        vm.execute_with_budget(&mut thread, 8),
        Ok(VmStep::Return {
            value: Value::Int64(41),
            module_id,
            return_type: TypeIdx(0),
        }) if module_id == caller_id
    ));
}

#[test]
fn frame_holds_only_its_executing_module_after_graph_drop() {
    let executing_id = ModuleId::new(7);
    let unrelated_id = ModuleId::new(19);
    let graph = Arc::new(
        BytecodeGraph::from_modules(
            SemanticRevision::new(0),
            vec![
                node(
                    executing_id,
                    "executing.gfs",
                    create_test_module(vec![Instruction::RetNull], Vec::new()),
                ),
                node(
                    unrelated_id,
                    "unrelated.gfs",
                    create_test_module(vec![Instruction::RetNull], Vec::new()),
                ),
            ],
            Vec::new(),
        )
        .expect("valid graph"),
    );
    let executing_code = graph
        .node_handle(executing_id)
        .expect("executing node exists");
    let unrelated_code = graph
        .node_handle(unrelated_id)
        .expect("unrelated node exists");
    let vm = VirtualMachine::new(graph.clone());
    let mut thread = thread::VmThreadState::test_new();

    vm.prepare_function(&mut thread, executing_id, FuncIdx(0), Vec::new())
        .expect("function prepares");
    drop(vm);
    drop(graph);

    assert!(Arc::ptr_eq(&thread.call_stack[0].code, &executing_code));
    assert_eq!(Arc::strong_count(&executing_code), 2);
    assert_eq!(Arc::strong_count(&unrelated_code), 1);
}
