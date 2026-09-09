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
        validate_if_changed(
            module,
            "after local simplification",
            report.simplified_instructions,
        )?;
    }
    if configuration.constant_propagation {
        report.folded_constants = propagate_and_fold_constants(module);
        validate_if_changed(
            module,
            "after constant propagation",
            report.folded_constants,
        )?;
    }
    if configuration.copy_propagation {
        report.propagated_copies = propagate_ssa_copies(module);
        validate_if_changed(module, "after copy propagation", report.propagated_copies)?;
    }
    if configuration.dead_definitions {
        report.removed_dead_definitions = remove_dead_constant_definitions(module);
        validate_if_changed(
            module,
            "after dead definition elimination",
            report.removed_dead_definitions,
        )?;
    }
    if configuration.inlining {
        report.inlined_calls = inline_functions(module, configuration.max_inline_instructions);
        validate_if_changed(module, "after inlining", report.inlined_calls)?;
    }
    if configuration.tail_calls {
        report.tail_calls = optimize_tail_calls(module);
        validate_if_changed(module, "after tail-call recognition", report.tail_calls)?;
    }

    report.instructions_after = instruction_count(module);
    report.calls_after = call_count(module);
    report.call_graph_changed = report.inlined_calls > 0;
    Ok(report)
}

type CopyDefinition = (LocalId, BlockId, usize);
type IndexedCopyDefinition = (LocalId, usize, usize);

fn propagate_ssa_copies(module: &mut MirModule) -> usize {
    module
        .functions
        .iter_mut()
        .map(|function| {
            let definitions = collect_copy_definitions(function);
            if definitions.is_empty() {
                return 0;
            }

            // Work on one candidate and publish it only after validation. This
            // avoids cloning the rest of the module, and skips cloning
            // functions that contain no copy-propagation candidates.
            let mut candidate = function.clone();
            let replaced = propagate_function_copies(&mut candidate, &definitions);
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

fn propagate_function_copies(
    function: &mut MirFunction,
    definitions: &HashMap<LocalId, CopyDefinition>,
) -> usize {
    let dominators = dominators(function);
    let definitions = definitions
        .iter()
        .filter_map(|(&destination, &(source, block, instruction_index))| {
            dominators
                .block_index(block)
                .map(|block_index| (destination, (source, block_index, instruction_index)))
        })
        .collect::<HashMap<_, IndexedCopyDefinition>>();
    if definitions.is_empty() {
        return 0;
    }

    let mut replaced = 0;
    for (block_index, block) in function.blocks.iter_mut().enumerate() {
        for (index, (instruction, _)) in block.instructions.iter_mut().enumerate() {
            replace_instruction_copies(
                instruction,
                block_index,
                index,
                &definitions,
                &dominators,
                &mut replaced,
            );
        }
        replace_terminator_copies(
            &mut block.terminator.0,
            block_index,
            usize::MAX,
            &definitions,
            &dominators,
            &mut replaced,
        );
    }
    replaced
}

fn collect_copy_definitions(function: &MirFunction) -> HashMap<LocalId, CopyDefinition> {
    let ownership = function
        .locals
        .iter()
        .map(|local| (local.id, local.is_owned))
        .collect::<HashMap<_, _>>();
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
    block_index: usize,
    use_index: usize,
    definitions: &HashMap<LocalId, IndexedCopyDefinition>,
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
        if !dominators.dominates_indices(block_index, *definition_block)
            || (*definition_block == block_index && *definition_index >= use_index)
        {
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
    block_index: usize,
    use_index: usize,
    definitions: &HashMap<LocalId, IndexedCopyDefinition>,
    dominators: &Dominators,
    replaced: &mut usize,
) {
    if let Instruction::Drop(local) = instruction {
        let mut operand = Operand::Local(*local);
        replace_operand_copy(
            &mut operand,
            block_index,
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
        replace_operand_copy(
            operand,
            block_index,
            use_index,
            definitions,
            dominators,
            replaced,
        );
    };
    for_each_instruction_operand_mut(instruction, replace);
}

fn replace_terminator_copies(
    terminator: &mut Terminator,
    block_index: usize,
    use_index: usize,
    definitions: &HashMap<LocalId, IndexedCopyDefinition>,
    dominators: &Dominators,
    replaced: &mut usize,
) {
    let replace = |operand: &mut Operand| {
        replace_operand_copy(
            operand,
            block_index,
            use_index,
            definitions,
            dominators,
            replaced,
        );
    };
    for_each_terminator_operand_mut(terminator, replace);
}

fn validate(module: &MirModule, stage: &str) -> Result<()> {
    galfus_ir::validate_module(module)
        .map_err(|errors| anyhow!("MIR validation failed {stage}: {errors:?}"))
}

fn validate_if_changed(module: &MirModule, stage: &str, changes: usize) -> Result<()> {
    if changes == 0 {
        return Ok(());
    }
    validate(module, stage)
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
