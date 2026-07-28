#[test]
fn test_copy_deep_copies_nested_structs() {
    let instrs = vec![
        Instruction::AllocLocal {
            dest: Reg(1),
            type_idx: TypeIdx(3),
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(0),
        },
        Instruction::StoreField {
            obj: Reg(1),
            field: FieldIdx(0),
            val: Reg(2),
        },
        Instruction::AllocLocal {
            dest: Reg(3),
            type_idx: TypeIdx(8),
        },
        Instruction::StoreField {
            obj: Reg(3),
            field: FieldIdx(0),
            val: Reg(1),
        },
        Instruction::Copy {
            dest: Reg(4),
            src: Reg(3),
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1),
        },
        Instruction::StoreField {
            obj: Reg(1),
            field: FieldIdx(0),
            val: Reg(2),
        },
        Instruction::Ret { src: Reg(4) },
    ];

    let mut image = create_test_module(instrs, vec![Constant::Int64(7), Constant::Int64(9)]);
    image.types.push(BytecodeType::Struct(StructLayoutIdx(1)));
    image.struct_layouts.push(StructLayout {
        name: "Box".to_string(),
        fields: vec![FieldLayout {
            name: "point".to_string(),
            ty: TypeIdx(3),
            offset: 0,
            ownership: OwnershipKind::Strong,
        }],
        constraints: vec![],
    });

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VirtualThread::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    let copied_box_ref = match res {
        Value::Object(obj_ref) => obj_ref,
        other => panic!("expected copied object, got {:?}", other),
    };

    let copied_point_ref = match thread.heap.get_object(copied_box_ref).unwrap() {
        HeapObject::Struct { fields, .. } => match fields[0] {
            Value::Object(obj_ref) => obj_ref,
            ref other => panic!("expected copied point, got {:?}", other),
        },
        other => panic!("expected copied box, got {:?}", other),
    };

    match thread.heap.get_object(copied_point_ref).unwrap() {
        HeapObject::Struct { fields, .. } => {
            assert_eq!(fields[0], Value::Int64(7));
        }
        other => panic!("expected copied point, got {:?}", other),
    }
}

#[test]
fn test_copy_preserves_internal_weak_observer_topology() {
    let instrs = vec![
        Instruction::AllocLocal {
            dest: Reg(1),
            type_idx: TypeIdx(8),
        },
        Instruction::AllocLocal {
            dest: Reg(2),
            type_idx: TypeIdx(8),
        },
        Instruction::StoreField {
            obj: Reg(1),
            field: FieldIdx(0),
            val: Reg(2),
        },
        Instruction::StoreField {
            obj: Reg(2),
            field: FieldIdx(1),
            val: Reg(1),
        },
        Instruction::Copy {
            dest: Reg(3),
            src: Reg(1),
        },
        Instruction::Ret { src: Reg(3) },
    ];

    let mut image = create_test_module(instrs, vec![]);
    image.types.push(BytecodeType::Struct(StructLayoutIdx(1)));
    image.struct_layouts.push(StructLayout {
        name: "Node".to_string(),
        fields: vec![
            FieldLayout {
                name: "child".to_string(),
                ty: TypeIdx(8),
                offset: 0,
                ownership: OwnershipKind::Strong,
            },
            FieldLayout {
                name: "parent".to_string(),
                ty: TypeIdx(8),
                offset: 8,
                ownership: OwnershipKind::Weak,
            },
        ],
        constraints: vec![],
    });

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VirtualThread::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    let copied_parent_ref = match res {
        Value::Object(obj_ref) => obj_ref,
        other => panic!("expected copied parent, got {:?}", other),
    };

    let copied_child_ref = match thread.heap.get_object(copied_parent_ref).unwrap() {
        HeapObject::Struct { fields, .. } => match fields[0] {
            Value::Object(obj_ref) => obj_ref,
            ref other => panic!("expected copied child, got {:?}", other),
        },
        other => panic!("expected copied parent, got {:?}", other),
    };

    match thread.heap.get_object(copied_child_ref).unwrap() {
        HeapObject::Struct { fields, .. } => {
            assert_eq!(fields[1], Value::Object(copied_parent_ref));
        }
        other => panic!("expected copied child, got {:?}", other),
    }
}

#[test]
fn test_copy_nulls_external_weak_observer_target() {
    let instrs = vec![
        Instruction::AllocLocal {
            dest: Reg(1),
            type_idx: TypeIdx(8),
        },
        Instruction::AllocLocal {
            dest: Reg(2),
            type_idx: TypeIdx(8),
        },
        Instruction::AllocLocal {
            dest: Reg(3),
            type_idx: TypeIdx(8),
        },
        Instruction::StoreField {
            obj: Reg(2),
            field: FieldIdx(0),
            val: Reg(3),
        },
        Instruction::StoreField {
            obj: Reg(2),
            field: FieldIdx(1),
            val: Reg(1),
        },
        Instruction::Copy {
            dest: Reg(4),
            src: Reg(2),
        },
        Instruction::Ret { src: Reg(4) },
    ];

    let mut image = create_test_module(instrs, vec![]);
    image.types.push(BytecodeType::Struct(StructLayoutIdx(1)));
    image.struct_layouts.push(StructLayout {
        name: "Node".to_string(),
        fields: vec![
            FieldLayout {
                name: "child".to_string(),
                ty: TypeIdx(8),
                offset: 0,
                ownership: OwnershipKind::Strong,
            },
            FieldLayout {
                name: "parent".to_string(),
                ty: TypeIdx(8),
                offset: 8,
                ownership: OwnershipKind::Weak,
            },
        ],
        constraints: vec![],
    });

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VirtualThread::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    let copied_ref = match res {
        Value::Object(obj_ref) => obj_ref,
        other => panic!("expected copied node, got {:?}", other),
    };

    match thread.heap.get_object(copied_ref).unwrap() {
        HeapObject::Struct { fields, .. } => {
            assert_eq!(fields[1], Value::Null);
        }
        other => panic!("expected copied node, got {:?}", other),
    }
}

#[test]
fn test_copy_preserves_strong_ownership_topology() {
    let instrs = vec![
        Instruction::AllocLocal {
            dest: Reg(1),
            type_idx: TypeIdx(8),
        },
        Instruction::AllocLocal {
            dest: Reg(2),
            type_idx: TypeIdx(9),
        },
        Instruction::StoreField {
            obj: Reg(2),
            field: FieldIdx(0),
            val: Reg(1),
        },
        Instruction::StoreField {
            obj: Reg(2),
            field: FieldIdx(1),
            val: Reg(1),
        },
        Instruction::Copy {
            dest: Reg(3),
            src: Reg(2),
        },
        Instruction::Ret { src: Reg(3) },
    ];

    let mut image = create_test_module(instrs, vec![]);
    image.types.push(BytecodeType::Struct(StructLayoutIdx(1)));
    image.types.push(BytecodeType::Struct(StructLayoutIdx(2)));
    image.struct_layouts.push(StructLayout {
        name: "Node".to_string(),
        fields: vec![FieldLayout {
            name: "value".to_string(),
            ty: TypeIdx(0),
            offset: 0,
            ownership: OwnershipKind::Value,
        }],
        constraints: vec![],
    });
    image.struct_layouts.push(StructLayout {
        name: "Pair".to_string(),
        fields: vec![
            FieldLayout {
                name: "left".to_string(),
                ty: TypeIdx(8),
                offset: 0,
                ownership: OwnershipKind::Strong,
            },
            FieldLayout {
                name: "right".to_string(),
                ty: TypeIdx(8),
                offset: 8,
                ownership: OwnershipKind::Strong,
            },
        ],
        constraints: vec![],
    });

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VirtualThread::new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    let copied_pair_ref = match res {
        Value::Object(obj_ref) => obj_ref,
        other => panic!("expected copied pair, got {:?}", other),
    };

    match thread.heap.get_object(copied_pair_ref).unwrap() {
        HeapObject::Struct { fields, .. } => {
            assert_eq!(fields[0], fields[1]);
            assert_ne!(fields[0], Value::Object(VmObjectRef(0)));
        }
        other => panic!("expected copied pair, got {:?}", other),
    }
}

#[test]
fn test_copy_rejects_fieldless_structs_at_runtime() {
    let instrs = vec![
        Instruction::AllocLocal {
            dest: Reg(1),
            type_idx: TypeIdx(8),
        },
        Instruction::Copy {
            dest: Reg(2),
            src: Reg(1),
        },
        Instruction::Ret { src: Reg(2) },
    ];

    let mut image = create_test_module(instrs, vec![]);
    image.types.push(BytecodeType::Struct(StructLayoutIdx(1)));
    image.struct_layouts.push(StructLayout {
        name: "Token".to_string(),
        fields: vec![],
        constraints: vec![],
    });

    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph.clone()));
    let mut thread = thread::VirtualThread::new();
    let err = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap_err();

    assert!(matches!(err.error, VmError::TypeMismatch { .. }));
}
