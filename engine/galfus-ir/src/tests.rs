use super::*;
use galfus_core::{FunctionId, TypeId};

#[test]
fn test_pure_mir_construction() {
    let mut func = mir::MirFunction {
        id: FunctionId::new(0),
        name: "test_pure".to_string(),
        return_type: TypeId::new(0),
        parameter_types: vec![],
        locals: vec![],
        blocks: vec![],
        type_substitutions: std::collections::HashMap::new(),
        is_async: false,
    };

    let block = mir::BasicBlock {
        id: mir::BlockId::new(0),
        parameters: vec![],
        instructions: vec![],
        terminator: (mir::Terminator::Return(None), None),
    };
    func.blocks.push(block);

    let module = mir::MirModule {
        functions: vec![func],
        globals: vec![],
        constant_pool: vec![],
    };

    let result = validator::validate_module(&module);
    assert!(result.is_ok());
}

#[test]
fn operand_visitors_cover_nested_rvalues_instructions_and_terminators() {
    let first = mir::LocalId::new(1);
    let second = mir::LocalId::new(2);
    let third = mir::LocalId::new(3);
    let mut instruction = mir::Instruction::Assign(
        mir::LocalId::new(4),
        mir::RValue::CreateIndirectFuture {
            func: mir::Operand::Local(first),
            args: vec![mir::Operand::Local(second)],
        },
    );
    let mut terminator = mir::Terminator::Branch {
        cond: mir::Operand::Local(first),
        true_block: mir::BlockId::new(1),
        true_args: vec![mir::Operand::Local(second)],
        false_block: mir::BlockId::new(2),
        false_args: vec![mir::Operand::Local(third)],
    };

    let mut read_locals = Vec::new();
    for_each_instruction_operand(&instruction, |operand| {
        if let mir::Operand::Local(local) = operand {
            read_locals.push(*local);
        }
    });
    assert_eq!(read_locals, vec![first, second]);

    for_each_instruction_operand_mut(&mut instruction, |operand| {
        if let mir::Operand::Local(local) = operand {
            *local = mir::LocalId::new(local.raw() + 10);
        }
    });
    for_each_terminator_operand_mut(&mut terminator, |operand| {
        if let mir::Operand::Local(local) = operand {
            *local = mir::LocalId::new(local.raw() + 10);
        }
    });

    let mut rewritten_locals = Vec::new();
    for_each_instruction_operand(&instruction, |operand| {
        if let mir::Operand::Local(local) = operand {
            rewritten_locals.push(*local);
        }
    });
    assert_eq!(
        rewritten_locals,
        vec![mir::LocalId::new(11), mir::LocalId::new(12)]
    );

    let mut terminator_locals = Vec::new();
    for_each_terminator_operand(&terminator, |operand| {
        if let mir::Operand::Local(local) = operand {
            terminator_locals.push(*local);
        }
    });
    assert_eq!(
        terminator_locals,
        vec![
            mir::LocalId::new(11),
            mir::LocalId::new(12),
            mir::LocalId::new(13)
        ]
    );
}
