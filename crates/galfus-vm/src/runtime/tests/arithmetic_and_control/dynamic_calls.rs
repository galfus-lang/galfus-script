#[test]
fn test_dynamic_call_returns_to_destination() {
    let image = BytecodeModule {
        name: "test".to_string(),
        constants: ConstantPool {
            constants: vec![Constant::Function(FuncIdx(1)), Constant::Int64(7)],
        },
        functions: vec![
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 0,
                local_count: 2,
                temp_count: 2,
                return_ty: TypeIdx(0),
                instructions: vec![
                    Instruction::LoadConst {
                        dest: Reg(0),
                        const_idx: ConstIdx(0),
                    },
                    Instruction::CallDynamic {
                        dest: Reg(1),
                        func_reg: Reg(0),
                        args_start: Reg(0),
                        arg_count: 0,
                    },
                    Instruction::Ret { src: Reg(1) },
                ],
            },
            BytecodeFunction {
                name: "callee".to_string(),
                param_count: 0,
                local_count: 1,
                temp_count: 0,
                return_ty: TypeIdx(0),
                instructions: vec![
                    Instruction::LoadConst {
                        dest: Reg(0),
                        const_idx: ConstIdx(1),
                    },
                    Instruction::Ret { src: Reg(0) },
                ],
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
    let mut thread = thread::VirtualThread::new();
    let result = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();

    assert_eq!(result, Value::Int64(7));
}

#[test]
fn test_dynamic_call_uses_the_function_value_module() {
    let first_module_id = galfus_core::ModuleId::new(1);
    let second_module_id = galfus_core::ModuleId::new(2);
    let first = create_test_module(
        vec![
            Instruction::LoadConst {
                dest: Reg(0),
                const_idx: ConstIdx(0),
            },
            Instruction::Ret { src: Reg(0) },
        ],
        vec![Constant::Int64(41)],
    );
    let mut second = create_test_module(
        vec![
            Instruction::CallDynamic {
                dest: Reg(1),
                func_reg: Reg(0),
                args_start: Reg(0),
                arg_count: 0,
            },
            Instruction::Ret { src: Reg(1) },
        ],
        vec![],
    );
    second.functions[0].param_count = 1;

    let graph = graph_with_nodes(
        galfus_core::SemanticRevision::new(0),
        vec![
            galfus_bytecode::BytecodeNode {
                id: first_module_id,
                path: galfus_core::ModulePath::new("first.gfs").unwrap(),
                semantic_revision: galfus_core::SemanticRevision::new(0),
                module: first,
                metadata: None,
            },
            galfus_bytecode::BytecodeNode {
                id: second_module_id,
                path: galfus_core::ModulePath::new("second.gfs").unwrap(),
                semantic_revision: galfus_core::SemanticRevision::new(0),
                module: second,
                metadata: None,
            },
        ],
    );
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = thread::VirtualThread::new();

    let result = vm
        .run_function(
            &mut thread,
            second_module_id,
            FuncIdx(0),
            vec![Value::Function {
                module_id: first_module_id,
                func_idx: FuncIdx(0),
            }],
        )
        .unwrap();

    assert_eq!(result, Value::Int64(41));
}
