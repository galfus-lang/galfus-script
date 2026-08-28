#[cfg(test)]
mod tests;

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};

use galfus_ir::mir::{
    BlockId, Constant, Instruction, LocalId, MirBinaryOp, MirFunction, MirModule, Operand, RValue,
    Terminator,
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
    let globals = module.globals.clone();
    let constant_pool = module.constant_pool.clone();
    module
        .functions
        .iter_mut()
        .map(|function| {
            let original = function.clone();
            let replaced = propagate_function_copies(function);
            if replaced == 0 {
                return 0;
            }
            let verification_module = MirModule {
                functions: vec![function.clone()],
                globals: globals.clone(),
                constant_pool: constant_pool.clone(),
            };
            if galfus_ir::validate_module(&verification_module).is_err() {
                *function = original;
                0
            } else {
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

fn dominators(function: &MirFunction) -> HashMap<BlockId, HashSet<BlockId>> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| block.id)
        .collect::<HashSet<_>>();
    let entry = function.blocks.first().map(|block| block.id);
    let mut predecessors = HashMap::<BlockId, Vec<BlockId>>::new();
    for block in &function.blocks {
        for successor in successors(&block.terminator.0) {
            predecessors.entry(successor).or_default().push(block.id);
        }
    }
    let mut result = function
        .blocks
        .iter()
        .map(|block| {
            let initial = if Some(block.id) == entry || !predecessors.contains_key(&block.id) {
                [block.id].into_iter().collect()
            } else {
                blocks.clone()
            };
            (block.id, initial)
        })
        .collect::<HashMap<_, HashSet<_>>>();
    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            if Some(block.id) == entry {
                continue;
            }
            let Some(preds) = predecessors.get(&block.id) else {
                continue;
            };
            let mut next = blocks.clone();
            for predecessor in preds {
                next.retain(|candidate| result[predecessor].contains(candidate));
            }
            next.insert(block.id);
            if result[&block.id] != next {
                result.insert(block.id, next);
                changed = true;
            }
        }
    }
    result
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
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
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
        let dominates = dominators
            .get(&block)
            .is_some_and(|set| set.contains(definition_block));
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
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
    replaced: &mut usize,
) {
    let mut replace = |operand: &mut Operand| {
        replace_operand_copy(operand, block, use_index, definitions, dominators, replaced);
    };
    match instruction {
        Instruction::Assign(_, rvalue) => replace_rvalue_operands(rvalue, &mut replace),
        Instruction::Drop(local) => {
            let mut operand = Operand::Local(*local);
            replace(&mut operand);
            if let Operand::Local(replacement) = operand {
                *local = replacement;
            }
        }
        Instruction::StoreGlobal(_, operand) => replace(operand),
        Instruction::StoreIndex { arr, idx, val } => {
            replace(arr);
            replace(idx);
            replace(val);
        }
        Instruction::StoreField { obj, val, .. } => {
            replace(obj);
            replace(val);
        }
        Instruction::Call { args, .. }
        | Instruction::AwaitAll { futures: args, .. }
        | Instruction::AwaitRace { futures: args, .. } => {
            for argument in args {
                replace(argument);
            }
        }
        Instruction::IndirectCall { func, args, .. } => {
            replace(func);
            for argument in args {
                replace(argument);
            }
        }
        Instruction::ConstraintCall { obj, args, .. } => {
            replace(obj);
            for argument in args {
                replace(argument);
            }
        }
        Instruction::Await { future, .. } => replace(future),
    }
}

fn replace_rvalue_operands(rvalue: &mut RValue, replace: &mut impl FnMut(&mut Operand)) {
    match rvalue {
        RValue::Use(operand)
        | RValue::UnaryOp(_, operand)
        | RValue::Cast(operand, _)
        | RValue::Copy(operand)
        | RValue::MemberAccess(operand, _)
        | RValue::ChoiceVariantIs(operand, _)
        | RValue::Instanceof(operand, _)
        | RValue::Len(operand) => replace(operand),
        RValue::BinaryOp(_, lhs, rhs) | RValue::ArrayIndex(lhs, rhs) => {
            replace(lhs);
            replace(rhs);
        }
        RValue::NewStruct { fields, .. }
        | RValue::NewArray(_, fields)
        | RValue::NewTuple(_, fields) => {
            for operand in fields {
                replace(operand);
            }
        }
        RValue::NewArrayDynamic(_, elements) => {
            for element in elements {
                match element {
                    galfus_ir::mir::ArrayLiteralElement::Single(operand)
                    | galfus_ir::mir::ArrayLiteralElement::Spread(operand) => replace(operand),
                }
            }
        }
        RValue::NewArrayZeroedDynamic { length, .. } => replace(length),
        RValue::Choice(_, _, Some(operand)) => replace(operand),
        RValue::CreateFuture { args, .. } => {
            for operand in args {
                replace(operand);
            }
        }
        RValue::CreateIndirectFuture { func, args } => {
            replace(func);
            for operand in args {
                replace(operand);
            }
        }
        RValue::NewArrayZeroed { .. } | RValue::LoadGlobal(_) | RValue::Choice(_, _, None) => {}
    }
}

fn replace_terminator_copies(
    terminator: &mut Terminator,
    block: BlockId,
    use_index: usize,
    definitions: &HashMap<LocalId, CopyDefinition>,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
    replaced: &mut usize,
) {
    let mut replace = |operand: &mut Operand| {
        replace_operand_copy(operand, block, use_index, definitions, dominators, replaced);
    };
    match terminator {
        Terminator::Return(Some(operand)) => replace(operand),
        Terminator::Jump { args, .. } | Terminator::TailCall { args, .. } => {
            for operand in args {
                replace(operand);
            }
        }
        Terminator::Branch {
            cond,
            true_args,
            false_args,
            ..
        } => {
            replace(cond);
            for operand in true_args.iter_mut().chain(false_args) {
                replace(operand);
            }
        }
        Terminator::Return(None) | Terminator::Panic(_) => {}
    }
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
    match instruction {
        Instruction::Assign(_, rvalue) => collect_rvalue_uses(rvalue, used),
        Instruction::Drop(local) => {
            used.insert(*local);
        }
        Instruction::StoreGlobal(_, operand) => collect_operand(operand, used),
        Instruction::StoreIndex { arr, idx, val } => {
            collect_operand(arr, used);
            collect_operand(idx, used);
            collect_operand(val, used);
        }
        Instruction::StoreField { obj, val, .. } => {
            collect_operand(obj, used);
            collect_operand(val, used);
        }
        Instruction::Call { args, .. } => {
            for operand in args {
                collect_operand(operand, used);
            }
        }
        Instruction::IndirectCall { func, args, .. } => {
            collect_operand(func, used);
            for operand in args {
                collect_operand(operand, used);
            }
        }
        Instruction::ConstraintCall { obj, args, .. } => {
            collect_operand(obj, used);
            for operand in args {
                collect_operand(operand, used);
            }
        }
        Instruction::Await { future, .. } => collect_operand(future, used),
        Instruction::AwaitAll { futures, .. } | Instruction::AwaitRace { futures, .. } => {
            for operand in futures {
                collect_operand(operand, used);
            }
        }
    }
}

fn collect_rvalue_uses(rvalue: &RValue, used: &mut HashSet<galfus_ir::mir::LocalId>) {
    match rvalue {
        RValue::Use(operand)
        | RValue::UnaryOp(_, operand)
        | RValue::Cast(operand, _)
        | RValue::Copy(operand)
        | RValue::MemberAccess(operand, _)
        | RValue::ChoiceVariantIs(operand, _)
        | RValue::Instanceof(operand, _)
        | RValue::Len(operand) => collect_operand(operand, used),
        RValue::BinaryOp(_, lhs, rhs) | RValue::ArrayIndex(lhs, rhs) => {
            collect_operand(lhs, used);
            collect_operand(rhs, used);
        }
        RValue::NewStruct { fields, .. }
        | RValue::NewArray(_, fields)
        | RValue::NewTuple(_, fields) => {
            for operand in fields {
                collect_operand(operand, used);
            }
        }
        RValue::NewArrayDynamic(_, elements) => {
            for element in elements {
                match element {
                    galfus_ir::mir::ArrayLiteralElement::Single(operand)
                    | galfus_ir::mir::ArrayLiteralElement::Spread(operand) => {
                        collect_operand(operand, used)
                    }
                }
            }
        }
        RValue::NewArrayZeroedDynamic { length, .. } => collect_operand(length, used),
        RValue::Choice(_, _, payload) => {
            if let Some(operand) = payload {
                collect_operand(operand, used);
            }
        }
        RValue::CreateFuture { args, .. } => {
            for operand in args {
                collect_operand(operand, used);
            }
        }
        RValue::CreateIndirectFuture { func, args } => {
            collect_operand(func, used);
            for operand in args {
                collect_operand(operand, used);
            }
        }
        RValue::NewArrayZeroed { .. } | RValue::LoadGlobal(_) => {}
    }
}
fn collect_terminator_uses(terminator: &Terminator, used: &mut HashSet<galfus_ir::mir::LocalId>) {
    match terminator {
        Terminator::Return(Some(operand)) => collect_operand(operand, used),
        Terminator::Jump { args, .. } => {
            for operand in args {
                collect_operand(operand, used);
            }
        }
        Terminator::Branch {
            cond,
            true_args,
            false_args,
            ..
        } => {
            collect_operand(cond, used);
            for operand in true_args.iter().chain(false_args) {
                collect_operand(operand, used);
            }
        }
        Terminator::TailCall { args, .. } => {
            for operand in args {
                collect_operand(operand, used);
            }
        }
        _ => {}
    }
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
