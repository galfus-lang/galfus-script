use galfus_bytecode::{
    BytecodeFunction, BytecodeGraphTransaction, BytecodeModule, Constant, ExportKind, Instruction,
    PackageImage,
};
use std::collections::HashSet;

use galfus_core::SemanticRevision;

pub fn optimize_package(
    package: &PackageImage,
    semantic_revision: SemanticRevision,
) -> Result<PackageImage, String> {
    let graph = package.graph();
    let mut new_modules = Vec::new();

    let mut global_method_names = HashSet::new();
    for node in graph.modules() {
        for func in &node.module.functions {
            for inst in &func.instructions {
                if let Instruction::CallMethod { name_const, .. } = inst
                    && let Some(Constant::String(name)) =
                        node.module.constants.constants.get(name_const.0 as usize)
                {
                    global_method_names.insert(name.clone());
                }
            }
        }
    }

    for node in graph.modules() {
        let mut new_node = node.clone();
        for func in &mut new_node.module.functions {
            optimize_function(func);
        }

        prune_module(&mut new_node.module, &global_method_names);

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

            if let Instruction::Move { dest, src } = inst
                && dest == src
            {
                to_remove[i] = true;
                changed = true;
                continue;
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
pub fn prune_module(module: &mut BytecodeModule, global_method_names: &HashSet<String>) {
    let mut reachable_functions = HashSet::new();
    let mut reachable_constants = HashSet::new();

    let mut func_queue = Vec::new();
    let mut const_queue = Vec::new();

    let old_funcs_len = module.functions.len();

    if let Some(init_idx) = module.init_func_idx
        && (init_idx.0 as usize) < old_funcs_len
    {
        reachable_functions.insert(init_idx.0 as usize);
        func_queue.push(init_idx.0 as usize);
    }
    for export in &module.exports {
        if let ExportKind::Function(idx) = &export.kind
            && (idx.0 as usize) < old_funcs_len
        {
            reachable_functions.insert(idx.0 as usize);
            func_queue.push(idx.0 as usize);
        }
    }
    for (i, func) in module.functions.iter().enumerate() {
        let is_called_method = global_method_names
            .iter()
            .any(|method| func.name == *method || func.name.ends_with(&format!("::{method}")));

        if func.adapter_proxy_metadata.is_some() || is_called_method {
            reachable_functions.insert(i);
            func_queue.push(i);
        }
    }

    while !func_queue.is_empty() || !const_queue.is_empty() {
        while let Some(func_idx) = func_queue.pop() {
            let func = &module.functions[func_idx];
            for inst in &func.instructions {
                use galfus_bytecode::Instruction::*;
                match inst {
                    LoadConst { const_idx, .. } => {
                        if reachable_constants.insert(const_idx.0 as usize) {
                            const_queue.push(const_idx.0 as usize);
                        }
                    }
                    CallMethod { name_const, .. } => {
                        if reachable_constants.insert(name_const.0 as usize) {
                            const_queue.push(name_const.0 as usize);
                        }
                    }
                    Panic { const_idx } => {
                        if reachable_constants.insert(const_idx.0 as usize) {
                            const_queue.push(const_idx.0 as usize);
                        }
                    }
                    Call { func, .. } | CreateFuture { func, .. }
                        if (func.0 as usize) < old_funcs_len
                            && reachable_functions.insert(func.0 as usize) =>
                    {
                        func_queue.push(func.0 as usize);
                    }
                    _ => {}
                }
            }
        }

        while let Some(c_idx) = const_queue.pop() {
            if let Constant::Function(f_idx) = &module.constants.constants[c_idx]
                && (f_idx.0 as usize) < old_funcs_len
                && reachable_functions.insert(f_idx.0 as usize)
            {
                func_queue.push(f_idx.0 as usize);
            }
        }
    }

    let mut func_remap = vec![0; module.functions.len()];
    let mut new_funcs = Vec::new();
    for (i, f) in std::mem::take(&mut module.functions)
        .into_iter()
        .enumerate()
    {
        if reachable_functions.contains(&i) {
            func_remap[i] = new_funcs.len() as u16;
            new_funcs.push(f);
        }
    }
    let new_funcs_len = new_funcs.len();
    module.functions = new_funcs;

    let remap_func_idx = |f: galfus_bytecode::FuncIdx,
                          func_remap: &[u16],
                          old_len: usize,
                          new_len: usize|
     -> galfus_bytecode::FuncIdx {
        let idx = f.0 as usize;
        if idx < old_len {
            galfus_bytecode::FuncIdx(func_remap[idx])
        } else {
            let import_idx = idx - old_len;
            galfus_bytecode::FuncIdx((import_idx + new_len) as u16)
        }
    };

    let mut const_remap = vec![0; module.constants.constants.len()];
    let mut new_consts = Vec::new();
    for (i, c) in std::mem::take(&mut module.constants.constants)
        .into_iter()
        .enumerate()
    {
        if reachable_constants.contains(&i) {
            const_remap[i] = new_consts.len() as u16;
            new_consts.push(c);
        }
    }
    module.constants.constants = new_consts;

    if let Some(init_idx) = &mut module.init_func_idx {
        *init_idx = remap_func_idx(*init_idx, &func_remap, old_funcs_len, new_funcs_len);
    }
    for export in &mut module.exports {
        if let ExportKind::Function(idx) = &mut export.kind {
            *idx = remap_func_idx(*idx, &func_remap, old_funcs_len, new_funcs_len);
        }
    }
    for c in &mut module.constants.constants {
        if let Constant::Function(f_idx) = c {
            *f_idx = remap_func_idx(*f_idx, &func_remap, old_funcs_len, new_funcs_len);
        }
    }

    for func in &mut module.functions {
        for inst in &mut func.instructions {
            use galfus_bytecode::Instruction::*;
            match inst {
                LoadConst { const_idx, .. } => {
                    const_idx.0 = const_remap[const_idx.0 as usize];
                }
                CallMethod { name_const, .. } => {
                    name_const.0 = const_remap[name_const.0 as usize];
                }
                Panic { const_idx } => {
                    const_idx.0 = const_remap[const_idx.0 as usize];
                }
                Call { func, .. } | CreateFuture { func, .. } => {
                    *func = remap_func_idx(*func, &func_remap, old_funcs_len, new_funcs_len);
                }
                _ => {}
            }
        }
    }
}
