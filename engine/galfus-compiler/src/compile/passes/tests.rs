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
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
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
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
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
            inlining: true,
            tail_calls: false,
        },
    )
    .unwrap();

    assert_eq!(report.inlined_calls, 1);
    assert_eq!(report.calls_before, 1);
    assert_eq!(report.calls_after, 0);
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
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
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
            },
            LocalDecl {
                id: LocalId::new(1),
                ty,
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
            inlining: false,
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
