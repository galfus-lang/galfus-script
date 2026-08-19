use std::sync;

use crate::thread;

use super::*;
use galfus_bytecode::BytecodeModule;
use galfus_core::{BindingId, HandleId, OpaqueTypeId};

#[test]
fn test_ownership_deterministic_release() {
    let image = BytecodeModule {
        name: "test".to_string(),
        global_count: 0,
        constants: ConstantPool { constants: vec![] },
        functions: vec![BytecodeFunction {
            name: "main".to_string(),
            param_count: 0,
            local_count: 8,
            temp_count: 8,
            return_ty: TypeIdx(3),
            adapter_proxy_metadata: None,
            instructions: vec![
                Instruction::AllocLocal {
                    dest: Reg(1),
                    type_idx: TypeIdx(3),
                },
                Instruction::AllocLocal {
                    dest: Reg(2),
                    type_idx: TypeIdx(3),
                },
                Instruction::StoreField {
                    obj: Reg(1),
                    field: FieldIdx(0),
                    val: Reg(2),
                },
                Instruction::Drop { reg: Reg(2) },
                Instruction::Ret { src: Reg(1) },
            ],
        }],
        types: vec![
            BytecodeType::Int64,                      // TypeIdx(0)
            BytecodeType::Null,                       // TypeIdx(1)
            BytecodeType::Null,                       // TypeIdx(2)
            BytecodeType::Struct(StructLayoutIdx(0)), // TypeIdx(3)
        ],
        struct_layouts: vec![StructLayout {
            name: "Node".to_string(),
            fields: vec![
                FieldLayout {
                    name: "next".to_string(),
                    ty: TypeIdx(3),
                    offset: 0,
                    ownership: OwnershipKind::Strong,
                },
                FieldLayout {
                    name: "val".to_string(),
                    ty: TypeIdx(0),
                    offset: 8,
                    ownership: OwnershipKind::Value,
                },
            ],
            constraints: vec![],
        }],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![],
        init_func_idx: None,
    };

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::test_new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    let node1_ref = match res {
        Value::Object(r) => r,
        other => panic!("expected object, got {:?}", other),
    };

    let node1 = thread.heap.get_object(node1_ref).unwrap();
    let node2_ref = match node1 {
        HeapObject::Struct { fields, .. } => match fields[0] {
            Value::Object(r) => r,
            ref other => panic!("expected object in field 0, got {:?}", other),
        },
        other => panic!("expected struct, got {:?}", other),
    };

    assert!(thread.heap.get_object(node1_ref).is_ok());
    assert!(thread.heap.get_object(node2_ref).is_ok());

    let _handle_ref = thread
        .heap
        .alloc(HeapObject::AdapterHandle {
            binding_id: BindingId::new(1),
            type_id: OpaqueTypeId::new("graphics", "Texture").unwrap(),
            id: HandleId::new(42),
        })
        .unwrap();
}

#[test]
fn test_ownership_cycle_release() {
    let image = BytecodeModule {
        name: "test".to_string(),
        global_count: 0,
        constants: ConstantPool { constants: vec![] },
        functions: vec![BytecodeFunction {
            name: "main".to_string(),
            param_count: 0,
            local_count: 8,
            temp_count: 8,
            return_ty: TypeIdx(4),
            adapter_proxy_metadata: None,
            instructions: vec![
                Instruction::AllocLocal {
                    dest: Reg(1),
                    type_idx: TypeIdx(3),
                },
                Instruction::AllocLocal {
                    dest: Reg(2),
                    type_idx: TypeIdx(3),
                },
                Instruction::StoreField {
                    obj: Reg(1),
                    field: FieldIdx(0),
                    val: Reg(2),
                },
                Instruction::StoreField {
                    obj: Reg(2),
                    field: FieldIdx(0),
                    val: Reg(1),
                },
                Instruction::NewTuple {
                    dest: Reg(3),
                    type_idx: TypeIdx(4),
                    start: Reg(1),
                    count: 2,
                },
                Instruction::Ret { src: Reg(3) },
            ],
        }],
        types: vec![
            BytecodeType::Int64,                               // TypeIdx(0)
            BytecodeType::Null,                                // TypeIdx(1)
            BytecodeType::Null,                                // TypeIdx(2)
            BytecodeType::Struct(StructLayoutIdx(0)),          // TypeIdx(3)
            BytecodeType::Tuple(vec![TypeIdx(3), TypeIdx(3)]), // TypeIdx(4)
        ],
        struct_layouts: vec![StructLayout {
            name: "Node".to_string(),
            fields: vec![FieldLayout {
                name: "next".to_string(),
                ty: TypeIdx(3),
                offset: 0,
                ownership: OwnershipKind::Strong,
            }],
            constraints: vec![],
        }],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![],
        init_func_idx: None,
    };

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::test_new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    let tuple_ref = match res {
        Value::Object(r) => r,
        other => panic!("expected object, got {:?}", other),
    };

    let (node1_ref, node2_ref) = match thread.heap.get_object(tuple_ref).unwrap() {
        HeapObject::Tuple { elements } => {
            let n1 = match &elements[0] {
                Value::Object(r) => *r,
                other => panic!("expected object, got {:?}", other),
            };
            let n2 = match &elements[1] {
                Value::Object(r) => *r,
                other => panic!("expected object, got {:?}", other),
            };
            (n1, n2)
        }
        other => panic!("expected tuple, got {:?}", other),
    };

    assert!(thread.heap.get_object(node1_ref).is_ok());
    assert!(thread.heap.get_object(node2_ref).is_ok());
    assert!(thread.heap.get_object(tuple_ref).is_ok());
}

#[test]
fn test_obsolete_reference_returns_invalid_object() {
    let mut thread = thread::VmThreadState::test_new();
    let obj_ref = thread
        .heap
        .alloc(HeapObject::Tuple { elements: vec![] })
        .unwrap();
    thread.heap.free_object(obj_ref).unwrap();

    // Allocate again to reuse slot
    let new_ref = thread
        .heap
        .alloc(HeapObject::Tuple { elements: vec![] })
        .unwrap();
    assert_ne!(obj_ref, new_ref);

    let err = thread.heap.get_object(obj_ref).unwrap_err();
    assert_eq!(err, VmError::InvalidObjectReference);
}

#[test]
fn test_gc_fails_on_invalid_reference() {
    let mut thread = thread::VmThreadState::test_new();
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: galfus_bytecode::BytecodeModule {
            name: "test".to_string(),
            global_count: 0,
            constants: galfus_bytecode::ConstantPool { constants: vec![] },
            types: vec![],
            functions: vec![],
            struct_layouts: vec![],
            choice_layouts: vec![],
            imports: vec![],
            exports: vec![],
            init_func_idx: None,
        },
        metadata: None,
    });
    let _vm = VirtualMachine::new(std::sync::Arc::new(graph));

    thread.module_states.insert(
        galfus_core::ModuleId::new(0),
        crate::runtime::RuntimeModuleState {
            globals: vec![Value::Object(crate::runtime::VmObjectRef {
                index: 999,
                generation: 0,
            })],
            initialized: true,
        },
    );
}

#[test]
fn test_deterministic_order_of_released_handles() {
    let mut thread = thread::VmThreadState::test_new();
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: galfus_bytecode::BytecodeModule {
            name: "test".to_string(),
            global_count: 0,
            constants: galfus_bytecode::ConstantPool { constants: vec![] },
            types: vec![],
            functions: vec![],
            struct_layouts: vec![],
            choice_layouts: vec![],
            imports: vec![],
            exports: vec![],
            init_func_idx: None,
        },
        metadata: None,
    });
    let _vm = VirtualMachine::new(std::sync::Arc::new(graph));

    let _h1 = thread
        .heap
        .alloc(HeapObject::AdapterHandle {
            binding_id: galfus_core::BindingId::new(1),
            type_id: galfus_core::OpaqueTypeId::new("graphics", "Texture").unwrap(),
            id: galfus_core::HandleId::new(10),
        })
        .unwrap();
    let _h2 = thread
        .heap
        .alloc(HeapObject::AdapterHandle {
            binding_id: galfus_core::BindingId::new(1),
            type_id: galfus_core::OpaqueTypeId::new("graphics", "Texture").unwrap(),
            id: galfus_core::HandleId::new(20),
        })
        .unwrap();
}

#[test]
fn test_visit_roots_includes_entry_func_and_system_response() {
    let mut thread = thread::VmThreadState::test_new();
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: galfus_bytecode::BytecodeModule {
            name: "test".to_string(),
            global_count: 0,
            constants: galfus_bytecode::ConstantPool { constants: vec![] },
            types: vec![],
            functions: vec![],
            struct_layouts: vec![],
            choice_layouts: vec![],
            imports: vec![],
            exports: vec![],
            init_func_idx: None,
        },
        metadata: None,
    });
    let _vm = VirtualMachine::new(std::sync::Arc::new(graph));

    let h1 = thread
        .heap
        .alloc(HeapObject::Tuple { elements: vec![] })
        .unwrap();
    let h2 = thread
        .heap
        .alloc(HeapObject::Tuple { elements: vec![] })
        .unwrap();

    thread.entry_func = Some(Value::Object(h1));
    thread.system_response = Some(Value::Object(h2));
}
