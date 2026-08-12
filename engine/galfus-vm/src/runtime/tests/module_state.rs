use std::sync;

use crate::thread;

use super::*;
use galfus_bytecode::GlobalIdx;

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
fn globals_with_the_same_index_are_isolated_by_module() {
    let first = galfus_core::ModuleId::new(1);
    let second = galfus_core::ModuleId::new(2);
    let first_module = create_test_module(
        vec![
            Instruction::LoadConst {
                dest: Reg(0),
                const_idx: ConstIdx(0),
            },
            Instruction::StoreGlobal {
                module_id: first,
                global_idx: GlobalIdx(0),
                src: Reg(0),
            },
            Instruction::LoadGlobal {
                dest: Reg(1),
                module_id: first,
                global_idx: GlobalIdx(0),
            },
            Instruction::Ret { src: Reg(1) },
        ],
        vec![Constant::Int64(10)],
    );
    let second_module = create_test_module(
        vec![
            Instruction::LoadConst {
                dest: Reg(0),
                const_idx: ConstIdx(0),
            },
            Instruction::StoreGlobal {
                module_id: second,
                global_idx: GlobalIdx(0),
                src: Reg(0),
            },
            Instruction::LoadGlobal {
                dest: Reg(1),
                module_id: second,
                global_idx: GlobalIdx(0),
            },
            Instruction::Ret { src: Reg(1) },
        ],
        vec![Constant::Int64(20)],
    );
    let graph = graph_with_nodes(
        galfus_core::SemanticRevision::new(0),
        vec![node(first, first_module), node(second, second_module)],
    );
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::test_new();

    assert_eq!(
        vm.run_function(&mut thread, first, FuncIdx(0), vec![]),
        Ok(Value::Int64(10))
    );
    assert_eq!(
        vm.run_function(&mut thread, second, FuncIdx(0), vec![]),
        Ok(Value::Int64(20))
    );
    assert_eq!(
        thread.module_state(first).map(|state| &state.globals),
        Some(&vec![Value::Int64(10)])
    );
    assert_eq!(
        thread.module_state(second).map(|state| &state.globals),
        Some(&vec![Value::Int64(20)])
    );
}
