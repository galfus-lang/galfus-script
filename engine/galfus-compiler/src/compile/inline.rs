use std::collections::HashMap;

use galfus_ir::mir::{
    Instruction, LocalDecl, LocalId, MirFunction, MirModule, Operand, RValue, Terminator,
};

const MAX_INLINE_INSTRUCTIONS: usize = 8;
const MAX_INLINE_LOCALS: usize = 16;

/// Inline only leaf functions whose complete body is small and synchronous.
///
/// Keeping this deliberately narrow makes the transform independent from call
/// graph ordering and prevents it from crossing async, provider, adapter, or
/// dynamic-call boundaries.
pub fn inline_functions(module: &mut MirModule, max_function_instructions: usize) -> usize {
    let candidates = module
        .functions
        .iter()
        .filter(|function| is_inline_candidate(function))
        .map(|function| (function.id, function.clone()))
        .collect::<HashMap<_, _>>();

    let mut inlined_calls = 0;
    for caller in &mut module.functions {
        if caller.is_async {
            continue;
        }
        inlined_calls += inline_in_function(caller, &candidates, max_function_instructions);
    }
    inlined_calls
}

fn is_inline_candidate(function: &MirFunction) -> bool {
    if function.is_async
        || function.blocks.len() != 1
        || function.locals.len() > MAX_INLINE_LOCALS
        || function.blocks[0].instructions.len() > MAX_INLINE_INSTRUCTIONS
        || !function.blocks[0].parameters.is_empty()
        || !matches!(function.blocks[0].terminator.0, Terminator::Return(Some(_)))
    {
        return false;
    }

    function.blocks[0]
        .instructions
        .iter()
        .all(|(instruction, _)| {
            matches!(
                instruction,
                Instruction::Assign(
                    _,
                    RValue::Use(_)
                        | RValue::UnaryOp(_, _)
                        | RValue::BinaryOp(_, _, _)
                        | RValue::Cast(_, _)
                        | RValue::Copy(_)
                )
            )
        })
}

fn inline_in_function(
    caller: &mut MirFunction,
    candidates: &HashMap<galfus_core::FunctionId, MirFunction>,
    max_function_instructions: usize,
) -> usize {
    let mut next_local_id = caller
        .locals
        .iter()
        .map(|local| local.id.raw())
        .max()
        .unwrap_or(0)
        + 1;
    let mut inlined_calls = 0;
    let mut function_instructions = caller
        .blocks
        .iter()
        .map(|block| block.instructions.len())
        .sum::<usize>();

    for block in &mut caller.blocks {
        let mut instructions = Vec::with_capacity(block.instructions.len());
        for (instruction, span) in std::mem::take(&mut block.instructions) {
            let Instruction::Call {
                func,
                args,
                destination,
                is_external: false,
            } = &instruction
            else {
                instructions.push((instruction, span));
                continue;
            };
            let Some(callee) = candidates.get(func) else {
                instructions.push((instruction, span));
                continue;
            };
            let callee_block = &callee.blocks[0];
            if args.len() != callee.parameter_types.len() {
                instructions.push((instruction, span));
                continue;
            }
            let added_instructions = args.len() + callee_block.instructions.len();
            if function_instructions + added_instructions > max_function_instructions {
                instructions.push((instruction, span));
                continue;
            }

            let mut local_map = HashMap::new();
            for local in &callee.locals {
                let replacement = LocalId::new(next_local_id);
                next_local_id += 1;
                local_map.insert(local.id, replacement);
                caller.locals.push(LocalDecl {
                    id: replacement,
                    ty: local.ty,
                    is_owned: local.is_owned,
                });
            }

            for (parameter, argument) in callee.locals.iter().zip(args) {
                instructions.push((
                    Instruction::Assign(local_map[&parameter.id], RValue::Use(argument.clone())),
                    span,
                ));
            }
            instructions.extend(callee_block.instructions.iter().map(
                |(instruction, instruction_span)| {
                    (map_instruction(instruction, &local_map), *instruction_span)
                },
            ));
            let Terminator::Return(Some(value)) = &callee_block.terminator.0 else {
                unreachable!("inline candidate must return a value");
            };
            instructions.push((
                Instruction::Assign(*destination, RValue::Use(map_operand(value, &local_map))),
                callee_block.terminator.1,
            ));
            function_instructions += added_instructions;
            inlined_calls += 1;
        }
        block.instructions = instructions;
    }

    inlined_calls
}

fn map_operand(operand: &Operand, local_map: &HashMap<LocalId, LocalId>) -> Operand {
    match operand {
        Operand::Local(local) => Operand::Local(*local_map.get(local).unwrap_or(local)),
        _ => operand.clone(),
    }
}

fn map_rvalue(rvalue: &RValue, local_map: &HashMap<LocalId, LocalId>) -> RValue {
    match rvalue {
        RValue::Use(operand) => RValue::Use(map_operand(operand, local_map)),
        RValue::UnaryOp(operation, operand) => {
            RValue::UnaryOp(*operation, map_operand(operand, local_map))
        }
        RValue::BinaryOp(operation, lhs, rhs) => RValue::BinaryOp(
            *operation,
            map_operand(lhs, local_map),
            map_operand(rhs, local_map),
        ),
        RValue::Cast(operand, ty) => RValue::Cast(map_operand(operand, local_map), *ty),
        RValue::Copy(operand) => RValue::Copy(map_operand(operand, local_map)),
        _ => unreachable!("inline candidates contain only pure primitive rvalues"),
    }
}

fn map_instruction(
    instruction: &Instruction,
    local_map: &HashMap<LocalId, LocalId>,
) -> Instruction {
    match instruction {
        Instruction::Assign(local, rvalue) => Instruction::Assign(
            *local_map.get(local).unwrap_or(local),
            map_rvalue(rvalue, local_map),
        ),
        _ => unreachable!("inline candidates contain only assignments"),
    }
}
