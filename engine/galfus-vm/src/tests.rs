use std::sync;

use super::*;

#[test]
fn test_vm_creation() {
    let image = galfus_bytecode::BytecodeModule {
        name: "test".to_string(),
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
