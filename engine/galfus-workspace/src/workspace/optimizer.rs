use galfus_bytecode::{BytecodeFunction, BytecodeGraphTransaction, Instruction, PackageImage};
use std::collections::HashSet;

use galfus_core::SemanticRevision;

pub fn optimize_package(
    package: &PackageImage,
    semantic_revision: SemanticRevision,
) -> Result<PackageImage, String> {
    let graph = package.graph();
    let mut new_modules = Vec::new();

    for node in graph.modules() {
        let mut new_node = node.clone();
        for func in &mut new_node.module.functions {
            optimize_function(func);
        }
        new_modules.push(new_node);
    }

    let transaction = BytecodeGraphTransaction {
        base_version: graph.version(),
        semantic_revision,
        upserted_modules: new_modules,
        removed_modules: vec![],
        edges: graph.edges().to_vec(),
    };

    let new_graph = graph.apply(transaction).map_err(|e| e.to_string())?;

    PackageImage::try_new(
        new_graph,
        package.target().clone(),
        package.entry_point().cloned(),
        package.metadata().clone(),
        package.limits().clone(),
        package.adapter_requirements().to_vec(),
        package.provider_requirements().to_vec(),
    )
    .map_err(|e| e.to_string())
}

fn optimize_function(func: &mut BytecodeFunction) {
    let mut changed = true;
    while changed {
        changed = false;

        let mut jump_targets = HashSet::new();
        for (i, inst) in func.instructions.iter().enumerate() {
            match inst {
                Instruction::Jump { offset }
                | Instruction::JumpTrue { offset, .. }
                | Instruction::JumpFalse { offset, .. }
                | Instruction::JumpNull { offset, .. } => {
                    let target = (i as i32 + 1 + offset) as usize;
                    jump_targets.insert(target);
                }
                _ => {}
            }
        }

        let mut to_remove = vec![false; func.instructions.len()];
        let mut dead = false;

        for (i, inst) in func.instructions.iter().enumerate() {
            if jump_targets.contains(&i) {
                dead = false;
            }
            if dead {
                to_remove[i] = true;
                changed = true;
                continue;
            }

            if let Instruction::Move { dest, src } = inst {
                if dest == src {
                    to_remove[i] = true;
                    changed = true;
                    continue;
                }
            }

            match inst {
                Instruction::Jump { .. }
                | Instruction::Ret { .. }
                | Instruction::RetNull
                | Instruction::Panic { .. } => {
                    dead = true;
                }
                _ => {}
            }
        }

        if !changed {
            break;
        }

        let mut new_instructions = Vec::with_capacity(func.instructions.len());
        let mut old_to_new = vec![0; func.instructions.len() + 1];
        
        let mut new_idx = 0;
        for (old_idx, &remove) in to_remove.iter().enumerate() {
            if !remove {
                old_to_new[old_idx] = new_idx;
                new_idx += 1;
            } else {
                old_to_new[old_idx] = new_idx;
            }
        }
        old_to_new[func.instructions.len()] = new_idx;

        for (old_idx, inst) in func.instructions.iter().enumerate() {
            if to_remove[old_idx] {
                continue;
            }
            let mut new_inst = inst.clone();
            match &mut new_inst {
                Instruction::Jump { offset }
                | Instruction::JumpTrue { offset, .. }
                | Instruction::JumpFalse { offset, .. }
                | Instruction::JumpNull { offset, .. } => {
                    let old_target = (old_idx as i32 + 1 + *offset) as usize;
                    let new_target = old_to_new[old_target];
                    let current_new_idx = old_to_new[old_idx];
                    *offset = new_target as i32 - current_new_idx as i32 - 1;
                }
                _ => {}
            }
            new_instructions.push(new_inst);
        }

        func.instructions = new_instructions;
    }
}
