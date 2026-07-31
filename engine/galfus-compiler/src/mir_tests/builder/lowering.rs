#[test]
fn test_mir_builder_basic() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn add(a: i32, b: i32): i32 {
            return a + b
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();

    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    // Typecheck
    let type_result = check_declaration_types(&source, &graph);
    assert!(!type_result.has_errors(), "Typecheck errors occurred");

    // Build MIR
    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();

    assert_eq!(mir_module.functions.len(), 1);
    let func = &mir_module.functions[0];
    assert_eq!(func.name, "add");
    assert_eq!(func.parameter_types.len(), 2);

    assert!(!func.blocks.is_empty());
    let bb = func
        .blocks
        .iter()
        .find(|b| matches!(b.terminator.0, Terminator::Return(_)))
        .unwrap();
    assert!(!bb.instructions.is_empty());

    // Find the Add instruction in some block
    let assign_inst = func
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|(inst, _)| {
            matches!(
                inst,
                Instruction::Assign(_, RValue::BinaryOp(MirBinaryOp::Add, _, _))
            )
        })
        .unwrap();

    match &assign_inst.0 {
        Instruction::Assign(
            _dest,
            RValue::BinaryOp(MirBinaryOp::Add, Operand::Local(_lhs), Operand::Local(_rhs)),
        ) => {}
        other => panic!("Unexpected instruction: {:?}", other),
    }
    match &bb.terminator.0 {
        Terminator::Return(Some(Operand::Local(_ret_local))) => {}
        other => panic!("Unexpected terminator: {:?}", other),
    }
}

#[test]
fn test_mir_builder_lowers_named_function_as_a_function_constant() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn worker(args: [[u8]]): i32 {
            return 0
        }

        fn main(args: [[u8]]): i32 {
            const handler = worker
            return handler(args)
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let worker = mir_module
        .functions
        .iter()
        .find(|function| function.name == "worker")
        .expect("worker function should be lowered");

    assert!(
        mir_module
            .functions
            .iter()
            .flat_map(|function| { function.blocks.iter().flat_map(|block| &block.instructions) })
            .any(|(instruction, _)| {
                matches!(
                    instruction,
                    Instruction::Assign(_, RValue::Use(Operand::Constant(Constant::Function(id))))
                        if *id == worker.id
                )
            })
    );
}

#[test]
fn test_mir_builder_lowers_named_expression_function_body() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn double(value: i32): i32 => value * 2
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    let type_result = check_definition_types(
        &source,
        &graph,
        check_declaration_types(&source, &graph),
    );
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let function = mir_module
        .functions
        .iter()
        .find(|function| function.name == "double")
        .expect("expression function should be lowered");

    assert!(function.blocks.iter().any(|block| {
        matches!(
            block.terminator.0,
            Terminator::Return(Some(Operand::Local(_)))
        )
    }));
    assert!(function.blocks.iter().any(|block| {
        block.instructions.iter().any(|(instruction, _)| {
            matches!(
                instruction,
                Instruction::Assign(_, RValue::BinaryOp(MirBinaryOp::Multiply, _, _))
            )
        })
    }));
}

#[test]
fn test_mir_builder_lowers_expression_and_block_function_expressions() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn main(): i32 {
            var double = fn (value: i32): i32 => value * 2
            var one = fn (): i32 {
                return 1
            }
            return double(one())
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    let type_result = check_definition_types(
        &source,
        &graph,
        check_declaration_types(&source, &graph),
    );
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let mir_module = MirBuilder::new(&graph, &type_result, code).build();
    let functions = mir_module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("__anon_func_"))
        .collect::<Vec<_>>();

    assert_eq!(functions.len(), 2);
    assert!(functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|(instruction, _)| {
                matches!(
                    instruction,
                    Instruction::Assign(_, RValue::BinaryOp(MirBinaryOp::Multiply, _, _))
                )
            })
        })
    }));
    assert!(functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator.0,
                Terminator::Return(Some(Operand::Constant(Constant::Int32(1))))
            )
        })
    }));
}

#[test]
fn test_mir_builder_lowers_copy_expression() {
    let source_id = SourceId::new(0);
    let code = r#"
        struct User {
            id: i32,
        }

        fn clone_user(user: User): User {
            return copy user
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();

    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();
    let func = mir_module
        .functions
        .iter()
        .find(|function| function.name == "clone_user")
        .expect("clone_user function should be lowered");

    assert!(func.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|(instruction, _)| matches!(instruction, Instruction::Assign(_, RValue::Copy(_))))
    }));
}

#[test]
fn test_mir_builder_applies_default_parameter_when_argument_is_null() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn read(terminator: [u8] = "\n"): [u8] {
            return terminator
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    let type_result = check_declaration_types(&source, &graph);

    assert!(!type_result.has_errors(), "Typecheck errors occurred");

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();
    let function = mir_module
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("read function should be lowered");

    assert!(function.blocks.iter().any(|block| {
        block.instructions.iter().any(|(instruction, _)| {
            matches!(
                instruction,
                Instruction::Assign(
                    _,
                    RValue::BinaryOp(
                        MirBinaryOp::NullFallback,
                        Operand::Local(_),
                        Operand::Constant(Constant::String(value)),
                    ),
                ) if value == "\n"
            )
        })
    }));
}

#[test]
fn test_mir_builder_lowers_concrete_typeof_branch() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn label(): [u8] {
            var dummy: i32 = 0
            return instanceof dummy {
                i32 v => "number",
                _ => "other",
            }
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();

    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    let type_result = check_declaration_types(&source, &graph);
    assert!(!type_result.has_errors(), "Typecheck errors occurred");

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();

    let func = &mir_module.functions[0];

    assert!(has_string_assignment(func, "number"));
}

#[test]
fn test_mir_builder_specializes_generic_typeof_call() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn label<T: i32 | bool>(dummy: T): [u8] {
            return instanceof dummy {
                i32 v => "number",
                bool v => "flag",
            }
        }

        fn main(): [u8] {
            return label<i32>(0)
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();

    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();

    let main = mir_module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function should be lowered");
    let call_id = first_call_function_id(main).expect("main should call label");

    let specialized = mir_module
        .functions
        .iter()
        .find(|function| function.id == call_id)
        .expect("specialized function should be emitted");

    assert!(specialized.name.starts_with("label#"));

    assert!(has_string_assignment(specialized, "number"));
}

#[test]
fn test_mir_builder_specializes_typeof_generic_parameter() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn label<T: i32 | bool>(): [u8] {
            return typeof T {
                i32 => "number",
                bool => "flag",
            }
        }

        fn main(): [u8] {
            return label<i32>()
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();
    let main = mir_module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function should be lowered");
    let call_id = first_call_function_id(main).expect("main should call label");
    let specialized = mir_module
        .functions
        .iter()
        .find(|function| function.id == call_id)
        .expect("specialized function should be emitted");

    assert!(has_string_assignment(specialized, "number"));
    assert!(!has_string_assignment(specialized, "flag"));
}

fn first_call_function_id(func: &MirFunction) -> Option<FunctionId> {
    for block in &func.blocks {
        for (inst, _) in &block.instructions {
            if let Instruction::Call { func, .. } = inst {
                return Some(*func);
            }
        }
    }
    None
}

fn has_string_assignment(func: &MirFunction, expected_value: &str) -> bool {
    func.blocks.iter().any(|block| {
        let assigned = block.instructions.iter().any(|(inst, _)| {
            if let Instruction::Assign(_, RValue::Use(Operand::Constant(Constant::String(val)))) =
                inst
            {
                val == expected_value
            } else {
                false
            }
        });
        let returned = matches!(
            &block.terminator.0,
            Terminator::Return(Some(Operand::Constant(Constant::String(value))))
                if value == expected_value
        );
        assigned || returned
    })
}

#[test]
fn test_mir_builder_phase1() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn complex_expr(x: i32): i32 {
            var a = 42
            const b = 3.14
            var s = "hello"
            var bl = true
            var n = null
            a = x + 10
            var c = <i32>b
            return a
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(!graph.has_errors(), "Parse or resolve errors occurred");

    let type_result = check_declaration_types(&source, &graph);
    assert!(!type_result.has_errors(), "Typecheck errors occurred");

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();

    assert_eq!(mir_module.functions.len(), 1);
    let func = &mir_module.functions[0];
    assert_eq!(func.name, "complex_expr");
    assert_eq!(func.parameter_types.len(), 1);

    // We expect several locals:
    // 0: parameter `x`
    // 1: `a`
    // 2: `b`
    // 3: `s`
    // 4: `bl`
    // 5: `n`
    // 6: temporary for `x + 10`
    // 7: `c`
    // Let's verify instructions in the body
    let return_block = func
        .blocks
        .iter()
        .find(|b| matches!(b.terminator.0, Terminator::Return(_)))
        .unwrap();
    match &return_block.terminator.0 {
        Terminator::Return(Some(Operand::Local(_local_id))) => {
            // It returns `a` (which is local 1)
        }
        other => panic!("Unexpected terminator: {:?}", other),
    }
}

#[test]
fn test_mir_builder_phase2() {
    let source_id = SourceId::new(0);
    let code = r#"
        fn other_func(x: i32): i32 {
            return x * 2
        }

        fn control_flow(cond: bool, val: i32): i32 {
            var res = 0;
            if cond {
                res = other_func(val);
            } else {
                res = 100;
            }

            var i = 0;
            loop {
                if i >= 10 {
                    break;
                }
                i = i + 1;
                if i == 5 {
                    continue;
                }
                if i == 8 {
                    break;
                }
            }

            loop {
                break;
            }

            return res
        }
    "#;
    let source = SourceFile::new(source_id, "test.gfs".to_string(), code.to_string());

    let parse_result = parse(&source);
    let resolve_result = resolve(&source, parse_result.into_graph());
    let graph = resolve_result.into_graph();
    assert!(
        !graph.has_errors(),
        "Parse or resolve errors occurred: {:?}",
        graph.diagnostics()
    );

    let type_result = check_declaration_types(&source, &graph);
    assert!(
        !type_result.has_errors(),
        "Typecheck errors occurred: {:?}",
        type_result.diagnostics()
    );

    let builder = MirBuilder::new(&graph, &type_result, code);
    let mir_module = builder.build();

    assert_eq!(mir_module.functions.len(), 2);
    let func = &mir_module.functions[1];
    assert_eq!(func.name, "control_flow");

    assert!(
        func.blocks.len() > 1,
        "Control flow should generate multiple blocks"
    );
    let has_return = func
        .blocks
        .iter()
        .any(|b| matches!(b.terminator.0, Terminator::Return(_)));
    assert!(has_return, "Return statement not found in MIR");
}
