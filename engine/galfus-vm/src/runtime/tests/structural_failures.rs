use super::*;
use galfus_bytecode::{
    BytecodeNode, ExportKind, ExportSlot, ImportEdge, ImportKind, ImportResolutionMode, ImportSlot,
    Instruction,
};
use galfus_core::{ModuleId, RuntimeExportId, RuntimeExportKind};
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
fn call_to_module_outside_the_ready_registry_returns_module_not_ready_error() {
    let graph = BytecodeGraph::new();
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = crate::thread::VmThreadState::test_new();

    let result = vm.run_function(&mut thread, ModuleId::new(99), FuncIdx(0), vec![]);
    assert!(
        matches!(
            result,
            Err(crate::error::VmPanic { error: VmError::ModuleNotReady { module_id: m_id }, .. }) if m_id == ModuleId::new(99)
        ),
        "Expected ModuleNotReady, got {:?}",
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
fn create_future_for_module_outside_the_ready_registry_returns_module_not_ready_error() {
    let graph = BytecodeGraph::new();
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = crate::thread::VmThreadState::test_new();

    thread
        .push_frame(
            sync::Arc::new(node(
                ModuleId::new(99),
                create_test_module(vec![Instruction::RetNull], vec![]),
            )),
            FuncIdx(0),
            0,
            None,
            0,
        )
        .unwrap();

    let step = vm.execute_system_instruction(
        &mut thread,
        &Instruction::CreateFuture {
            dest: Reg(0),
            func: FuncIdx(0),
            args_start: Reg(0),
            arg_count: 0,
            arg_types: Box::new([]),
            return_type: galfus_bytecode::TypeIdx(0),
        },
    );
    assert!(
        matches!(
            step,
            Err(VmError::ModuleNotReady { module_id: m_id }) if m_id == ModuleId::new(99)
        ),
        "Expected ModuleNotReady"
    );
}

#[test]
fn ready_module_registry_resolves_sparse_module_ids() {
    let first_module_id = ModuleId::new(7);
    let second_module_id = ModuleId::new(50_000);
    let graph = sync::Arc::new(graph_with_nodes(
        galfus_core::SemanticRevision::new(0),
        vec![
            node(
                first_module_id,
                create_test_module(vec![Instruction::RetNull], vec![]),
            ),
            node(
                second_module_id,
                create_test_module(vec![Instruction::RetNull], vec![]),
            ),
        ],
    ));
    let vm = VirtualMachine::from_ready_modules(
        graph.clone(),
        [
            graph
                .node_handle(first_module_id)
                .expect("first ready module exists"),
            graph
                .node_handle(second_module_id)
                .expect("second ready module exists"),
        ],
    );

    assert_eq!(
        vm.get_module(first_module_id)
            .expect("first sparse module is ready")
            .name,
        "test"
    );
    assert_eq!(
        vm.get_module(second_module_id)
            .expect("second sparse module is ready")
            .name,
        "test"
    );
}

#[test]
fn direct_function_call_rejects_an_imported_global_before_execution() {
    let importer = ModuleId::new(7);
    let target = ModuleId::new(31);
    let mut importer_module = create_test_module(
        vec![
            Instruction::Call {
                dest: Reg(0),
                func: FuncIdx(1),
                args_start: Reg(0),
                arg_count: 0,
            },
            Instruction::Ret { src: Reg(0) },
        ],
        vec![],
    );
    importer_module.imports = vec![ImportSlot {
        module_name: format!("module-{}.gfs", target.raw()),
        symbol_name: "value".to_string(),
        ty: TypeIdx(0),
        kind: ImportKind::Global,
        target_module_id: Some(target),
        target_export_id: Some(RuntimeExportId::new(
            target,
            RuntimeExportKind::Global,
            "value",
        )),
    }];
    let mut target_module = create_test_module(vec![Instruction::RetNull], vec![]);
    target_module.global_count = 1;
    target_module.exports = vec![ExportSlot {
        symbol_name: "value".to_string(),
        kind: ExportKind::Global(galfus_bytecode::GlobalIdx(0)),
    }];
    let graph = BytecodeGraph::from_modules(
        galfus_core::SemanticRevision::new(0),
        vec![node(importer, importer_module), node(target, target_module)],
        vec![ImportEdge {
            from: importer,
            to: target,
        }],
    )
    .expect("valid graph with a direct global import");
    let vm = VirtualMachine::new(sync::Arc::new(graph))
        .with_import_resolution_mode(ImportResolutionMode::Direct);
    let mut thread = crate::thread::VmThreadState::test_new();

    let result = vm.run_function(&mut thread, importer, FuncIdx(0), vec![]);

    assert!(matches!(
        result,
        Err(crate::error::VmPanic {
            error: VmError::ImportKindMismatch {
                module_id,
                slot: 0,
                expected: RuntimeExportKind::Function,
                actual: RuntimeExportKind::Global,
            },
            ..
        }) if module_id == importer
    ));
}

#[test]
fn ret_uses_frame_owned_code_after_the_graph_module_is_unavailable() {
    let graph = BytecodeGraph::new();
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = crate::thread::VmThreadState::test_new();

    thread
        .push_frame(
            sync::Arc::new(node(
                ModuleId::new(99),
                create_test_module(vec![Instruction::RetNull], vec![]),
            )),
            FuncIdx(0),
            0,
            None,
            0,
        )
        .unwrap();

    let step = vm.execute_control_instruction(&mut thread, &Instruction::RetNull);
    assert!(
        matches!(
            step,
            Ok(VmStep::Return { value: Value::Null, module_id, return_type: TypeIdx(0) })
                if module_id == ModuleId::new(99)
        ),
        "Expected frame-owned code to provide the return type"
    );
}
