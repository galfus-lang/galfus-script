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
    let mut thread = crate::thread::VmThreadState::test_new();

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
    let mut thread = crate::thread::VmThreadState::test_new();

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

#[test]
fn create_future_for_missing_module_returns_module_not_found_error() {
    let graph = BytecodeGraph::new();
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = crate::thread::VmThreadState::test_new();

    thread.call_stack.push(crate::CallFrame {
        module_id: ModuleId::new(99),
        func_idx: FuncIdx(0),
        pc: 0,
        return_dest: None,
        registers: vec![],
    });

    let step = vm.execute_system_instruction(
        &mut thread,
        Instruction::CreateFuture {
            dest: Reg(0),
            func: FuncIdx(0),
            args_start: Reg(0),
            arg_count: 0,
            arg_types: vec![],
            return_type: galfus_bytecode::TypeIdx(0),
        },
    );
    assert!(
        matches!(
            step,
            Err(VmError::ModuleNotFound { module_id: m_id }) if m_id == ModuleId::new(99)
        ),
        "Expected ModuleNotFound"
    );
}

#[test]
fn ret_from_missing_module_returns_module_not_found_error() {
    let graph = BytecodeGraph::new();
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = crate::thread::VmThreadState::test_new();

    thread.call_stack.push(crate::CallFrame {
        module_id: ModuleId::new(99),
        func_idx: FuncIdx(0),
        pc: 0,
        return_dest: None,
        registers: vec![],
    });

    let step = vm.execute_control_instruction(&mut thread, Instruction::RetNull);
    assert!(
        matches!(
            step,
            Err(VmError::ModuleNotFound { module_id: m_id }) if m_id == ModuleId::new(99)
        ),
        "Expected ModuleNotFound"
    );
}
