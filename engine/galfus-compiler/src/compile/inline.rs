use galfus_ir::mir::{
    ArrayLiteralElement, BasicBlock, BlockId, Instruction, LocalDecl, LocalId, MirFunction,
    MirModule, Operand, RValue, Terminator,
};
use std::collections::HashMap;

pub fn inline_functions(mir: &mut MirModule) {
    let mut candidates = HashMap::new();
    for func in &mir.functions {
        if !func.is_async && func.blocks.len() <= 5 && !has_calls_to_self(func) {
            // println!("Inliner: Candidate found {}", func.name);
            candidates.insert(func.id, func.clone());
        }
    }

    for i in 0..mir.functions.len() {
        if mir.functions[i].is_async {
            continue;
        }
        inline_in_function(&mut mir.functions[i], &candidates);
    }
}

fn has_calls_to_self(func: &MirFunction) -> bool {
    for block in &func.blocks {
        for (inst, _) in &block.instructions {
            if let Instruction::Call { func: target, .. } = inst
                && target == &func.id
            {
                return true;
            }
        }
    }
    false
}

fn inline_in_function(
    caller: &mut MirFunction,
    candidates: &HashMap<galfus_core::FunctionId, MirFunction>,
) {
    let mut changed = true;
    while changed {
        changed = false;

        let mut next_local_id = caller.locals.iter().map(|l| l.id.raw()).max().unwrap_or(0) + 1000;
        let mut next_block_id = caller.blocks.iter().map(|b| b.id.raw()).max().unwrap_or(0) + 1000;

        let mut new_blocks = Vec::new();
        let old_blocks = std::mem::take(&mut caller.blocks);

        for mut block in old_blocks {
            for i in 0..block.instructions.len() {
                if let Instruction::Call {
                    func: target_id,
                    args,
                    destination,
                    is_external: false,
                } = &block.instructions[i].0
                    && let Some(callee) = candidates.get(target_id)
                {
                    changed = true;
                    // println!("Inliner: Inlining {} into {}", callee.name, caller.name);

                    let mut part1_instrs = block.instructions[..i].to_vec();
                    let part2_instrs = block.instructions[i + 1..].to_vec();

                    let part2_id = BlockId::new(next_block_id);
                    next_block_id += 1;

                    let mut local_map = HashMap::new();
                    for p in callee.blocks.iter().flat_map(|b| &b.parameters) {
                        let new_id = LocalId::new(next_local_id);
                        next_local_id += 1;
                        local_map.insert(p.id, new_id);
                        caller.locals.push(LocalDecl {
                            id: new_id,
                            ty: p.ty,
                        });
                    }
                    for (idx, l) in callee.locals.iter().enumerate() {
                        let new_id = LocalId::new(next_local_id);
                        next_local_id += 1;
                        local_map.insert(l.id, new_id);
                        caller.locals.push(LocalDecl {
                            id: new_id,
                            ty: l.ty,
                        });

                        if idx < callee.parameter_types.len() {
                            part1_instrs.push((
                                Instruction::Assign(new_id, RValue::Use(args[idx].clone())),
                                None, // span could be block span or None
                            ));
                        }
                    }

                    let mut block_map = HashMap::new();
                    for b in &callee.blocks {
                        let new_id = BlockId::new(next_block_id);
                        next_block_id += 1;
                        block_map.insert(b.id, new_id);
                    }

                    let callee_entry = block_map[&callee.blocks[0].id];

                    new_blocks.push(BasicBlock {
                        id: block.id,
                        parameters: block.parameters.clone(),
                        instructions: part1_instrs,
                        terminator: (
                            Terminator::Jump {
                                target: callee_entry,
                                args: vec![],
                            },
                            None,
                        ),
                    });

                    for cb in &callee.blocks {
                        let mut mapped_instrs = Vec::new();
                        for (inst, span) in &cb.instructions {
                            mapped_instrs.push((map_inst(inst, &local_map), *span));
                        }

                        let mapped_term = match &cb.terminator.0 {
                            Terminator::Return(Some(op)) => {
                                mapped_instrs.push((
                                    Instruction::Assign(
                                        *destination,
                                        RValue::Use(map_op(op, &local_map)),
                                    ),
                                    cb.terminator.1,
                                ));
                                Terminator::Jump {
                                    target: part2_id,
                                    args: vec![],
                                }
                            }
                            Terminator::Return(None) => Terminator::Jump {
                                target: part2_id,
                                args: vec![],
                            },
                            t => map_term(t, &block_map, &local_map),
                        };

                        let mapped_params = cb
                            .parameters
                            .iter()
                            .map(|p| LocalDecl {
                                id: local_map[&p.id],
                                ty: p.ty,
                            })
                            .collect();

                        new_blocks.push(BasicBlock {
                            id: block_map[&cb.id],
                            parameters: mapped_params,
                            instructions: mapped_instrs,
                            terminator: (mapped_term, cb.terminator.1),
                        });
                    }

                    block.id = part2_id;
                    block.parameters = Vec::new();
                    block.instructions = part2_instrs;
                    break;
                }
            }

            new_blocks.push(block);
        }
        caller.blocks = new_blocks;
    }
}

fn map_op(op: &Operand, local_map: &HashMap<LocalId, LocalId>) -> Operand {
    match op {
        Operand::Local(l) => Operand::Local(*local_map.get(l).unwrap_or(l)),
        _ => op.clone(),
    }
}

fn map_arr_el(
    el: &ArrayLiteralElement,
    local_map: &HashMap<LocalId, LocalId>,
) -> ArrayLiteralElement {
    match el {
        ArrayLiteralElement::Single(op) => ArrayLiteralElement::Single(map_op(op, local_map)),
        ArrayLiteralElement::Spread(op) => ArrayLiteralElement::Spread(map_op(op, local_map)),
    }
}

fn map_rval(r: &RValue, local_map: &HashMap<LocalId, LocalId>) -> RValue {
    match r {
        RValue::Use(op) => RValue::Use(map_op(op, local_map)),
        RValue::UnaryOp(op, a) => RValue::UnaryOp(*op, map_op(a, local_map)),
        RValue::BinaryOp(op, a, b) => {
            RValue::BinaryOp(*op, map_op(a, local_map), map_op(b, local_map))
        }
        RValue::Cast(a, t) => RValue::Cast(map_op(a, local_map), *t),
        RValue::Copy(a) => RValue::Copy(map_op(a, local_map)),
        RValue::NewStruct {
            struct_type,
            fields,
        } => RValue::NewStruct {
            struct_type: *struct_type,
            fields: fields.iter().map(|f| map_op(f, local_map)).collect(),
        },
        RValue::NewArray(t, ops) => {
            RValue::NewArray(*t, ops.iter().map(|o| map_op(o, local_map)).collect())
        }
        RValue::NewArrayDynamic(t, els) => {
            RValue::NewArrayDynamic(*t, els.iter().map(|e| map_arr_el(e, local_map)).collect())
        }
        RValue::NewArrayZeroed {
            array_type,
            element_type,
            size,
        } => RValue::NewArrayZeroed {
            array_type: *array_type,
            element_type: *element_type,
            size: *size,
        },
        RValue::NewArrayZeroedDynamic {
            array_type,
            element_type,
            length,
        } => RValue::NewArrayZeroedDynamic {
            array_type: *array_type,
            element_type: *element_type,
            length: map_op(length, local_map),
        },
        RValue::NewTuple(t, ops) => {
            RValue::NewTuple(*t, ops.iter().map(|o| map_op(o, local_map)).collect())
        }
        RValue::MemberAccess(op, s) => RValue::MemberAccess(map_op(op, local_map), s.clone()),
        RValue::ArrayIndex(a, b) => RValue::ArrayIndex(map_op(a, local_map), map_op(b, local_map)),
        RValue::Choice(t, s, op) => {
            RValue::Choice(*t, s.clone(), op.as_ref().map(|o| map_op(o, local_map)))
        }
        RValue::ChoiceVariantIs(op, sym) => RValue::ChoiceVariantIs(map_op(op, local_map), *sym),
        RValue::Instanceof(op, t) => RValue::Instanceof(map_op(op, local_map), *t),
        RValue::LoadGlobal(s) => RValue::LoadGlobal(s.clone()),
        RValue::Len(op) => RValue::Len(map_op(op, local_map)),
        RValue::CreateFuture {
            func,
            args,
            is_external,
        } => RValue::CreateFuture {
            func: *func,
            args: args.iter().map(|a| map_op(a, local_map)).collect(),
            is_external: *is_external,
        },
        RValue::CreateIndirectFuture { func, args } => RValue::CreateIndirectFuture {
            func: map_op(func, local_map),
            args: args.iter().map(|a| map_op(a, local_map)).collect(),
        },
    }
}

fn map_inst(inst: &Instruction, local_map: &HashMap<LocalId, LocalId>) -> Instruction {
    match inst {
        Instruction::Assign(l, r) => {
            Instruction::Assign(*local_map.get(l).unwrap_or(l), map_rval(r, local_map))
        }
        Instruction::Drop(l) => Instruction::Drop(*local_map.get(l).unwrap_or(l)),
        Instruction::StoreGlobal(s, op) => {
            Instruction::StoreGlobal(s.clone(), map_op(op, local_map))
        }
        Instruction::StoreIndex { arr, idx, val } => Instruction::StoreIndex {
            arr: map_op(arr, local_map),
            idx: map_op(idx, local_map),
            val: map_op(val, local_map),
        },
        Instruction::StoreField {
            obj,
            field_name,
            val,
        } => Instruction::StoreField {
            obj: map_op(obj, local_map),
            field_name: field_name.clone(),
            val: map_op(val, local_map),
        },
        Instruction::Call {
            func,
            args,
            destination,
            is_external,
        } => Instruction::Call {
            func: *func,
            args: args.iter().map(|a| map_op(a, local_map)).collect(),
            destination: *local_map.get(destination).unwrap_or(destination),
            is_external: *is_external,
        },
        Instruction::IndirectCall {
            func,
            args,
            destination,
        } => Instruction::IndirectCall {
            func: map_op(func, local_map),
            args: args.iter().map(|a| map_op(a, local_map)).collect(),
            destination: *local_map.get(destination).unwrap_or(destination),
        },
        Instruction::ConstraintCall {
            method_name,
            obj,
            args,
            destination,
        } => Instruction::ConstraintCall {
            method_name: method_name.clone(),
            obj: map_op(obj, local_map),
            args: args.iter().map(|a| map_op(a, local_map)).collect(),
            destination: *local_map.get(destination).unwrap_or(destination),
        },
        Instruction::Await {
            future,
            destination,
        } => Instruction::Await {
            future: map_op(future, local_map),
            destination: *local_map.get(destination).unwrap_or(destination),
        },
        Instruction::AwaitAll {
            futures,
            destination,
        } => Instruction::AwaitAll {
            futures: futures.iter().map(|a| map_op(a, local_map)).collect(),
            destination: *local_map.get(destination).unwrap_or(destination),
        },
        Instruction::AwaitRace {
            futures,
            destination,
        } => Instruction::AwaitRace {
            futures: futures.iter().map(|a| map_op(a, local_map)).collect(),
            destination: *local_map.get(destination).unwrap_or(destination),
        },
    }
}

fn map_term(
    term: &Terminator,
    block_map: &HashMap<BlockId, BlockId>,
    local_map: &HashMap<LocalId, LocalId>,
) -> Terminator {
    match term {
        Terminator::Return(op) => Terminator::Return(op.as_ref().map(|o| map_op(o, local_map))),
        Terminator::Jump { target, args } => Terminator::Jump {
            target: *block_map.get(target).unwrap_or(target),
            args: args.iter().map(|a| map_op(a, local_map)).collect(),
        },
        Terminator::Branch {
            cond,
            true_block,
            true_args,
            false_block,
            false_args,
        } => Terminator::Branch {
            cond: map_op(cond, local_map),
            true_block: *block_map.get(true_block).unwrap_or(true_block),
            true_args: true_args.iter().map(|a| map_op(a, local_map)).collect(),
            false_block: *block_map.get(false_block).unwrap_or(false_block),
            false_args: false_args.iter().map(|a| map_op(a, local_map)).collect(),
        },
        Terminator::Panic(s) => Terminator::Panic(s.clone()),
        Terminator::TailCall {
            func,
            args,
            is_external,
        } => Terminator::TailCall {
            func: *func,
            args: args.iter().map(|a| map_op(a, local_map)).collect(),
            is_external: *is_external,
        },
    }
}

pub fn optimize_mir(mir: &mut MirModule) {
    for func in &mut mir.functions {
        loop {
            let mut replacements = HashMap::new();

            // Find simple copies: local1 = local2
            for block in &func.blocks {
                for (inst, _) in &block.instructions {
                    if let Instruction::Assign(dest, RValue::Use(Operand::Local(src))) = inst
                        && dest != src
                    {
                        replacements.insert(*dest, *src);
                    }
                }
            }

            if replacements.is_empty() {
                break;
            }

            // Replace all usages of `dest` with `src`
            for block in &mut func.blocks {
                let mut new_instrs = Vec::new();
                for (inst, span) in &block.instructions {
                    if let Instruction::Assign(dest, RValue::Use(Operand::Local(src))) = inst
                        && dest != src
                        && replacements.get(dest) == Some(src)
                    {
                        // Remove the copy instruction entirely!
                        continue;
                    }
                    new_instrs.push((map_inst(inst, &replacements), *span));
                }
                block.instructions = new_instrs;

                // Map terminator
                let block_map = HashMap::new();
                block.terminator.0 = map_term(&block.terminator.0, &block_map, &replacements);
            }
        }
    }
}
