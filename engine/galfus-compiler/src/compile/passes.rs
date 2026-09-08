mod analysis;
mod constants;
mod dead_definitions;
#[cfg(test)]
mod tests;

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};

use galfus_ir::mir::{
    BlockId, Instruction, LocalId, MirFunction, MirModule, Operand, RValue, Terminator,
};
use galfus_ir::{for_each_instruction_operand_mut, for_each_terminator_operand_mut};

use super::{inline::inline_functions, tco::optimize_tail_calls};
use analysis::{Dominators, dominators};
use constants::propagate_and_fold_constants;
use dead_definitions::remove_dead_constant_definitions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirPassConfiguration {
    pub local_simplification: bool,
    pub constant_propagation: bool,
    pub copy_propagation: bool,
    pub dead_definitions: bool,
    pub inlining: bool,
    pub max_inline_instructions: usize,
    pub tail_calls: bool,
}

impl Default for MirPassConfiguration {
    fn default() -> Self {
        Self {
            local_simplification: true,
            constant_propagation: true,
            copy_propagation: true,
            dead_definitions: true,
            inlining: true,
            max_inline_instructions: 512,
            tail_calls: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MirPassReport {
    pub instructions_before: usize,
    pub instructions_after: usize,
    pub calls_before: usize,
    pub calls_after: usize,
    pub simplified_instructions: usize,
    pub folded_constants: usize,
    pub propagated_copies: usize,
    pub removed_dead_definitions: usize,
    pub inlined_calls: usize,
    pub tail_calls: usize,
    pub call_graph_changed: bool,
}

pub fn run(module: &mut MirModule, configuration: MirPassConfiguration) -> Result<MirPassReport> {
    validate(module, "before")?;
    let mut report = MirPassReport {
        instructions_before: instruction_count(module),
        calls_before: call_count(module),
        ..MirPassReport::default()
    };

    // MIR construction converts each function to SSA before this manager runs.
    // The passes below therefore never rewrite a local outside its defining block.
    if configuration.local_simplification {
        report.simplified_instructions = simplify_local_identities(module);
        validate(module, "after local simplification")?;
    }
    if configuration.constant_propagation {
        report.folded_constants = propagate_and_fold_constants(module);
        validate(module, "after constant propagation")?;
    }
    if configuration.copy_propagation {
        report.propagated_copies = propagate_ssa_copies(module);
        validate(module, "after copy propagation")?;
    }
    if configuration.dead_definitions {
        report.removed_dead_definitions = remove_dead_constant_definitions(module);
        validate(module, "after dead definition elimination")?;
    }
    if configuration.inlining {
        report.inlined_calls = inline_functions(module, configuration.max_inline_instructions);
        validate(module, "after inlining")?;
    }
    if configuration.tail_calls {
        report.tail_calls = optimize_tail_calls(module);
        validate(module, "after tail-call recognition")?;
    }

    report.instructions_after = instruction_count(module);
    report.calls_after = call_count(module);
    report.call_graph_changed = report.inlined_calls > 0;
    Ok(report)
}

type CopyDefinition = (LocalId, BlockId, usize);

fn propagate_ssa_copies(module: &mut MirModule) -> usize {
    module
        .functions
        .iter_mut()
        .map(|function| {
            // Work on one candidate and publish it only after validation. This
            // globals and constant pool for every changed function.
            let mut candidate = function.clone();
            let replaced = propagate_function_copies(&mut candidate);
            if replaced == 0 {
                return 0;
            }
            if galfus_ir::validate_function(&candidate).is_err() {
                0
            } else {
                *function = candidate;
                replaced
            }
        })
        .sum()
}

fn propagate_function_copies(function: &mut MirFunction) -> usize {
    let ownership = function
        .locals
        .iter()
        .map(|local| (local.id, local.is_owned))
        .collect::<HashMap<_, _>>();
    let definitions = collect_copy_definitions(function, &ownership);
    if definitions.is_empty() {
        return 0;
    }
    let dominators = dominators(function);
    let mut replaced = 0;
    for block in &mut function.blocks {
        for (index, (instruction, _)) in block.instructions.iter_mut().enumerate() {
            replace_instruction_copies(
                instruction,
                block.id,
                index,
                &definitions,
                &dominators,
                &mut replaced,
            );
        }
        replace_terminator_copies(
            &mut block.terminator.0,
            block.id,
            usize::MAX,
            &definitions,
            &dominators,
            &mut replaced,
        );
    }
    replaced
}

fn collect_copy_definitions(
    function: &MirFunction,
    ownership: &HashMap<LocalId, bool>,
) -> HashMap<LocalId, CopyDefinition> {
    let mut definition_counts = function
        .parameter_types
        .iter()
        .enumerate()
        .map(|(index, _)| (LocalId::new(index as u32), 1_usize))
        .collect::<HashMap<_, _>>();
    for block in &function.blocks {
        for parameter in &block.parameters {
            *definition_counts.entry(parameter.id).or_default() += 1;
        }
        for (instruction, _) in &block.instructions {
            if let Some(destination) = instruction_destination(instruction) {
                *definition_counts.entry(destination).or_default() += 1;
            }
        }
    }
    let mut definitions = HashMap::new();
    for block in &function.blocks {
        for (index, (instruction, _)) in block.instructions.iter().enumerate() {
            let Instruction::Assign(destination, RValue::Use(Operand::Local(source))) = instruction
            else {
                continue;
            };
            if !ownership.get(destination).copied().unwrap_or(true)
                && !ownership.get(source).copied().unwrap_or(true)
                && definition_counts.get(destination) == Some(&1)
                && definition_counts.get(source) == Some(&1)
            {
                definitions.insert(*destination, (*source, block.id, index));
            }
        }
    }
    definitions
}

fn instruction_destination(instruction: &Instruction) -> Option<LocalId> {
    match instruction {
        Instruction::Assign(destination, _)
        | Instruction::Call { destination, .. }
        | Instruction::IndirectCall { destination, .. }
        | Instruction::ConstraintCall { destination, .. }
        | Instruction::Await { destination, .. }
        | Instruction::AwaitAll { destination, .. }
        | Instruction::AwaitRace { destination, .. } => Some(*destination),
        _ => None,
    }
}

fn replace_operand_copy(
    operand: &mut Operand,
    block: BlockId,
    use_index: usize,
    definitions: &HashMap<LocalId, CopyDefinition>,
    dominators: &Dominators,
    replaced: &mut usize,
) {
    let Operand::Local(local) = operand else {
        return;
    };
    let mut source = *local;
    let mut seen = HashSet::new();
    while seen.insert(source) {
        let Some((next, definition_block, definition_index)) = definitions.get(&source) else {
            break;
        };
        let dominates = dominators.dominates(block, *definition_block);
        if !dominates || (*definition_block == block && *definition_index >= use_index) {
            break;
        }
        source = *next;
    }
    if source != *local {
        *local = source;
        *replaced += 1;
    }
}

fn replace_instruction_copies(
    instruction: &mut Instruction,
    block: BlockId,
    use_index: usize,
    definitions: &HashMap<LocalId, CopyDefinition>,
    dominators: &Dominators,
    replaced: &mut usize,
) {
    if let Instruction::Drop(local) = instruction {
        let mut operand = Operand::Local(*local);
        replace_operand_copy(
            &mut operand,
            block,
            use_index,
            definitions,
            dominators,
            replaced,
        );
        if let Operand::Local(replacement) = operand {
            *local = replacement;
        }
        return;
    }
    let replace = |operand: &mut Operand| {
        replace_operand_copy(operand, block, use_index, definitions, dominators, replaced);
    };
    for_each_instruction_operand_mut(instruction, replace);
}

fn replace_terminator_copies(
    terminator: &mut Terminator,
    block: BlockId,
    use_index: usize,
    definitions: &HashMap<LocalId, CopyDefinition>,
    dominators: &Dominators,
    replaced: &mut usize,
) {
    let replace = |operand: &mut Operand| {
        replace_operand_copy(operand, block, use_index, definitions, dominators, replaced);
    };
    for_each_terminator_operand_mut(terminator, replace);
}

fn validate(module: &MirModule, stage: &str) -> Result<()> {
    galfus_ir::validate_module(module)
        .map_err(|errors| anyhow!("MIR validation failed {stage}: {errors:?}"))
}

fn simplify_local_identities(module: &mut MirModule) -> usize {
    let mut removed = 0;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            block.instructions.retain(|(instruction, _)| {
                let is_identity = matches!(
                    instruction,
                    Instruction::Assign(destination, RValue::Use(Operand::Local(source)))
                        if destination == source
                );
                removed += usize::from(is_identity);
                !is_identity
            });
        }
    }
    removed
}

fn instruction_count(module: &MirModule) -> usize {
    module
        .functions
        .iter()
        .map(|function| {
            function
                .blocks
                .iter()
                .map(|block| block.instructions.len() + 1)
                .sum::<usize>()
        })
        .sum()
}

fn call_count(module: &MirModule) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|(instruction, _)| matches!(instruction, Instruction::Call { .. }))
        .count()
}
