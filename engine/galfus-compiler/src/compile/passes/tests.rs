use galfus_core::{FunctionId, TypeId};
use galfus_ir::mir::{
    BasicBlock, Constant, Instruction, LocalDecl, LocalId, MirBinaryOp, MirFunction, MirModule,
    Operand, RValue, Terminator,
};

use super::{MirPassConfiguration, run};

fn function(
    id: u32,
    locals: Vec<LocalDecl>,
    parameter_types: Vec<TypeId>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
) -> MirFunction {
    MirFunction {
        id: FunctionId::new(id),
        name: format!("function_{id}"),
        return_type: TypeId::new(0),
        parameter_types,
        locals,
        blocks: vec![BasicBlock {
            id: galfus_ir::mir::BlockId::new(0),
            parameters: Vec::new(),
            instructions: instructions
                .into_iter()
                .map(|instruction| (instruction, None))
                .collect(),
            terminator: (terminator, None),
        }],
        type_substitutions: Default::default(),
        is_async: false,
    }
}

#[test]
fn inlining_is_independently_enabled_and_reports_removed_call() {
    let ty = TypeId::new(0);
    let callee = function(
        1,
        vec![
            LocalDecl {
                id: LocalId::new(0),
                ty,
                is_owned: false,
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
                is_owned: false,
            },
        ],
        vec![ty],
        vec![Instruction::Assign(
            LocalId::new(1),
            RValue::BinaryOp(
                MirBinaryOp::Add,
                Operand::Local(LocalId::new(0)),
                Operand::Constant(Constant::Int32(1)),
            ),
        )],
        Terminator::Return(Some(Operand::Local(LocalId::new(1)))),
    );
    let caller = function(
        2,
        vec![
            LocalDecl {
                id: LocalId::new(0),
                ty,
                is_owned: false,
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
                is_owned: false,
            },
        ],
        vec![ty],
        vec![Instruction::Call {
            func: FunctionId::new(1),
            args: vec![Operand::Local(LocalId::new(0))],
            destination: LocalId::new(1),
            is_external: false,
        }],
        Terminator::Return(Some(Operand::Local(LocalId::new(1)))),
    );
    let mut module = MirModule {
        functions: vec![callee, caller],
        globals: Vec::new(),
        constant_pool: Vec::new(),
    };

    let report = run(
        &mut module,
        MirPassConfiguration {
            local_simplification: false,
            constant_propagation: false,
            copy_propagation: false,
            dead_definitions: false,
            inlining: true,
            max_inline_instructions: 512,
            tail_calls: false,
        },
    )
    .unwrap();

    assert_eq!(report.inlined_calls, 1);
    assert_eq!(report.calls_before, 1);
    assert_eq!(report.calls_after, 0);
    assert!(report.call_graph_changed);
}

#[test]
fn inlining_respects_the_function_instruction_budget() {
    let ty = TypeId::new(0);
    let callee = function(
        1,
        vec![LocalDecl {
            id: LocalId::new(0),
            ty,
            is_owned: false,
        }],
        vec![ty],
        Vec::new(),
        Terminator::Return(Some(Operand::Local(LocalId::new(0)))),
    );
    let caller = function(
        2,
        vec![
            LocalDecl {
                id: LocalId::new(0),
                ty,
                is_owned: false,
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
                is_owned: false,
            },
        ],
        vec![ty],
        vec![Instruction::Call {
            func: FunctionId::new(1),
            args: vec![Operand::Local(LocalId::new(0))],
            destination: LocalId::new(1),
            is_external: false,
        }],
        Terminator::Return(Some(Operand::Local(LocalId::new(1)))),
    );
    let mut module = MirModule {
        functions: vec![callee, caller],
        globals: Vec::new(),
        constant_pool: Vec::new(),
    };

    let report = run(
        &mut module,
        MirPassConfiguration {
            local_simplification: false,
            constant_propagation: false,
            copy_propagation: false,
            dead_definitions: false,
            inlining: true,
            max_inline_instructions: 1,
            tail_calls: false,
        },
    )
    .unwrap();

    assert_eq!(report.inlined_calls, 0);
    assert_eq!(report.calls_after, 1);
}

#[test]
fn tail_calls_are_independently_enabled_and_exclude_external_calls() {
    let ty = TypeId::new(0);
    let local_function = function(
        1,
        vec![
            LocalDecl {
                id: LocalId::new(0),
                ty,
                is_owned: false,
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
                is_owned: false,
            },
        ],
        vec![ty],
        vec![Instruction::Call {
            func: FunctionId::new(1),
            args: vec![Operand::Local(LocalId::new(0))],
            destination: LocalId::new(1),
            is_external: false,
        }],
        Terminator::Return(Some(Operand::Local(LocalId::new(1)))),
    );
    let external_function = function(
        2,
        vec![
            LocalDecl {
                id: LocalId::new(0),
                ty,
                is_owned: false,
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
                is_owned: false,
            },
        ],
        vec![ty],
        vec![Instruction::Call {
            func: FunctionId::new(3),
            args: vec![Operand::Local(LocalId::new(0))],
            destination: LocalId::new(1),
            is_external: true,
        }],
        Terminator::Return(Some(Operand::Local(LocalId::new(1)))),
    );
    let mut module = MirModule {
        functions: vec![local_function, external_function],
        globals: Vec::new(),
        constant_pool: Vec::new(),
    };

    let report = run(
        &mut module,
        MirPassConfiguration {
            local_simplification: false,
            constant_propagation: false,
            copy_propagation: false,
            dead_definitions: false,
            inlining: false,
            max_inline_instructions: 512,
            tail_calls: true,
        },
    )
    .unwrap();

    assert_eq!(report.tail_calls, 1);
    assert!(matches!(
        module.functions[0].blocks[0].terminator.0,
        Terminator::TailCall { .. }
    ));
    assert!(matches!(
        module.functions[1].blocks[0].terminator.0,
        Terminator::Return(_)
    ));
}

#[test]
fn constant_propagation_folds_without_removing_observable_instructions() {
    let ty = TypeId::new(0);
    let mut module = MirModule {
        functions: vec![function(
            1,
            vec![
                LocalDecl {
                    id: LocalId::new(0),
                    ty,
                    is_owned: false,
                },
                LocalDecl {
                    id: LocalId::new(1),
                    ty,
                    is_owned: false,
                },
                LocalDecl {
                    id: LocalId::new(2),
                    ty,
                    is_owned: false,
                },
            ],
            Vec::new(),
            vec![
                Instruction::Assign(
                    LocalId::new(0),
                    RValue::Use(Operand::Constant(Constant::Int32(2))),
                ),
                Instruction::Assign(
                    LocalId::new(1),
                    RValue::Use(Operand::Constant(Constant::Int32(3))),
                ),
                Instruction::Assign(
                    LocalId::new(2),
                    RValue::BinaryOp(
                        MirBinaryOp::Add,
                        Operand::Local(LocalId::new(0)),
                        Operand::Local(LocalId::new(1)),
                    ),
                ),
            ],
            Terminator::Return(Some(Operand::Local(LocalId::new(2)))),
        )],
        globals: Vec::new(),
        constant_pool: Vec::new(),
    };

    let report = run(
        &mut module,
        MirPassConfiguration {
            local_simplification: false,
            constant_propagation: true,
            copy_propagation: false,
            dead_definitions: true,
            inlining: false,
            max_inline_instructions: 512,
            tail_calls: false,
        },
    )
    .unwrap();

    assert_eq!(report.folded_constants, 1);
    assert_eq!(report.removed_dead_definitions, 2);
    assert!(matches!(
        module.functions[0].blocks[0].instructions.as_slice(),
        [(
            Instruction::Assign(_, RValue::Use(Operand::Constant(Constant::Int32(5)))),
            _
        )]
    ));
}

#[test]
fn copy_propagation_rewrites_dominated_primitive_uses() {
    let ty = TypeId::new(0);
    let mut module = MirModule {
        functions: vec![function(
            1,
            vec![
                LocalDecl {
                    id: LocalId::new(0),
                    ty,
                    is_owned: false,
                },
                LocalDecl {
                    id: LocalId::new(1),
                    ty,
                    is_owned: false,
                },
                LocalDecl {
                    id: LocalId::new(2),
                    ty,
                    is_owned: false,
                },
            ],
            Vec::new(),
            vec![
                Instruction::Assign(
                    LocalId::new(0),
                    RValue::Use(Operand::Constant(Constant::Int32(2))),
                ),
                Instruction::Assign(
                    LocalId::new(1),
                    RValue::Use(Operand::Local(LocalId::new(0))),
                ),
                Instruction::Assign(
                    LocalId::new(2),
                    RValue::BinaryOp(
                        MirBinaryOp::Add,
                        Operand::Local(LocalId::new(1)),
                        Operand::Constant(Constant::Int32(1)),
                    ),
                ),
            ],
            Terminator::Return(Some(Operand::Local(LocalId::new(2)))),
        )],
        globals: Vec::new(),
        constant_pool: Vec::new(),
    };

    let report = run(
        &mut module,
        MirPassConfiguration {
            local_simplification: false,
            constant_propagation: false,
            copy_propagation: true,
            dead_definitions: true,
            inlining: false,
            max_inline_instructions: 512,
            tail_calls: false,
        },
    )
    .unwrap();

    assert_eq!(report.propagated_copies, 1);
    assert_eq!(report.removed_dead_definitions, 1);
    assert!(matches!(
        module.functions[0].blocks[0].instructions.as_slice(),
        [
            (Instruction::Assign(_, RValue::Use(Operand::Constant(_))), _),
            (
                Instruction::Assign(
                    _,
                    RValue::BinaryOp(_, Operand::Local(local), Operand::Constant(_))
                ),
                _
            )
        ] if *local == LocalId::new(0)
    ));
}

#[test]
fn copy_propagation_preserves_owned_values() {
    let ty = TypeId::new(0);
    let mut module = MirModule {
        functions: vec![function(
            1,
            vec![
                LocalDecl {
                    id: LocalId::new(0),
                    ty,
                    is_owned: true,
                },
                LocalDecl {
                    id: LocalId::new(1),
                    ty,
                    is_owned: true,
                },
            ],
            vec![ty],
            vec![Instruction::Assign(
                LocalId::new(1),
                RValue::Use(Operand::Local(LocalId::new(0))),
            )],
            Terminator::Return(None),
        )],
        globals: Vec::new(),
        constant_pool: Vec::new(),
    };

    let report = run(
        &mut module,
        MirPassConfiguration {
            local_simplification: false,
            constant_propagation: false,
            copy_propagation: true,
            dead_definitions: true,
            inlining: false,
            max_inline_instructions: 512,
            tail_calls: false,
        },
    )
    .unwrap();

    assert_eq!(report.propagated_copies, 0);
    assert_eq!(report.removed_dead_definitions, 0);
}
