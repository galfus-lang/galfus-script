use std::sync;

use crate::thread;

use super::*;

use galfus_bytecode::BytecodeModule;

#[test]
fn test_len_and_copy_array() {
    let instrs = vec![
        // Create source array [1, 2, 3] of type idx 1 (Array of Int64)
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 3
        },
        Instruction::NewArray {
            dest: Reg(2), // src_arr
            type_idx: TypeIdx(1),
            len_reg: Reg(1),
        },
        // Populate elements
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(1), // 10
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(2), // 0 (idx)
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(3), // 20
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(4), // 1 (idx)
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(5), // 30
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(6), // 2 (idx)
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        // Query length of src_arr
        Instruction::Len {
            dest: Reg(5),
            src: Reg(2),
        },
        // Allocate dest_arr of size 5
        Instruction::LoadConst {
            dest: Reg(6),
            const_idx: ConstIdx(7), // 5
        },
        Instruction::NewArray {
            dest: Reg(7), // dest_arr
            type_idx: TypeIdx(1),
            len_reg: Reg(6),
        },
        // CopyArray from src_arr to dest_arr starting at index 1
        Instruction::LoadConst {
            dest: Reg(8),
            const_idx: ConstIdx(4), // 1 (dest_start)
        },
        Instruction::CopyArray {
            dest: Reg(7),
            dest_start: Reg(8),
            src: Reg(2),
        },
        Instruction::Ret { src: Reg(7) },
    ];

    let image = BytecodeModule {
        name: "test".to_string(),
        constants: ConstantPool {
            constants: vec![
                Constant::Int64(3),  // 0
                Constant::Int64(10), // 1
                Constant::Int64(0),  // 2
                Constant::Int64(20), // 3
                Constant::Int64(1),  // 4
                Constant::Int64(30), // 5
                Constant::Int64(2),  // 6
                Constant::Int64(5),  // 7
            ],
        },
        functions: vec![BytecodeFunction {
            name: "main".to_string(),
            param_count: 0,
            local_count: 10,
            temp_count: 10,
            return_ty: TypeIdx(1),
            proxy_metadata: None,
            instructions: instrs,
        }],
        types: vec![BytecodeType::Int64, BytecodeType::Array(TypeIdx(0))],
        struct_layouts: vec![],
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
    let mut thread = thread::VmThreadState::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    let arr_ref = match res {
        Value::Object(r) => r,
        other => panic!("expected object, got {:?}", other),
    };
    let arr_obj = thread.heap.get_object(arr_ref).unwrap();
    match arr_obj {
        HeapObject::Array { elements, .. } => {
            assert_eq!(elements.len(), 5);
            assert_eq!(elements[0], Value::Int64(0));
            assert_eq!(elements[1], Value::Int64(10));
            assert_eq!(elements[2], Value::Int64(20));
            assert_eq!(elements[3], Value::Int64(30));
            assert_eq!(elements[4], Value::Int64(0));
        }
        other => panic!("expected array, got {:?}", other),
    }
}

#[test]
fn test_load_index_accepts_negative_index() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0),
        },
        Instruction::NewArray {
            dest: Reg(2),
            type_idx: TypeIdx(4),
            len_reg: Reg(1),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(1),
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(2),
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(3),
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(4),
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(5),
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(6),
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(7),
        },
        Instruction::LoadIndex {
            dest: Reg(5),
            arr: Reg(2),
            idx: Reg(4),
        },
        Instruction::Ret { src: Reg(5) },
    ];

    let image = create_test_module(
        instrs,
        vec![
            Constant::Int64(3),
            Constant::Int64(10),
            Constant::Int64(0),
            Constant::Int64(20),
            Constant::Int64(1),
            Constant::Int64(30),
            Constant::Int64(2),
            Constant::Int64(-1),
        ],
    );

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();

    assert_eq!(res, Value::Int64(30));
}

#[test]
fn test_load_index_out_of_bounds_returns_null() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0),
        },
        Instruction::NewArray {
            dest: Reg(2),
            type_idx: TypeIdx(4),
            len_reg: Reg(1),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(1),
        },
        Instruction::LoadIndex {
            dest: Reg(4),
            arr: Reg(2),
            idx: Reg(3),
        },
        Instruction::Ret { src: Reg(4) },
    ];

    let image = create_test_module(instrs, vec![Constant::Int64(3), Constant::Int64(99)]);

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();

    assert_eq!(res, Value::Null);
}

#[test]
fn test_store_index_accepts_negative_index() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0),
        },
        Instruction::NewArray {
            dest: Reg(2),
            type_idx: TypeIdx(4),
            len_reg: Reg(1),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(1),
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(2),
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(3),
        },
        Instruction::LoadIndex {
            dest: Reg(5),
            arr: Reg(2),
            idx: Reg(4),
        },
        Instruction::Ret { src: Reg(5) },
    ];

    let image = create_test_module(
        instrs,
        vec![
            Constant::Int64(3),
            Constant::Int64(99),
            Constant::Int64(-1),
            Constant::Int64(2),
        ],
    );

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();

    assert_eq!(res, Value::Int64(99));
}

#[test]
fn test_store_index_out_of_bounds_returns_error() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0),
        },
        Instruction::NewArray {
            dest: Reg(2),
            type_idx: TypeIdx(4),
            len_reg: Reg(1),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(1),
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(2),
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(4),
            val: Reg(3),
        },
        Instruction::RetNull,
    ];

    let image = create_test_module(
        instrs,
        vec![Constant::Int64(3), Constant::Int64(99), Constant::Int64(3)],
    );

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::new();
    let err = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap_err();

    assert!(matches!(
        err.error,
        VmError::IndexOutOfBounds { index: 3, len: 3 }
    ));
}

#[test]
fn test_legacy_receive_instruction_is_rejected() {
    let mut image = create_test_module(
        vec![
            Instruction::ReceiveFilter {
                dest: Reg(2),
                sender: Reg(0),
                timeout: Reg(1),
            },
            Instruction::Ret { src: Reg(2) },
        ],
        vec![],
    );
    image.functions[0].param_count = 2;
    let module_id = galfus_core::ModuleId::new(0);
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: module_id,
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::new();
    vm.prepare_function(
        &mut thread,
        module_id,
        FuncIdx(0),
        vec![VmValue::Int64(7), VmValue::Int32(10)],
    )
    .unwrap();
    let Err(error) = vm.execute_with_budget(&mut thread, 100) else {
        panic!("legacy receive is not a Future activation");
    };
    assert!(matches!(
        error.error,
        VmError::UnimplementedInstruction { .. }
    ));
}

#[test]
fn test_legacy_mailbox_instruction_is_rejected() {
    let image = create_test_module(
        vec![
            Instruction::MailboxHasMessages { dest: Reg(0) },
            Instruction::MailboxGetMessage { dest: Reg(1) },
            Instruction::MailboxHasMessages { dest: Reg(2) },
            Instruction::RetNull,
        ],
        vec![],
    );
    let module_id = galfus_core::ModuleId::new(0);
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: module_id,
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::new();
    vm.prepare_function(&mut thread, module_id, FuncIdx(0), vec![])
        .unwrap();
    let Err(error) = vm.execute_with_budget(&mut thread, 100) else {
        panic!("legacy mailbox access is not a Future activation");
    };
    assert!(matches!(
        error.error,
        VmError::UnimplementedInstruction { .. }
    ));
}
