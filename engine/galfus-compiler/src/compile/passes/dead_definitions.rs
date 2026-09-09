use galfus_ir::mir::{Constant, Instruction, LocalId, MirModule, Operand, RValue, Terminator};
use galfus_ir::{for_each_instruction_operand, for_each_terminator_operand};
use std::collections::{HashMap, HashSet};

pub(super) fn remove_dead_constant_definitions(module: &mut MirModule) -> usize {
    let mut removed = 0;
    for function in &mut module.functions {
        let ownership = function
            .locals
            .iter()
            .map(|local| (local.id, local.is_owned))
            .collect::<HashMap<_, _>>();
        let mut used = HashSet::new();
        for block in &function.blocks {
            for (instruction, _) in &block.instructions {
                collect_instruction_uses(instruction, &mut used);
            }
            collect_terminator_uses(&block.terminator.0, &mut used);
        }
        for block in &mut function.blocks {
            block.instructions.retain(|(instruction, _)| {
                let dead = match instruction {
                    Instruction::Assign(destination, RValue::Use(Operand::Constant(constant))) => {
                        !used.contains(destination)
                            && !ownership.get(destination).copied().unwrap_or(true)
                            && is_trivially_discardable_constant(constant)
                    }
                    Instruction::Assign(destination, RValue::Use(Operand::Local(source))) => {
                        !used.contains(destination)
                            && !ownership.get(destination).copied().unwrap_or(true)
                            && !ownership.get(source).copied().unwrap_or(true)
                    }
                    _ => false,
                };
                removed += usize::from(dead);
                !dead
            });
        }
    }
    removed
}

fn is_trivially_discardable_constant(constant: &Constant) -> bool {
    matches!(
        constant,
        Constant::Null
            | Constant::Bool(_)
            | Constant::Int8(_)
            | Constant::Int16(_)
            | Constant::Int32(_)
            | Constant::Int64(_)
            | Constant::Uint8(_)
            | Constant::Uint16(_)
            | Constant::Uint32(_)
            | Constant::Uint64(_)
            | Constant::Float32(_)
            | Constant::Float64(_)
            | Constant::Function(_)
    )
}

fn collect_operand(operand: &Operand, used: &mut HashSet<LocalId>) {
    if let Operand::Local(local) = operand {
        used.insert(*local);
    }
}

fn collect_instruction_uses(instruction: &Instruction, used: &mut HashSet<LocalId>) {
    if let Instruction::Drop(local) = instruction {
        used.insert(*local);
        return;
    }
    for_each_instruction_operand(instruction, |operand| collect_operand(operand, used));
}

fn collect_terminator_uses(terminator: &Terminator, used: &mut HashSet<LocalId>) {
    for_each_terminator_operand(terminator, |operand| collect_operand(operand, used));
}
