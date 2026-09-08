#[cfg(test)]
mod tests;

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};

use galfus_ir::mir::{
    BlockId, Constant, Instruction, LocalId, MirBinaryOp, MirFunction, MirModule, Operand, RValue,
    Terminator,
};
use galfus_ir::{
    for_each_instruction_operand, for_each_instruction_operand_mut, for_each_terminator_operand,
    for_each_terminator_operand_mut,
};

use super::{inline::inline_functions, tco::optimize_tail_calls};

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

fn propagate_and_fold_constants(module: &mut MirModule) -> usize {
    let mut changed = 0;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            let mut constants = HashMap::<galfus_ir::mir::LocalId, Constant>::new();
            for (instruction, _) in &mut block.instructions {
                let Instruction::Assign(destination, rvalue) = instruction else {
                    continue;
                };
                match rvalue {
                    RValue::Use(Operand::Local(local)) => {
                        if let Some(value) = constants.get(local).cloned() {
                            *rvalue = RValue::Use(Operand::Constant(value.clone()));
                            constants.insert(*destination, value);
                            changed += 1;
                        } else {
                            constants.remove(destination);
                        }
                    }
                    RValue::Use(Operand::Constant(value)) => {
                        constants.insert(*destination, value.clone());
                    }
                    RValue::BinaryOp(operation, lhs, rhs) => {
                        let lhs = resolve_constant(lhs, &constants);
                        let rhs = resolve_constant(rhs, &constants);
                        if let Some(value) =
                            fold_primitive_binary(*operation, lhs.as_ref(), rhs.as_ref())
                        {
                            *rvalue = RValue::Use(Operand::Constant(value.clone()));
                            constants.insert(*destination, value);
                            changed += 1;
                        } else {
                            constants.remove(destination);
                        }
                    }
                    _ => {
                        constants.remove(destination);
                    }
                }
            }
        }
    }
    changed
}

fn resolve_constant(
    operand: &Operand,
    constants: &HashMap<galfus_ir::mir::LocalId, Constant>,
) -> Option<Constant> {
    match operand {
        Operand::Constant(value) => Some(value.clone()),
        Operand::Local(local) => constants.get(local).cloned(),
        Operand::ConstRef(_) => None,
    }
}

fn fold_primitive_binary(
    operation: MirBinaryOp,
    lhs: Option<&Constant>,
    rhs: Option<&Constant>,
) -> Option<Constant> {
    macro_rules! fold {
        ($left:expr, $right:expr, $variant:ident) => {
            Some(match operation {
                MirBinaryOp::Add => Constant::$variant($left.wrapping_add($right)),
                MirBinaryOp::Subtract => Constant::$variant($left.wrapping_sub($right)),
                MirBinaryOp::Multiply => Constant::$variant($left.wrapping_mul($right)),
                MirBinaryOp::Divide if $right != 0 => {
                    Constant::$variant($left.wrapping_div($right))
                }
                MirBinaryOp::Remainder if $right != 0 => {
                    Constant::$variant($left.wrapping_rem($right))
                }
                MirBinaryOp::Equal => Constant::Bool($left == $right),
                MirBinaryOp::NotEqual => Constant::Bool($left != $right),
                MirBinaryOp::Less => Constant::Bool($left < $right),
                MirBinaryOp::LessEqual => Constant::Bool($left <= $right),
                MirBinaryOp::Greater => Constant::Bool($left > $right),
                MirBinaryOp::GreaterEqual => Constant::Bool($left >= $right),
                _ => return None,
            })
        };
    }
    macro_rules! fold_float {
        ($left:expr, $right:expr, $variant:ident) => {
            Some(match operation {
                MirBinaryOp::Add => Constant::$variant($left + $right),
                MirBinaryOp::Subtract => Constant::$variant($left - $right),
                MirBinaryOp::Multiply => Constant::$variant($left * $right),
                MirBinaryOp::Divide => Constant::$variant($left / $right),
                MirBinaryOp::Remainder => Constant::$variant($left % $right),
                MirBinaryOp::Equal => Constant::Bool($left == $right),
                MirBinaryOp::NotEqual => Constant::Bool($left != $right),
                MirBinaryOp::Less => Constant::Bool($left < $right),
                MirBinaryOp::LessEqual => Constant::Bool($left <= $right),
                MirBinaryOp::Greater => Constant::Bool($left > $right),
                MirBinaryOp::GreaterEqual => Constant::Bool($left >= $right),
                _ => return None,
            })
        };
    }
    match (lhs?, rhs?) {
        (Constant::Int8(left), Constant::Int8(right)) => fold!(*left, *right, Int8),
        (Constant::Int16(left), Constant::Int16(right)) => fold!(*left, *right, Int16),
        (Constant::Int32(left), Constant::Int32(right)) => fold!(*left, *right, Int32),
        (Constant::Int64(left), Constant::Int64(right)) => fold!(*left, *right, Int64),
        (Constant::Uint8(left), Constant::Uint8(right)) => fold!(*left, *right, Uint8),
        (Constant::Uint16(left), Constant::Uint16(right)) => fold!(*left, *right, Uint16),
        (Constant::Uint32(left), Constant::Uint32(right)) => fold!(*left, *right, Uint32),
        (Constant::Uint64(left), Constant::Uint64(right)) => fold!(*left, *right, Uint64),
        (Constant::Float32(left), Constant::Float32(right)) => fold_float!(*left, *right, Float32),
        (Constant::Float64(left), Constant::Float64(right)) => fold_float!(*left, *right, Float64),
        _ => None,
    }
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

struct Dominators {
    indices: HashMap<BlockId, usize>,
    sets: Vec<Vec<bool>>,
}

impl Dominators {
    fn dominates(&self, block: BlockId, candidate: BlockId) -> bool {
        let Some(&block_index) = self.indices.get(&block) else {
            return false;
        };
        let Some(&candidate_index) = self.indices.get(&candidate) else {
            return false;
        };
        self.sets[block_index][candidate_index]
    }
}

fn dominators(function: &MirFunction) -> Dominators {
    let indices = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect::<HashMap<_, _>>();
    let entry = function.blocks.first().map(|block| block.id);
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in successors(&block.terminator.0) {
            if let (Some(&source), Some(&target)) =
                (indices.get(&block.id), indices.get(&successor))
            {
                predecessors[target].push(source);
            }
        }
    }
    let mut sets = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            if Some(block.id) == entry || predecessors[index].is_empty() {
                let mut initial = vec![false; function.blocks.len()];
                initial[index] = true;
                initial
            } else {
                vec![true; function.blocks.len()]
            }
        })
        .collect::<Vec<_>>();
    let mut changed = true;
    while changed {
        changed = false;
        for (index, block) in function.blocks.iter().enumerate() {
            if Some(block.id) == entry {
                continue;
            }
            let Some((&first, rest)) = predecessors[index].split_first() else {
                continue;
            };
            let mut next = sets[first].clone();
            for predecessor in rest {
                for (candidate, dominates) in next.iter_mut().zip(&sets[*predecessor]) {
                    *candidate &= *dominates;
                }
            }
            next[index] = true;
            if sets[index] != next {
                sets[index] = next;
                changed = true;
            }
        }
    }
    Dominators { indices, sets }
}

fn successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            true_block,
            false_block,
            ..
        } => vec![*true_block, *false_block],
        _ => Vec::new(),
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

fn remove_dead_constant_definitions(module: &mut MirModule) -> usize {
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

fn collect_operand(operand: &Operand, used: &mut HashSet<galfus_ir::mir::LocalId>) {
    if let Operand::Local(local) = operand {
        used.insert(*local);
    }
}
fn collect_instruction_uses(
    instruction: &Instruction,
    used: &mut HashSet<galfus_ir::mir::LocalId>,
) {
    if let Instruction::Drop(local) = instruction {
        used.insert(*local);
        return;
    }
    for_each_instruction_operand(instruction, |operand| collect_operand(operand, used));
}
fn collect_terminator_uses(terminator: &Terminator, used: &mut HashSet<galfus_ir::mir::LocalId>) {
    for_each_terminator_operand(terminator, |operand| collect_operand(operand, used));
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
