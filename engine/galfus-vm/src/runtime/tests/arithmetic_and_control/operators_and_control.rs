use std::sync;

use crate::thread;

#[test]
fn test_basic_arithmetic() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0),
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1),
        },
        Instruction::Add {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        },
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(10), Constant::Int64(20)]);
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
    assert_eq!(res, Value::Int64(30));
}

#[test]
fn typed_immediates_preserve_integer_unsigned_and_float_semantics() {
    let cases = [
        (Constant::Int32(9), ImmediateValue::I32(4), ImmediateBinaryOp::Subtract, Value::Int32(5)),
        (Constant::Int64(9), ImmediateValue::I64(4), ImmediateBinaryOp::Multiply, Value::Int64(36)),
        (Constant::Uint32(9), ImmediateValue::U32(4), ImmediateBinaryOp::Greater, Value::Bool(true)),
        (Constant::Uint64(9), ImmediateValue::U64(4), ImmediateBinaryOp::Remainder, Value::Uint64(1)),
        (Constant::Float32(1.5), ImmediateValue::F32(2.0f32.to_bits()), ImmediateBinaryOp::Add, Value::Float32(3.5)),
        (Constant::Float64(1.5), ImmediateValue::F64(2.0f64.to_bits()), ImmediateBinaryOp::Multiply, Value::Float64(3.0)),
    ];

    for (constant, rhs, operation, expected) in cases {
        let image = create_test_module(
            vec![
                Instruction::LoadConst { dest: Reg(1), const_idx: ConstIdx(0) },
                Instruction::BinaryImmediate { dest: Reg(2), lhs: Reg(1), operation, rhs },
                Instruction::Ret { src: Reg(2) },
            ],
            vec![constant],
        );
        let graph = graph_with_node(galfus_bytecode::BytecodeNode {
            id: galfus_core::ModuleId::new(0), path: galfus_core::ModulePath::new("immediate.gfs").unwrap(),
            semantic_revision: galfus_core::SemanticRevision::new(0), module: image, metadata: None,
        });
        let vm = VirtualMachine::new(sync::Arc::new(graph));
        let actual = vm.run_function(&mut thread::VmThreadState::test_new(), galfus_core::ModuleId::new(0), FuncIdx(0), vec![]).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn immediate_integer_division_by_zero_panics() {
    let image = create_test_module(
        vec![
            Instruction::LoadConst { dest: Reg(1), const_idx: ConstIdx(0) },
            Instruction::BinaryImmediate { dest: Reg(2), lhs: Reg(1), operation: ImmediateBinaryOp::Divide, rhs: ImmediateValue::I32(0) },
            Instruction::Ret { src: Reg(2) },
        ], vec![Constant::Int32(1)],
    );
    let graph = graph_with_node(galfus_bytecode::BytecodeNode { id: galfus_core::ModuleId::new(0), path: galfus_core::ModulePath::new("immediate.gfs").unwrap(), semantic_revision: galfus_core::SemanticRevision::new(0), module: image, metadata: None });
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    assert!(vm.run_function(&mut thread::VmThreadState::test_new(), galfus_core::ModuleId::new(0), FuncIdx(0), vec![]).is_err());
}

#[test]
fn test_sub_mul_div_rem_pow() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 15
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1), // 4
        },
        Instruction::Sub {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        }, // 11
        Instruction::Mul {
            dest: Reg(4),
            lhs: Reg(3),
            rhs: Reg(2),
        }, // 44
        Instruction::Div {
            dest: Reg(5),
            lhs: Reg(4),
            rhs: Reg(2),
        }, // 11
        Instruction::Rem {
            dest: Reg(6),
            lhs: Reg(5),
            rhs: Reg(2),
        }, // 3
        Instruction::Pow {
            dest: Reg(7),
            lhs: Reg(6),
            rhs: Reg(2),
        }, // 3^4 = 81
        Instruction::Ret { src: Reg(7) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(15), Constant::Int64(4)]);
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
    assert_eq!(res, Value::Int64(81));
}

#[test]
fn float_division_and_power_normalize_special_results() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0),
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1),
        },
        Instruction::Div {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        },
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(instrs, vec![Constant::Float64(0.0), Constant::Float64(0.0)]);
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::test_new();
    let Value::Float64(value) = vm
        .run_function(&mut thread, galfus_core::ModuleId::new(0), FuncIdx(0), vec![])
        .expect("float division completes")
    else {
        panic!("float division must return f64");
    };
    assert_eq!(value.to_bits(), galfus_core::CANONICAL_F64_NAN);

    let value = vm
        .pow_values(Value::Float64(-1.0), Value::Float64(0.5))
        .expect("float power completes");
    let Value::Float64(value) = value else {
        panic!("float power must return f64");
    };
    assert_eq!(value.to_bits(), galfus_core::CANONICAL_F64_NAN);
}

#[test]
fn float_arithmetic_normalizes_zero_and_nan_results() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0),
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1),
        },
        Instruction::Mul {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        },
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(instrs, vec![Constant::Float64(0.0), Constant::Float64(-1.0)]);
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::test_new();
    let Value::Float64(value) = vm
        .run_function(&mut thread, galfus_core::ModuleId::new(0), FuncIdx(0), vec![])
        .expect("float multiplication completes")
    else {
        panic!("float multiplication must return f64");
    };
    assert_eq!(value.to_bits(), 0.0f64.to_bits());

    let image = create_test_module(
        vec![
            Instruction::LoadConst {
                dest: Reg(1),
                const_idx: ConstIdx(0),
            },
            Instruction::LoadConst {
                dest: Reg(2),
                const_idx: ConstIdx(1),
            },
            Instruction::Sub {
                dest: Reg(3),
                lhs: Reg(1),
                rhs: Reg(2),
            },
            Instruction::Ret { src: Reg(3) },
        ],
        vec![Constant::Float64(f64::INFINITY), Constant::Float64(f64::INFINITY)],
    );
    let graph = graph_with_node(galfus_bytecode::BytecodeNode {
        id: galfus_core::ModuleId::new(0),
        path: galfus_core::ModulePath::new("test.gfs").unwrap(),
        semantic_revision: galfus_core::SemanticRevision::new(0),
        module: image,
        metadata: None,
    });
    let vm = VirtualMachine::new(sync::Arc::new(graph));
    let mut thread = thread::VmThreadState::test_new();
    let Value::Float64(value) = vm
        .run_function(&mut thread, galfus_core::ModuleId::new(0), FuncIdx(0), vec![])
        .expect("infinite subtraction completes")
    else {
        panic!("infinite subtraction must return f64");
    };
    assert_eq!(value.to_bits(), galfus_core::CANONICAL_F64_NAN);

    let value = vm
        .pow_values(Value::Float64(-1.0), Value::Float64(0.5))
        .expect("NaN-producing float power completes");
    let Value::Float64(value) = value else {
        panic!("float power must return f64");
    };
    assert_eq!(value.to_bits(), galfus_core::CANONICAL_F64_NAN);
}

#[test]
fn test_neg() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 5
        },
        Instruction::Neg {
            dest: Reg(2),
            src: Reg(1),
        }, // -5
        Instruction::Ret { src: Reg(2) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(5)]);
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
    assert_eq!(res, Value::Int64(-5));
}

#[test]
fn test_not() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // true
        },
        Instruction::Not {
            dest: Reg(2),
            src: Reg(1),
        }, // false
        Instruction::Ret { src: Reg(2) },
    ];
    let image = create_test_module(instrs, vec![Constant::Bool(true)]);
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
    assert_eq!(res, Value::Bool(false));
}

#[test]
fn test_bitnot() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 5
        },
        Instruction::BitNot {
            dest: Reg(2),
            src: Reg(1),
        }, // !5
        Instruction::Ret { src: Reg(2) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(5)]);
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
    assert_eq!(res, Value::Int64(!5));
}

#[test]
fn test_shl_shr_and_or_xor() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 8
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1), // 2
        },
        Instruction::Shl {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        }, // 32
        Instruction::Shr {
            dest: Reg(4),
            lhs: Reg(3),
            rhs: Reg(2),
        }, // 8
        Instruction::And {
            dest: Reg(5),
            lhs: Reg(4),
            rhs: Reg(1),
        }, // 8
        Instruction::Or {
            dest: Reg(6),
            lhs: Reg(5),
            rhs: Reg(2),
        }, // 8 | 2 = 10
        Instruction::Xor {
            dest: Reg(7),
            lhs: Reg(6),
            rhs: Reg(2),
        }, // 10 ^ 2 = 8
        Instruction::Ret { src: Reg(7) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(8), Constant::Int64(2)]);
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
    assert_eq!(res, Value::Int64(8));
}

#[test]
fn test_comparison_lt() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 100
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1), // 200
        },
        Instruction::Lt {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        }, // true
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(100), Constant::Int64(200)]);
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
    assert_eq!(res, Value::Bool(true));
}

#[test]
fn test_comparison_le() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 100
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1), // 200
        },
        Instruction::Le {
            dest: Reg(3),
            lhs: Reg(2),
            rhs: Reg(1),
        }, // false
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(instrs, vec![Constant::Int64(100), Constant::Int64(200)]);
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
    assert_eq!(res, Value::Bool(false));
}

#[test]
fn test_fallback() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // 100
        },
        Instruction::LoadNull { dest: Reg(2) },
        Instruction::Fallback {
            dest: Reg(3),
            src: Reg(2),
            fallback: Reg(1),
        }, // 100
        Instruction::Ret { src: Reg(3) },
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
    let mut thread = thread::VmThreadState::test_new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    assert_eq!(res, Value::Int64(100));
}

#[test]
fn test_control_flow_jumps() {
    let instrs = vec![
        Instruction::LoadConst {
            dest: Reg(1),
            const_idx: ConstIdx(0), // false
        },
        Instruction::JumpFalse {
            cond: Reg(1),
            offset: 2,
        },
        Instruction::LoadConst {
            dest: Reg(2),
            const_idx: ConstIdx(1), // 999
        },
        Instruction::Ret { src: Reg(2) },
        // Target of jump
        Instruction::LoadConst {
            dest: Reg(3),
            const_idx: ConstIdx(2), // 888
        },
        Instruction::Ret { src: Reg(3) },
    ];
    let image = create_test_module(
        instrs,
        vec![
            Constant::Bool(false),
            Constant::Int64(999),
            Constant::Int64(888),
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
    let mut thread = thread::VmThreadState::test_new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();
    assert_eq!(res, Value::Int64(888));
}

#[test]
fn test_nested_calls_return_to_explicit_destinations() {
    let main_instrs = vec![
        Instruction::Call {
            dest: Reg(1),
            func: FuncIdx(1),
            args_start: Reg(0),
            arg_count: 0,
        },
        Instruction::Call {
            dest: Reg(2),
            func: FuncIdx(2),
            args_start: Reg(0),
            arg_count: 0,
        },
        Instruction::Add {
            dest: Reg(3),
            lhs: Reg(1),
            rhs: Reg(2),
        },
        Instruction::Ret { src: Reg(3) },
    ];

    let one_instrs = vec![
        Instruction::LoadConst {
            dest: Reg(0),
            const_idx: ConstIdx(0),
        },
        Instruction::Ret { src: Reg(0) },
    ];

    let two_instrs = vec![
        Instruction::LoadConst {
            dest: Reg(0),
            const_idx: ConstIdx(1),
        },
        Instruction::Ret { src: Reg(0) },
    ];

    let image = BytecodeModule {
        name: "test".to_string(),
        global_count: 0,
        constants: ConstantPool {
            constants: vec![Constant::Int64(1), Constant::Int64(2)],
        },
        functions: vec![
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 0,
                local_count: 4,
                temp_count: 4,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: main_instrs,
            },
            BytecodeFunction {
                name: "one".to_string(),
                param_count: 0,
                local_count: 1,
                temp_count: 1,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: one_instrs,
            },
            BytecodeFunction {
                name: "two".to_string(),
                param_count: 0,
                local_count: 1,
                temp_count: 1,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: two_instrs,
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
    let mut thread = thread::VmThreadState::test_new();
    let res = vm
        .run_function(
            &mut thread,
            galfus_core::ModuleId::new(0),
            FuncIdx(0),
            vec![],
        )
        .unwrap();

    assert_eq!(res, Value::Int64(3));
}
