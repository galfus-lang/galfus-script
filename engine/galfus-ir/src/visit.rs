//! Traversal helpers for MIR operands.
//!
//! These helpers visit value uses only. Instruction destinations and control
//! flow targets are intentionally excluded because they are definitions and
//! graph edges, not operand reads.

use crate::mir::{ArrayLiteralElement, Instruction, Operand, RValue, Terminator};

pub fn for_each_rvalue_operand(rvalue: &RValue, mut visit: impl FnMut(&Operand)) {
    match rvalue {
        RValue::Use(operand)
        | RValue::UnaryOp(_, operand)
        | RValue::Cast(operand, _)
        | RValue::Copy(operand)
        | RValue::MemberAccess(operand, _)
        | RValue::ChoiceVariantIs(operand, _)
        | RValue::ImportedChoiceVariantIs(operand, _, _)
        | RValue::Instanceof(operand, _)
        | RValue::Len(operand) => visit(operand),
        RValue::BinaryOp(_, left, right) | RValue::ArrayIndex(left, right) => {
            visit(left);
            visit(right);
        }
        RValue::NewStruct { fields, .. }
        | RValue::NewArray(_, fields)
        | RValue::NewTuple(_, fields) => {
            for operand in fields {
                visit(operand);
            }
        }
        RValue::NewArrayDynamic(_, elements) => {
            for element in elements {
                match element {
                    ArrayLiteralElement::Single(operand) | ArrayLiteralElement::Spread(operand) => {
                        visit(operand);
                    }
                }
            }
        }
        RValue::NewArrayZeroedDynamic { length, .. } => visit(length),
        RValue::Choice(_, _, Some(operand)) => visit(operand),
        RValue::CreateFuture { args, .. } => {
            for operand in args {
                visit(operand);
            }
        }
        RValue::CreateIndirectFuture { func, args } => {
            visit(func);
            for operand in args {
                visit(operand);
            }
        }
        RValue::NewArrayZeroed { .. } | RValue::LoadGlobal(_) | RValue::Choice(_, _, None) => {}
    }
}

pub fn for_each_rvalue_operand_mut(rvalue: &mut RValue, mut visit: impl FnMut(&mut Operand)) {
    match rvalue {
        RValue::Use(operand)
        | RValue::UnaryOp(_, operand)
        | RValue::Cast(operand, _)
        | RValue::Copy(operand)
        | RValue::MemberAccess(operand, _)
        | RValue::ChoiceVariantIs(operand, _)
        | RValue::ImportedChoiceVariantIs(operand, _, _)
        | RValue::Instanceof(operand, _)
        | RValue::Len(operand) => visit(operand),
        RValue::BinaryOp(_, left, right) | RValue::ArrayIndex(left, right) => {
            visit(left);
            visit(right);
        }
        RValue::NewStruct { fields, .. }
        | RValue::NewArray(_, fields)
        | RValue::NewTuple(_, fields) => {
            for operand in fields {
                visit(operand);
            }
        }
        RValue::NewArrayDynamic(_, elements) => {
            for element in elements {
                match element {
                    ArrayLiteralElement::Single(operand) | ArrayLiteralElement::Spread(operand) => {
                        visit(operand);
                    }
                }
            }
        }
        RValue::NewArrayZeroedDynamic { length, .. } => visit(length),
        RValue::Choice(_, _, Some(operand)) => visit(operand),
        RValue::CreateFuture { args, .. } => {
            for operand in args {
                visit(operand);
            }
        }
        RValue::CreateIndirectFuture { func, args } => {
            visit(func);
            for operand in args {
                visit(operand);
            }
        }
        RValue::NewArrayZeroed { .. } | RValue::LoadGlobal(_) | RValue::Choice(_, _, None) => {}
    }
}

pub fn for_each_instruction_operand(instruction: &Instruction, mut visit: impl FnMut(&Operand)) {
    match instruction {
        Instruction::Assign(_, rvalue) => for_each_rvalue_operand(rvalue, visit),
        Instruction::StoreGlobal(_, operand) => visit(operand),
        Instruction::StoreIndex { arr, idx, val } => {
            visit(arr);
            visit(idx);
            visit(val);
        }
        Instruction::StoreField { obj, val, .. } => {
            visit(obj);
            visit(val);
        }
        Instruction::Call { args, .. }
        | Instruction::AwaitAll { futures: args, .. }
        | Instruction::AwaitRace { futures: args, .. } => {
            for operand in args {
                visit(operand);
            }
        }
        Instruction::IndirectCall { func, args, .. } => {
            visit(func);
            for operand in args {
                visit(operand);
            }
        }
        Instruction::ConstraintCall { obj, args, .. } => {
            visit(obj);
            for operand in args {
                visit(operand);
            }
        }
        Instruction::Await { future, .. } => visit(future),
        Instruction::Drop(_) => {}
    }
}

pub fn for_each_instruction_operand_mut(
    instruction: &mut Instruction,
    mut visit: impl FnMut(&mut Operand),
) {
    match instruction {
        Instruction::Assign(_, rvalue) => for_each_rvalue_operand_mut(rvalue, visit),
        Instruction::StoreGlobal(_, operand) => visit(operand),
        Instruction::StoreIndex { arr, idx, val } => {
            visit(arr);
            visit(idx);
            visit(val);
        }
        Instruction::StoreField { obj, val, .. } => {
            visit(obj);
            visit(val);
        }
        Instruction::Call { args, .. }
        | Instruction::AwaitAll { futures: args, .. }
        | Instruction::AwaitRace { futures: args, .. } => {
            for operand in args {
                visit(operand);
            }
        }
        Instruction::IndirectCall { func, args, .. } => {
            visit(func);
            for operand in args {
                visit(operand);
            }
        }
        Instruction::ConstraintCall { obj, args, .. } => {
            visit(obj);
            for operand in args {
                visit(operand);
            }
        }
        Instruction::Await { future, .. } => visit(future),
        Instruction::Drop(_) => {}
    }
}

pub fn for_each_terminator_operand(terminator: &Terminator, mut visit: impl FnMut(&Operand)) {
    match terminator {
        Terminator::Return(Some(operand)) => visit(operand),
        Terminator::Jump { args, .. } | Terminator::TailCall { args, .. } => {
            for operand in args {
                visit(operand);
            }
        }
        Terminator::Branch {
            cond,
            true_args,
            false_args,
            ..
        } => {
            visit(cond);
            for operand in true_args.iter().chain(false_args) {
                visit(operand);
            }
        }
        Terminator::Return(None) | Terminator::Panic(_) => {}
    }
}

pub fn for_each_terminator_operand_mut(
    terminator: &mut Terminator,
    mut visit: impl FnMut(&mut Operand),
) {
    match terminator {
        Terminator::Return(Some(operand)) => visit(operand),
        Terminator::Jump { args, .. } | Terminator::TailCall { args, .. } => {
            for operand in args {
                visit(operand);
            }
        }
        Terminator::Branch {
            cond,
            true_args,
            false_args,
            ..
        } => {
            visit(cond);
            for operand in true_args.iter_mut().chain(false_args) {
                visit(operand);
            }
        }
        Terminator::Return(None) | Terminator::Panic(_) => {}
    }
}
