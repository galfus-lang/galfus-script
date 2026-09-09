use galfus_ir::mir::{Constant, Instruction, LocalId, MirBinaryOp, MirModule, Operand, RValue};
use std::collections::HashMap;

pub(super) fn propagate_and_fold_constants(module: &mut MirModule) -> usize {
    let mut changed = 0;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            let mut constants = HashMap::<LocalId, Constant>::new();
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
