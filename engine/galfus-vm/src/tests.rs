use std::sync;

use super::*;

#[test]
fn thread_execution_storage_is_allocated_on_demand() {
    let mut thread = thread::VmThreadState::test_new();

    assert!(thread.call_stack.is_empty());
    assert!(thread.registers.is_empty());

    thread
        .push_frame(
            galfus_core::ModuleId::new(0),
            galfus_bytecode::FuncIdx(0),
            0,
            None,
            1,
            &[] as *const [galfus_bytecode::Instruction],
        )
        .expect("a frame within the quota must allocate its registers");

    assert_eq!(thread.call_stack.len(), 1);
    assert!(thread.registers.len() >= 1);
}

#[test]
fn test_vm_creation() {
    let image = galfus_bytecode::BytecodeModule {
        name: "test".to_string(),
        global_count: 0,
        constants: galfus_bytecode::ConstantPool::default(),
        functions: vec![],
        types: vec![],
        struct_layouts: vec![],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![],
        init_func_idx: None,
    };
    let graph = galfus_bytecode::BytecodeGraph::from_modules(
        galfus_core::SemanticRevision::new(0),
        vec![galfus_bytecode::BytecodeNode {
            id: galfus_core::ModuleId::new(0),
            path: galfus_core::ModulePath::new("test.gfs").unwrap(),
            semantic_revision: galfus_core::SemanticRevision::new(0),
            module: image,
            metadata: None,
        }],
        vec![],
    )
    .expect("test module must form a valid bytecode graph");
    let _vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
}

#[test]
fn vm_rejects_an_unsupported_bytecode_format_before_execution() {
    let graph = galfus_bytecode::BytecodeGraph::with_format_version(
        galfus_bytecode::BytecodeFormatVersion::new(1, 0, 0),
    );
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::test_new();

    let panic = vm
        .prepare_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            galfus_bytecode::FuncIdx(0),
            vec![],
        )
        .expect_err("unsupported bytecode must not be interpreted");

    assert_eq!(
        panic.error,
        VmError::UnsupportedBytecodeFormat(galfus_bytecode::BytecodeFormatError {
            supported: galfus_bytecode::CURRENT_BYTECODE_FORMAT_VERSION,
            actual: galfus_bytecode::BytecodeFormatVersion::new(1, 0, 0),
        })
    );
}
