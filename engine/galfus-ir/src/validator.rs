use crate::mir;

use crate::LocalId;
use crate::mir::*;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

pub fn validate_module(module: &MirModule) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    for func in &module.functions {
        if let Err(mut func_errors) = validate_function(func) {
            errors.append(&mut func_errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_function(func: &MirFunction) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let mut blocks = HashMap::new();
    for block in &func.blocks {
        if blocks.insert(block.id, block).is_some() {
            errors.push(ValidationError {
                message: format!(
                    "Function '{}': Duplicate basic block ID {:?}",
                    func.name, block.id
                ),
            });
        }
    }

    if !blocks.contains_key(&BlockId::new(0)) {
        errors.push(ValidationError {
            message: format!("Function '{}': Missing entry block", func.name),
        });
        return Err(errors);
    }

    for block in &func.blocks {
        for (target, _) in successor_blocks(&block.terminator.0) {
            if !blocks.contains_key(&target) {
                errors.push(ValidationError {
                    message: format!(
                        "Function '{}': Block {:?} jumps to undeclared block {:?}",
                        func.name, block.id, target
                    ),
                });
            }
        }
    }

    let mut assigned_locals = HashSet::new();
    for i in 0..func.parameter_types.len() {
        assigned_locals.insert(LocalId::new(i as u32));
    }

    for block in &func.blocks {
        for param in &block.parameters {
            if !assigned_locals.insert(param.id) {
                errors.push(ValidationError {
                    message: format!(
                        "SSA Violation: Local {:?} assigned multiple times (block parameter)",
                        param.id
                    ),
                });
            }
        }
        for inst in &block.instructions {
            match &inst.0 {
                mir::Instruction::Assign(dest, _)
                | mir::Instruction::Call {
                    destination: dest, ..
                }
                | mir::Instruction::IndirectCall {
                    destination: dest, ..
                }
                | mir::Instruction::ConstraintCall {
                    destination: dest, ..
                }
                | mir::Instruction::Await {
                    destination: dest, ..
                }
                | mir::Instruction::AwaitAll {
                    destination: dest, ..
                }
                | mir::Instruction::AwaitRace {
                    destination: dest, ..
                } if !assigned_locals.insert(*dest) => {
                    errors.push(ValidationError {
                        message: format!("SSA Violation: Local {:?} assigned multiple times", dest),
                    });
                }
                _ => {}
            }
        }

        match &block.terminator.0 {
            mir::Terminator::Jump { target, args } => {
                if let Some(target_block) = blocks.get(target)
                    && args.len() != target_block.parameters.len()
                {
                    errors.push(ValidationError {
                        message: "Jump arguments length mismatch".to_string(),
                    });
                }
            }
            mir::Terminator::Branch {
                true_block,
                true_args,
                false_block,
                false_args,
                ..
            } => {
                if let Some(tb) = blocks.get(true_block)
                    && true_args.len() != tb.parameters.len()
                {
                    errors.push(ValidationError {
                        message: "Branch true arguments mismatch".to_string(),
                    });
                }
                if let Some(fb) = blocks.get(false_block)
                    && false_args.len() != fb.parameters.len()
                {
                    errors.push(ValidationError {
                        message: "Branch false arguments mismatch".to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    let mut entry_initialized = HashSet::new();
    for idx in 0..func.parameter_types.len() {
        entry_initialized.insert(LocalId::new(idx as u32));
    }

    let initialized_at_entry = initialized_at_block_entries(&blocks, entry_initialized);
    let all_declared = func
        .locals
        .iter()
        .map(|decl| decl.id)
        .collect::<HashSet<_>>();

    for bb in &func.blocks {
        let mut initialized = initialized_at_entry
            .get(&bb.id)
            .cloned()
            .unwrap_or_else(|| all_declared.clone());

        for param in &bb.parameters {
            initialized.insert(param.id);
        }

        validate_basic_block(bb, func, &mut initialized, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn initialized_at_block_entries(
    blocks: &HashMap<BlockId, &BasicBlock>,
    entry_initialized: HashSet<LocalId>,
) -> HashMap<BlockId, HashSet<LocalId>> {
    let entry = BlockId::new(0);
    let mut initialized = HashMap::from([(entry, entry_initialized)]);
    let mut pending = VecDeque::from([entry]);

    while let Some(block_id) = pending.pop_front() {
        let Some(block) = blocks.get(&block_id) else {
            continue;
        };
        let mut outgoing = initialized[&block_id].clone();
        apply_initialization_effects(block, &mut outgoing);

        for (successor, args) in successor_blocks(&block.terminator.0) {
            if !blocks.contains_key(&successor) {
                continue;
            }
            let mut edge_outgoing = outgoing.clone();
            for arg in args {
                if let Operand::Local(l) = arg {
                    edge_outgoing.remove(l);
                }
            }
            let changed = match initialized.get(&successor) {
                Some(previous) => {
                    let merged = previous
                        .intersection(&edge_outgoing)
                        .copied()
                        .collect::<HashSet<_>>();
                    if &merged == previous {
                        false
                    } else {
                        initialized.insert(successor, merged);
                        true
                    }
                }
                None => {
                    initialized.insert(successor, edge_outgoing);
                    true
                }
            };
            if changed {
                pending.push_back(successor);
            }
        }
    }

    initialized
}

fn apply_initialization_effects(block: &BasicBlock, initialized: &mut HashSet<LocalId>) {
    for param in &block.parameters {
        initialized.insert(param.id);
    }
    for instruction in &block.instructions {
        match &instruction.0 {
            mir::Instruction::Assign(_, RValue::Use(Operand::Local(l))) => {
                initialized.remove(l);
            }
            mir::Instruction::Call { args, .. }
            | mir::Instruction::ConstraintCall { args, .. }
            | mir::Instruction::IndirectCall { args, .. } => {
                for arg in args {
                    if let Operand::Local(l) = arg {
                        initialized.remove(l);
                    }
                }
            }
            mir::Instruction::StoreGlobal(_, Operand::Local(l)) => {
                initialized.remove(l);
            }
            mir::Instruction::StoreIndex {
                val: Operand::Local(l),
                ..
            } => {
                initialized.remove(l);
            }
            mir::Instruction::StoreField {
                val: Operand::Local(l),
                ..
            } => {
                initialized.remove(l);
            }
            mir::Instruction::Drop(local) => {
                initialized.remove(local);
            }
            _ => {}
        }

        match &instruction.0 {
            mir::Instruction::Assign(destination, _)
            | mir::Instruction::Call { destination, .. }
            | mir::Instruction::ConstraintCall { destination, .. }
            | mir::Instruction::IndirectCall { destination, .. } => {
                initialized.insert(*destination);
            }
            _ => {}
        }
    }

    match &block.terminator.0 {
        mir::Terminator::Return(Some(Operand::Local(l))) => {
            initialized.remove(l);
        }
        mir::Terminator::Branch {
            cond: Operand::Local(l),
            ..
        } => {
            initialized.remove(l);
        }
        _ => {}
    }
}

fn successor_blocks(terminator: &mir::Terminator) -> Vec<(BlockId, &Vec<Operand>)> {
    match terminator {
        mir::Terminator::Jump { target, args } => vec![(*target, args)],
        mir::Terminator::Branch {
            true_block,
            true_args,
            false_block,
            false_args,
            ..
        } => vec![(*true_block, true_args), (*false_block, false_args)],
        mir::Terminator::Return(_) | mir::Terminator::Panic(_) => Vec::new(),
    }
}

fn validate_basic_block(
    bb: &BasicBlock,
    func: &MirFunction,
    initialized: &mut HashSet<LocalId>,
    errors: &mut Vec<ValidationError>,
) {
    for inst in &bb.instructions {
        match &inst.0 {
            mir::Instruction::Assign(dest, rvalue) => {
                if !func.locals.iter().any(|decl| decl.id == *dest) {
                    errors.push(ValidationError {
                        message: format!(
                            "Function '{}': Assignment to undeclared local ID {:?}",
                            func.name, dest
                        ),
                    });
                }
                validate_rvalue_operands(rvalue, func, initialized, errors);
                initialized.insert(*dest);
            }
            mir::Instruction::Drop(local) => {
                if !func.locals.iter().any(|decl| decl.id == *local) {
                    errors.push(ValidationError {
                        message: format!(
                            "Function '{}': Drop of undeclared local ID {:?}",
                            func.name, local
                        ),
                    });
                }
                initialized.remove(local);
            }
            mir::Instruction::StoreGlobal(_, val) => {
                validate_operand(val, func, initialized, errors);
            }
            mir::Instruction::StoreIndex { arr, idx, val } => {
                validate_operand(arr, func, initialized, errors);
                validate_operand(idx, func, initialized, errors);
                validate_operand(val, func, initialized, errors);
            }
            mir::Instruction::StoreField { obj, val, .. } => {
                validate_operand(obj, func, initialized, errors);
                validate_operand(val, func, initialized, errors);
            }

            mir::Instruction::Call {
                func: _,
                args,
                destination,
                is_external: _,
            } => {
                for arg in args {
                    validate_operand(arg, func, initialized, errors);
                }
                if !func.locals.iter().any(|decl| decl.id == *destination) {
                    errors.push(ValidationError {
                        message: format!(
                            "Function '{}': Call assigned to undeclared local ID {:?}",
                            func.name, destination
                        ),
                    });
                }
                initialized.insert(*destination);
            }
            mir::Instruction::ConstraintCall {
                method_name: _,
                obj,
                args,
                destination,
            } => {
                validate_operand(obj, func, initialized, errors);
                for arg in args {
                    validate_operand(arg, func, initialized, errors);
                }
                if !func.locals.iter().any(|decl| decl.id == *destination) {
                    errors.push(ValidationError {
                        message: format!(
                            "Function '{}': Constraint call assigned to undeclared local ID {:?}",
                            func.name, destination
                        ),
                    });
                }
                initialized.insert(*destination);
            }
            mir::Instruction::IndirectCall {
                func: func_op,
                args,
                destination,
            } => {
                validate_operand(func_op, func, initialized, errors);
                for arg in args {
                    validate_operand(arg, func, initialized, errors);
                }
                if !func.locals.iter().any(|decl| decl.id == *destination) {
                    errors.push(ValidationError {
                        message: format!(
                            "Function '{}': Indirect call assigned to undeclared local ID {:?}",
                            func.name, destination
                        ),
                    });
                }
                initialized.insert(*destination);
            }
            mir::Instruction::Await {
                future,
                destination,
            } => {
                validate_operand(future, func, initialized, errors);
                if !func.locals.iter().any(|decl| decl.id == *destination) {
                    errors.push(ValidationError {
                        message: format!(
                            "Function '{}': Await assigned to undeclared local ID {:?}",
                            func.name, destination
                        ),
                    });
                }
                initialized.insert(*destination);
            }
            mir::Instruction::AwaitAll {
                futures,
                destination,
            }
            | mir::Instruction::AwaitRace {
                futures,
                destination,
            } => {
                for fut in futures {
                    validate_operand(fut, func, initialized, errors);
                }
                if !func.locals.iter().any(|decl| decl.id == *destination) {
                    errors.push(ValidationError {
                        message: format!(
                            "Function '{}': Await tuple assigned to undeclared local ID {:?}",
                            func.name, destination
                        ),
                    });
                }
                initialized.insert(*destination);
            }
        }
    }

    match &bb.terminator.0 {
        mir::Terminator::Return(Some(op)) => {
            validate_operand(op, func, initialized, errors);
        }
        mir::Terminator::Return(None) => {}
        mir::Terminator::Jump { target: _, args: _ } => {}
        mir::Terminator::Branch { cond, .. } => {
            validate_operand(cond, func, initialized, errors);
        }
        mir::Terminator::Panic(_) => {}
    }
}

fn validate_rvalue_operands(
    rvalue: &RValue,
    func: &MirFunction,
    initialized: &HashSet<LocalId>,
    errors: &mut Vec<ValidationError>,
) {
    match rvalue {
        RValue::Use(operand)
        | RValue::UnaryOp(_, operand)
        | RValue::Len(operand)
        | RValue::Copy(operand)
        | RValue::MemberAccess(operand, _)
        | RValue::ChoiceVariantIs(operand, _)
        | RValue::Instanceof(operand, _)
        | RValue::Cast(operand, _) => {
            validate_operand(operand, func, initialized, errors);
        }
        RValue::BinaryOp(_, lhs, rhs) | RValue::ArrayIndex(lhs, rhs) => {
            validate_operand(lhs, func, initialized, errors);
            validate_operand(rhs, func, initialized, errors);
        }
        RValue::Choice(_, _, op) => {
            if let Some(op) = op {
                validate_operand(op, func, initialized, errors);
            }
        }
        RValue::NewStruct { fields, .. }
        | RValue::NewArray(_, fields)
        | RValue::NewTuple(_, fields) => {
            for op in fields {
                validate_operand(op, func, initialized, errors);
            }
        }
        RValue::NewArrayDynamic(_, elements) => {
            for elem in elements {
                match elem {
                    ArrayLiteralElement::Single(op) | ArrayLiteralElement::Spread(op) => {
                        validate_operand(op, func, initialized, errors);
                    }
                }
            }
        }
        RValue::NewArrayZeroed { .. } | RValue::LoadGlobal(_) => {}
        RValue::NewArrayZeroedDynamic { length, .. } => {
            validate_operand(length, func, initialized, errors);
        }
        RValue::CreateFuture { args, .. } => {
            for arg in args {
                validate_operand(arg, func, initialized, errors);
            }
        }
        RValue::CreateIndirectFuture {
            func: func_op,
            args,
        } => {
            validate_operand(func_op, func, initialized, errors);
            for arg in args {
                validate_operand(arg, func, initialized, errors);
            }
        }
    }
}

fn validate_operand(
    op: &Operand,
    func: &MirFunction,
    initialized: &HashSet<LocalId>,
    errors: &mut Vec<ValidationError>,
) {
    if let Operand::Local(local_id) = op {
        if !func.locals.iter().any(|decl| decl.id == *local_id) {
            errors.push(ValidationError {
                message: format!(
                    "Function '{}': Use of undeclared local ID {:?}",
                    func.name, local_id
                ),
            });
        }
        if !initialized.contains(local_id) {
            errors.push(ValidationError {
                message: format!(
                    "Function '{}': Use of uninitialized local ID {:?}",
                    func.name, local_id
                ),
            });
        }
    }
}
