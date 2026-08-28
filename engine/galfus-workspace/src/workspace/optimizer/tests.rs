use super::*;
use galfus_bytecode::{
    BytecodeNode, BytecodeType, ConstIdx, ConstantPool, DebugLocation, ExecutionMetadata, FuncIdx,
    Reg, TypeIdx,
};
use galfus_contract::{ExecutionTarget, LimitsMetadata};
use galfus_core::{ModuleId, ModulePath, SemanticRevision, SourceId, Span};
use std::sync::Arc;

fn module_for_transform_contract(function: BytecodeFunction) -> BytecodeModule {
    BytecodeModule {
        name: "transform-contract.gfs".to_string(),
        global_count: 0,
        constants: ConstantPool::default(),
        functions: vec![function],
        types: vec![BytecodeType::Int64],
        struct_layouts: Vec::new(),
        choice_layouts: Vec::new(),
        imports: Vec::new(),
        exports: vec![galfus_bytecode::ExportSlot {
            symbol_name: "main".to_string(),
            kind: ExportKind::Function(FuncIdx(0)),
        }],
        init_func_idx: None,
    }
}

fn optimize_with_transform_contract(
    function: BytecodeFunction,
) -> (BytecodeFunction, Vec<Option<usize>>) {
    let mut module = module_for_transform_contract(function);
    galfus_bytecode::validate_bytecode_module(&module)
        .expect("optimizer input must satisfy bytecode validation");
    let mut metadata = ExecutionMetadata::default();
    metadata.set_function_spans(
        FuncIdx(0),
        (0..module.functions[0].instructions.len())
            .map(|offset| (offset, Span::new(SourceId::new(0), offset, offset + 1)))
            .collect(),
    );

    let remap = optimize_function(&mut module.functions[0]);
    metadata.remap_instruction_offsets(FuncIdx(0), &remap);

    galfus_bytecode::validate_bytecode_module(&module)
        .expect("optimizer output must satisfy bytecode validation");
    assert!(
        remap
            .iter()
            .flatten()
            .all(|offset| { *offset < module.functions[0].instructions.len() })
    );
    for (old_offset, new_offset) in remap.iter().enumerate() {
        let Some(new_offset) = new_offset else {
            continue;
        };
        let location = metadata
            .location_for(FuncIdx(0), *new_offset)
            .expect("retained instruction must retain a source location");
        assert!(
            remap.iter().enumerate().any(|(candidate, mapped)| {
                *mapped == Some(*new_offset)
                    && location == DebugLocation::new(candidate, candidate + 1)
            }),
            "offset {old_offset} remapped to {new_offset} without a retained source location"
        );
    }
    (module.functions.remove(0), remap)
}

#[test]
fn canonicalization_threads_jump_chains_and_removes_their_dead_links() {
    let mut function = BytecodeFunction {
        name: "jump-chain".to_string(),
        param_count: 0,
        local_count: 0,
        temp_count: 0,
        return_ty: TypeIdx(0),
        adapter_proxy_metadata: None,
        instructions: vec![
            Instruction::Jump { offset: 1 },
            Instruction::RetNull,
            Instruction::Jump { offset: 1 },
            Instruction::RetNull,
            Instruction::RetNull,
        ],
    };

    let remap = optimize_function(&mut function);

    assert_eq!(function.instructions, vec![Instruction::RetNull]);
    assert_eq!(remap[0], Some(0));
    assert_eq!(remap[2], Some(0));
    assert_eq!(remap[4], Some(0));
}

#[test]
fn canonicalization_threads_conditional_targets_without_changing_fallthrough() {
    let (function, remap) = optimize_with_transform_contract(BytecodeFunction {
        name: "conditional-jump-chain".to_string(),
        param_count: 1,
        local_count: 0,
        temp_count: 0,
        return_ty: TypeIdx(0),
        adapter_proxy_metadata: None,
        instructions: vec![
            Instruction::JumpTrue {
                cond: Reg(0),
                offset: 1,
            },
            Instruction::RetNull,
            Instruction::Jump { offset: 1 },
            Instruction::RetNull,
            Instruction::RetNull,
        ],
    });

    assert_eq!(
        function.instructions,
        vec![
            Instruction::JumpTrue {
                cond: Reg(0),
                offset: 1,
            },
            Instruction::RetNull,
            Instruction::RetNull,
        ]
    );
    assert_eq!(remap, vec![Some(0), Some(1), Some(2), None, Some(2)]);
}

#[test]
fn transform_contract_rejects_invalid_input_before_optimization() {
    let module = module_for_transform_contract(BytecodeFunction {
        name: "invalid".to_string(),
        param_count: 0,
        local_count: 1,
        temp_count: 0,
        return_ty: TypeIdx(0),
        adapter_proxy_metadata: None,
        instructions: vec![Instruction::LoadNull { dest: Reg(1) }, Instruction::RetNull],
    });

    assert!(galfus_bytecode::validate_bytecode_module(&module).is_err());
}

#[test]
fn transform_contract_preserves_loop_and_branch_validity() {
    let (function, remap) = optimize_with_transform_contract(BytecodeFunction {
        name: "loop".to_string(),
        param_count: 1,
        local_count: 1,
        temp_count: 0,
        return_ty: TypeIdx(0),
        adapter_proxy_metadata: None,
        instructions: vec![
            Instruction::JumpFalse {
                cond: Reg(0),
                offset: 2,
            },
            Instruction::Move {
                dest: Reg(1),
                src: Reg(0),
            },
            Instruction::Jump { offset: -3 },
            Instruction::RetNull,
        ],
    });

    assert_eq!(function.instructions.len(), 4);
    assert_eq!(remap, vec![Some(0), Some(1), Some(2), Some(3)]);
}

#[test]
fn finalizing_an_already_canonical_package_reuses_its_allocation() {
    let package = Arc::new(
        PackageImage::try_new(
            galfus_bytecode::BytecodeGraph::new(),
            ExecutionTarget::new("test").expect("valid target"),
            None,
            galfus_bytecode::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid package"),
    );

    let finalized = optimize_package(Arc::clone(&package), None, SemanticRevision::new(1))
        .expect("canonical package finalizes");

    assert!(Arc::ptr_eq(&package, &finalized));
}

#[test]
fn finalization_prunes_targets_reachable_only_from_removed_calls() {
    let module = BytecodeModule {
        name: "second-prune.gfs".to_string(),
        global_count: 0,
        constants: ConstantPool::default(),
        functions: vec![
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 0,
                local_count: 1,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![
                    Instruction::Jump { offset: 1 },
                    Instruction::Call {
                        dest: Reg(0),
                        func: FuncIdx(1),
                        args_start: Reg(0),
                        arg_count: 0,
                    },
                    Instruction::RetNull,
                ],
            },
            BytecodeFunction {
                name: "dead-target".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![Instruction::RetNull],
            },
        ],
        types: vec![BytecodeType::Null],
        struct_layouts: Vec::new(),
        choice_layouts: Vec::new(),
        imports: Vec::new(),
        exports: vec![galfus_bytecode::ExportSlot {
            symbol_name: "main".to_string(),
            kind: ExportKind::Function(FuncIdx(0)),
        }],
        init_func_idx: None,
    };
    let graph = galfus_bytecode::BytecodeGraph::from_modules(
        SemanticRevision::new(1),
        vec![BytecodeNode {
            id: ModuleId::new(1),
            path: ModulePath::new("second-prune.gfs").expect("valid module path"),
            semantic_revision: SemanticRevision::new(1),
            module,
            metadata: None,
        }],
        Vec::new(),
    )
    .expect("valid graph");
    let package = Arc::new(
        PackageImage::try_new(
            graph,
            ExecutionTarget::new("test").expect("valid target"),
            None,
            galfus_bytecode::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid package"),
    );

    let finalized = optimize_package(package, None, SemanticRevision::new(2)).expect("finalizes");
    let module = &finalized.graph().get(ModuleId::new(1)).unwrap().module;
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].name, "main");
}

#[test]
fn finalization_restores_dynamic_dispatch_candidates_from_the_unfinalized_graph() {
    let package = |calls_method| {
        let instructions = if calls_method {
            vec![
                Instruction::CallMethod {
                    dest: Reg(0),
                    obj: Reg(0),
                    name_const: ConstIdx(0),
                    args_start: Reg(0),
                    arg_count: 1,
                },
                Instruction::RetNull,
            ]
        } else {
            vec![Instruction::RetNull]
        };
        let module = BytecodeModule {
            name: "dynamic.gfs".to_string(),
            global_count: 0,
            constants: ConstantPool {
                constants: vec![Constant::String("method".to_string())],
            },
            functions: vec![
                BytecodeFunction {
                    name: "main".to_string(),
                    param_count: 0,
                    local_count: 1,
                    temp_count: 0,
                    return_ty: TypeIdx(0),
                    adapter_proxy_metadata: None,
                    instructions,
                },
                BytecodeFunction {
                    name: "method".to_string(),
                    param_count: 0,
                    local_count: 0,
                    temp_count: 0,
                    return_ty: TypeIdx(0),
                    adapter_proxy_metadata: None,
                    instructions: vec![Instruction::RetNull],
                },
            ],
            types: vec![BytecodeType::Null],
            struct_layouts: Vec::new(),
            choice_layouts: Vec::new(),
            imports: Vec::new(),
            exports: vec![galfus_bytecode::ExportSlot {
                symbol_name: "main".to_string(),
                kind: ExportKind::Function(FuncIdx(0)),
            }],
            init_func_idx: None,
        };
        let graph = galfus_bytecode::BytecodeGraph::from_modules(
            SemanticRevision::new(1),
            vec![BytecodeNode {
                id: ModuleId::new(1),
                path: ModulePath::new("dynamic.gfs").expect("valid module path"),
                semantic_revision: SemanticRevision::new(1),
                module,
                metadata: None,
            }],
            Vec::new(),
        )
        .expect("valid graph");
        Arc::new(
            PackageImage::try_new(
                graph,
                ExecutionTarget::new("test").expect("valid target"),
                None,
                galfus_bytecode::PackageMetadata {
                    name: "test".into(),
                    version: None,
                    author: None,
                    email: None,
                    description: None,
                },
                LimitsMetadata::default(),
                Vec::new(),
                Vec::new(),
            )
            .expect("valid package"),
        )
    };

    let first = optimize_package(package(false), None, SemanticRevision::new(1))
        .expect("first finalization");
    assert_eq!(
        first
            .graph()
            .get(ModuleId::new(1))
            .unwrap()
            .module
            .functions
            .len(),
        1
    );

    let second = optimize_package(package(true), Some(&first), SemanticRevision::new(2))
        .expect("dynamic candidate finalization");
    assert_eq!(
        second
            .graph()
            .get(ModuleId::new(1))
            .unwrap()
            .module
            .functions
            .len(),
        2
    );
}

#[test]
fn prune_retains_a_tail_call_target_and_remaps_its_index() {
    let mut module = BytecodeModule {
        name: "tail-call.gfs".to_string(),
        global_count: 0,
        constants: ConstantPool {
            constants: vec![Constant::Int64(7)],
        },
        functions: vec![
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![Instruction::TailCall {
                    func: FuncIdx(1),
                    args_start: Reg(0),
                    arg_count: 0,
                }],
            },
            BytecodeFunction {
                name: "target".to_string(),
                param_count: 0,
                local_count: 1,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![
                    Instruction::LoadConst {
                        dest: Reg(0),
                        const_idx: galfus_bytecode::ConstIdx(0),
                    },
                    Instruction::Ret { src: Reg(0) },
                ],
            },
            BytecodeFunction {
                name: "unused".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![Instruction::RetNull],
            },
        ],
        types: vec![BytecodeType::Int64],
        struct_layouts: Vec::new(),
        choice_layouts: Vec::new(),
        imports: Vec::new(),
        exports: vec![galfus_bytecode::ExportSlot {
            symbol_name: "main".to_string(),
            kind: ExportKind::Function(FuncIdx(0)),
        }],
        init_func_idx: None,
    };

    let remap = prune_module(&mut module, &HashSet::new());

    assert_eq!(module.functions.len(), 2);
    assert_eq!(module.functions[0].name, "main");
    assert_eq!(module.functions[1].name, "target");
    assert_eq!(module.constants.constants.len(), 1);
    assert_eq!(remap, vec![Some(FuncIdx(0)), Some(FuncIdx(1)), None]);
    assert!(matches!(
        module.functions[0].instructions.as_slice(),
        [Instruction::TailCall {
            func: FuncIdx(1),
            ..
        }]
    ));
}

#[test]
fn compaction_preserves_parameter_abi_and_contiguous_operand_windows() {
    let mut function = BytecodeFunction {
        name: "compact.gfs".to_string(),
        param_count: 1,
        local_count: 5,
        temp_count: 3,
        return_ty: TypeIdx(0),
        adapter_proxy_metadata: None,
        instructions: vec![
            Instruction::LoadConst {
                dest: Reg(3),
                const_idx: ConstIdx(0),
            },
            Instruction::LoadConst {
                dest: Reg(6),
                const_idx: ConstIdx(1),
            },
            Instruction::NewTuple {
                dest: Reg(7),
                type_idx: TypeIdx(0),
                start: Reg(3),
                count: 2,
            },
            Instruction::AddI64 {
                dest: Reg(8),
                lhs: Reg(6),
                rhs: Reg(7),
            },
            Instruction::Ret { src: Reg(8) },
        ],
    };

    compact_registers(&mut function);

    assert_eq!(function.param_count, 1);
    assert_eq!(function.local_count, 2);
    assert_eq!(function.temp_count, 3);
    assert_eq!(
        function.instructions,
        vec![
            Instruction::LoadConst {
                dest: Reg(1),
                const_idx: ConstIdx(0),
            },
            Instruction::LoadConst {
                dest: Reg(3),
                const_idx: ConstIdx(1),
            },
            Instruction::NewTuple {
                dest: Reg(4),
                type_idx: TypeIdx(0),
                start: Reg(1),
                count: 2,
            },
            Instruction::AddI64 {
                dest: Reg(5),
                lhs: Reg(3),
                rhs: Reg(4),
            },
            Instruction::Ret { src: Reg(5) },
        ]
    );
}

#[test]
fn liveness_allocator_reuses_dead_registers() {
    let mut function = BytecodeFunction {
        name: "liveness.gfs".to_string(),
        param_count: 0,
        local_count: 3,
        temp_count: 0,
        return_ty: TypeIdx(0),
        adapter_proxy_metadata: None,
        instructions: vec![
            // Reg 0 is defined and used
            Instruction::LoadConst {
                dest: Reg(0),
                const_idx: ConstIdx(0),
            },
            Instruction::Move {
                dest: Reg(1),
                src: Reg(0),
            },
            // Reg 0 is now dead. Reg 2 is defined and used.
            // Liveness allocator should reuse the same physical register (0) for Reg(2)
            Instruction::LoadConst {
                dest: Reg(2),
                const_idx: ConstIdx(1),
            },
            Instruction::Ret { src: Reg(2) },
        ],
    };

    optimize_function(&mut function);

    // After allocation, it should have recognized that Reg(0) is dead before Reg(2) is defined.
    // So Reg(2) can be mapped to physical Reg(0).
    // The number of locals should be 2: one for Reg 1, and one reused for Reg 0 and Reg 2.
    assert_eq!(function.local_count, 2);
}

#[test]
fn liveness_allocator_reuses_dead_contiguous_windows() {
    let mut function = BytecodeFunction {
        name: "window-liveness.gfs".to_string(),
        param_count: 0,
        local_count: 6,
        temp_count: 0,
        return_ty: TypeIdx(0),
        adapter_proxy_metadata: None,
        instructions: vec![
            Instruction::LoadNull { dest: Reg(0) },
            Instruction::LoadNull { dest: Reg(1) },
            Instruction::NewTuple {
                dest: Reg(2),
                type_idx: TypeIdx(0),
                start: Reg(0),
                count: 2,
            },
            Instruction::Drop { reg: Reg(2) },
            Instruction::LoadNull { dest: Reg(3) },
            Instruction::LoadNull { dest: Reg(4) },
            Instruction::NewTuple {
                dest: Reg(5),
                type_idx: TypeIdx(0),
                start: Reg(3),
                count: 2,
            },
            Instruction::Ret { src: Reg(5) },
        ],
    };

    optimize_function(&mut function);

    let tuple_windows = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::NewTuple { start, count, .. } => Some((*start, *count)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tuple_windows, vec![(Reg(0), 2), (Reg(0), 2)]);
    assert_eq!(function.local_count, 3);
}
