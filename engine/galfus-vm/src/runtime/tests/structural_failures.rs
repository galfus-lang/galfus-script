use super::*;
use galfus_bytecode::{BytecodeNode, Instruction};
use galfus_core::ModuleId;
use std::sync;

fn node(id: galfus_core::ModuleId, module: BytecodeModule) -> BytecodeNode {
    BytecodeNode {
        id,
        path: galfus_core::ModulePath::new(format!("module-{}.gfs", id.raw()).as_str())
            .expect("valid module path"),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module,
        metadata: None,
    }
}

#[test]
fn call_to_missing_module_returns_module_not_found_error() {
    let graph = BytecodeGraph::new();
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = crate::thread::VmThreadState::new();

    let result = vm.run_function(&mut thread, ModuleId::new(99), FuncIdx(0), vec![]);
    assert!(
        matches!(
            result,
            Err(crate::error::VmPanic { error: VmError::ModuleNotFound { module_id: m_id }, .. }) if m_id == ModuleId::new(99)
        ),
        "Expected ModuleNotFound, got {:?}",
        result
    );
}

#[test]
fn call_to_missing_function_returns_function_out_of_bounds_error() {
    let module_id = ModuleId::new(1);
    let module = create_test_module(
        vec![
            Instruction::LoadNull { dest: Reg(0) },
            Instruction::Ret { src: Reg(0) },
        ],
        vec![],
    );
    let graph = graph_with_node(node(module_id, module));
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = crate::thread::VmThreadState::new();

    let result = vm.run_function(&mut thread, module_id, FuncIdx(99), vec![]);
    assert!(
        matches!(
            result,
            Err(crate::error::VmPanic { error: VmError::FunctionOutOfBounds { index: i }, .. }) if i == FuncIdx(99)
        ),
        "Expected FunctionOutOfBounds, got {:?}",
        result
    );
}
