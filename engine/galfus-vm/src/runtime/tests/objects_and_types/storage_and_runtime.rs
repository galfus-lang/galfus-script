use std::sync;

use crate::thread;

#[test]
fn test_structs_load_store() {
    let instrs = vec![
        Instruction::AllocLocal {
            dest: Reg(1),
            type_idx: TypeIdx(3), // Struct Point
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(0), // 42
        },
        Instruction::StoreField {
            obj: Reg(1),
            field: FieldIdx(0), // field x
            val: Reg(2),
        },
        Instruction::LoadField {
            dest: Reg(3),
            obj: Reg(1),
            field: FieldIdx(0),
        },
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(42)]);
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
    assert_eq!(res, Value::Int64(42));
}

#[test]
fn test_arrays_load_store() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 5
        },
        Instruction::NewArray {
            dest: Reg(2),
            type_idx: TypeIdx(4), // Array of Int64
            len_reg: Reg(1),
        },
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(1), // index 2
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(2), // value 99
        },
        Instruction::StoreIndex {
            arr: Reg(2),
            idx: Reg(3),
            val: Reg(4),
        },
        Instruction::LoadIndex {
            dest: Reg(5),
            arr: Reg(2),
            idx: Reg(3),
        },
        Instruction::Ret { src: Reg(5) },
    ];
    let image = create_test_module(
        instrs,
        vec![Constant::Int64(5), Constant::Int64(2), Constant::Int64(99)],
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
fn test_tuples() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 10
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1), // true
        },
        Instruction::NewTuple {
            dest: Reg(3),
            type_idx: TypeIdx(5),
            start: Reg(1),
            count: 2,
        },
        Instruction::LoadConst {
            dest: Reg(4),
            const_idx: ConstIdx(2), // index 1
        },
        Instruction::LoadIndex {
            dest: Reg(5),
            arr: Reg(3),
            idx: Reg(4),
        },
        Instruction::Ret { src: Reg(5) },
    ];
    let image = create_test_module(
        instrs,
        vec![
            Constant::Int64(10),
            Constant::Bool(true),
            Constant::Int64(1),
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
    assert_eq!(res, Value::Bool(true));
}

#[test]
fn test_choices() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 100
        },
        Instruction::NewChoice {
            dest: Reg(2),
            type_idx: TypeIdx(6),
            variant_idx: 1, // Some
            payload: Reg(1),
        },
        Instruction::Ret { src: Reg(2) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(100)]);
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
    if let Value::Object(obj_ref) = res {
        let heap_obj = thread.heap.get_object(obj_ref).unwrap();
        if let HeapObject::Choice {
            variant_idx,
            payload,
            ..
        } = heap_obj
        {
            assert_eq!(*variant_idx, 1);
            assert_eq!(*payload, Value::Int64(100));
        } else {
            panic!("Expected Choice");
        }
    } else {
        panic!("Expected ObjectRef");
    }
}

#[test]
fn test_cast() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 42 (Int64)
        },
        Instruction::Cast {
            dest: Reg(2),
            src: Reg(1),
            type_idx: TypeIdx(7), // Uint8
        }, // cast 42 (Int64) to 42 (Uint8)
        Instruction::Ret { src: Reg(2) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(42)]);
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
    assert_eq!(res, Value::Uint8(42));
}

#[test]
fn test_instanceof() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 42 (Int64)
        },
        Instruction::Instanceof {
            dest: Reg(2),
            src: Reg(1),
            type_idx: TypeIdx(0), // Int64
        }, // true
        Instruction::Ret { src: Reg(2) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(42)]);
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
    assert_eq!(res, Value::Bool(true));
}

#[test]
fn test_instanceof_constraint_satisfied_by_struct_layout() {
    let instrs = vec![
        Instruction::AllocLocal {
            dest: Reg(1),
            type_idx: TypeIdx(3), // Struct(Point)
        },
        Instruction::Instanceof {
            dest: Reg(2),
            src: Reg(1),
            type_idx: TypeIdx(8), // Constraint(Stringable)
        },
        Instruction::Ret { src: Reg(2) },
    ];
    let mut image = create_test_module(instrs, vec![]);
    image
        .types
        .push(BytecodeType::Constraint("Stringable".to_string()));
    image.struct_layouts[0]
        .constraints
        .push("Stringable".to_string());

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
    assert_eq!(res, Value::Bool(true));
}

#[test]
fn test_division_by_zero_panic() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 10
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1), // 0
        },
        Instruction::Div {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        },
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(10), Constant::Int64(0)]);
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VmThreadState::new();
    let res = vm.run_function(
        &mut thread,
        galfus_core::ModuleId::new(0),
        FuncIdx(0),
        vec![],
    );
    assert!(res.is_err());
    let panic_err = res.unwrap_err();
    assert_eq!(panic_err.error, VmError::DivisionByZero);
}

#[test]
fn test_unwinding_call_stack() {
    let instrs_main = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 5
        },
        Instruction::Call {
            dest: Reg(2),
            func: FuncIdx(1),
            args_start: Reg(1),
            arg_count: 1,
        },
        Instruction::Ret { src: Reg(2) },
    ];
    let instrs_helper = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(1), // 0
        },
        Instruction::Div {
            dest: Reg(2),
            lhs: Reg(0), // param 0 (value 5)
            rhs: Reg(1), // 0
        },
        Instruction::Ret { src: Reg(2) },
    ];

    let image = BytecodeModule {
        name: "test".to_string(),
        global_count: 0,
        constants: ConstantPool {
            constants: vec![Constant::Int64(5), Constant::Int64(0)],
        },
        functions: vec![
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 0,
                local_count: 4,
                temp_count: 4,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: instrs_main,
            },
            BytecodeFunction {
                name: "helper".to_string(),
                param_count: 1,
                local_count: 4,
                temp_count: 4,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: instrs_helper,
            },
        ],
        types: vec![BytecodeType::Int64],
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
    let res = vm.run_function(
        &mut thread,
        galfus_core::ModuleId::new(0),
        FuncIdx(0),
        vec![],
    );
    assert!(res.is_err());
    let panic_err = res.unwrap_err();
    assert_eq!(panic_err.error, VmError::DivisionByZero);
    assert_eq!(panic_err.stack_trace.len(), 2);
}
