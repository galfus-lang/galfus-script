mod allocator;
mod liveness;
#[cfg(test)]
mod tests;

use galfus_bytecode::{
    BytecodeFunction, BytecodeGraphTransaction, BytecodeModule, Constant, ExportKind, Instruction,
    PackageImage, Reg,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use galfus_core::SemanticRevision;

#[derive(Debug, Default)]
struct PackageOptimizationReport {
    call_graph_changed: bool,
}

pub(crate) fn optimize_package(
    package: Arc<PackageImage>,
    previous_finalized: Option<&PackageImage>,
    semantic_revision: SemanticRevision,
) -> Result<Arc<PackageImage>, String> {
    let graph = package.graph();
    let finalized_graph = previous_finalized.map_or(graph, PackageImage::graph);
    let method_names = global_method_names(graph.modules().map(|node| &node.module));

    let mut report = PackageOptimizationReport::default();
    let mut changed_nodes = HashMap::new();
    for node in graph.modules() {
        let needs_pruning = module_needs_pruning(&node.module, &method_names);
        let mut new_node = node.clone();
        let mut changed = false;
        if needs_pruning {
            #[cfg(debug_assertions)]
            let counts_before = module_counts(&new_node.module);
            let function_remap = prune_module(&mut new_node.module, &method_names);
            if let Some(metadata) = &mut new_node.metadata {
                metadata.remap_functions(&function_remap);
            }
            #[cfg(debug_assertions)]
            emit_prune_telemetry(
                node.path(),
                "initial",
                counts_before,
                module_counts(&new_node.module),
            );
            changed = true;
        }

        for (index, func) in new_node.module.functions.iter_mut().enumerate() {
            let mut optimized = func.clone();
            let calls_before = direct_call_count(&optimized);
            let offset_remap = optimize_function(&mut optimized);
            if optimized == *func {
                continue;
            }
            let calls_after = direct_call_count(&optimized);
            *func = optimized;
            if let Some(metadata) = &mut new_node.metadata {
                metadata.remap_instruction_offsets(
                    galfus_bytecode::FuncIdx(index as u16),
                    &offset_remap,
                );
            }
            report.call_graph_changed |= calls_before != calls_after;
            changed = true;
        }

        if changed
            || finalized_graph
                .get(node.id())
                .is_none_or(|previous| previous != &new_node)
        {
            changed_nodes.insert(node.id(), new_node);
        }
    }

    if report.call_graph_changed {
        let post_optimization_names = global_method_names(graph.modules().map(|node| {
            changed_nodes
                .get(&node.id())
                .map_or(&node.module, |changed| &changed.module)
        }));
        for node in graph.modules() {
            let current = changed_nodes.get(&node.id()).unwrap_or(node);
            if !module_needs_pruning(&current.module, &post_optimization_names) {
                continue;
            }
            let mut pruned = current.clone();
            #[cfg(debug_assertions)]
            let counts_before = module_counts(&pruned.module);
            let function_remap = prune_module(&mut pruned.module, &post_optimization_names);
            if let Some(metadata) = &mut pruned.metadata {
                metadata.remap_functions(&function_remap);
            }
            #[cfg(debug_assertions)]
            emit_prune_telemetry(
                node.path(),
                "post-call-graph",
                counts_before,
                module_counts(&pruned.module),
            );
            changed_nodes.insert(node.id(), pruned);
        }
    }

    changed_nodes.retain(|id, node| {
        !finalized_graph
            .get(*id)
            .is_some_and(|previous| previous == node)
    });

    let removed_modules = finalized_graph
        .modules()
        .filter(|node| graph.get(node.id()).is_none())
        .map(|node| node.id())
        .collect::<Vec<_>>();
    if changed_nodes.is_empty()
        && removed_modules.is_empty()
        && finalized_graph.edges() == graph.edges()
    {
        if let Some(previous_finalized) = previous_finalized {
            return Ok(Arc::new(previous_finalized.clone()));
        }
        return Ok(package);
    }

    let transaction = BytecodeGraphTransaction {
        base_version: finalized_graph.version(),
        semantic_revision,
        upserted_modules: changed_nodes.into_values().collect(),
        removed_modules,
        edges: graph.edges().to_vec(),
    };

    let new_graph = finalized_graph
        .apply(transaction)
        .map_err(|e| e.to_string())?;

    PackageImage::try_new(
        new_graph,
        package.target().clone(),
        package.entry_point().cloned(),
        package.metadata().clone(),
        package.limits().clone(),
        package.adapter_requirements().to_vec(),
        package.provider_requirements().to_vec(),
    )
    .map(Arc::new)
    .map_err(|e| e.to_string())
}

#[cfg(debug_assertions)]
fn module_counts(module: &BytecodeModule) -> (usize, usize) {
    (module.functions.len(), module.constants.constants.len())
}

#[cfg(debug_assertions)]
fn emit_prune_telemetry(
    path: &galfus_core::ModulePath,
    stage: &str,
    before: (usize, usize),
    after: (usize, usize),
) {
    if std::env::var_os("GALFUS_FINALIZATION_TELEMETRY").is_some() {
        eprintln!(
            "finalization module={} stage={} functions retained={} removed={} constants retained={} removed={}",
            path.as_str(),
            stage,
            after.0,
            before.0.saturating_sub(after.0),
            after.1,
            before.1.saturating_sub(after.1),
        );
    }
}

fn global_method_names<'a>(modules: impl Iterator<Item = &'a BytecodeModule>) -> HashSet<String> {
    let mut names = HashSet::new();
    for module in modules {
        for function in &module.functions {
            for instruction in &function.instructions {
                if let Instruction::CallMethod { name_const, .. } = instruction
                    && let Some(Constant::String(name)) =
                        module.constants.constants.get(name_const.0 as usize)
                {
                    names.insert(name.clone());
                }
            }
        }
    }
    names
}

fn direct_call_count(function: &BytecodeFunction) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction,
                Instruction::Call { .. }
                    | Instruction::CreateFuture { .. }
                    | Instruction::TailCall { .. }
            )
        })
        .count()
}

fn optimize_function(func: &mut BytecodeFunction) -> Vec<Option<usize>> {
    let instructions = &func.instructions;
    let len = instructions.len();
    let mut reachable = vec![false; len];
    let mut pending = len.checked_sub(1).map(|_| vec![0]).unwrap_or_default();
    while let Some(index) = pending.pop() {
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        let next = index + 1;
        match instructions[index] {
            Instruction::Jump { offset } => pending.push(jump_target(index, offset)),
            Instruction::JumpTrue { offset, .. }
            | Instruction::JumpFalse { offset, .. }
            | Instruction::JumpNull { offset, .. } => {
                pending.push(jump_target(index, offset));
                if next < len {
                    pending.push(next);
                }
            }
            Instruction::Ret { .. } | Instruction::RetNull | Instruction::Panic { .. } => {}
            _ if next < len => pending.push(next),
            _ => {}
        }
    }
    let mut remove = instructions
        .iter()
        .enumerate()
        .map(|(index, instruction)| {
            !reachable[index]
                || matches!(instruction, Instruction::Move { dest, src } if dest == src)
                || matches!(instruction, Instruction::Jump { offset } if *offset == 0)
        })
        .collect::<Vec<_>>();

    // Removing a jump can make its predecessor a jump-to-next. Iterate only
    // over the removal bitmap; the instruction sequence is rebuilt once below.
    loop {
        let mut changed = false;
        for (index, instruction) in instructions.iter().enumerate() {
            if remove[index] || !matches!(instruction, Instruction::Jump { .. }) {
                continue;
            }
            let Instruction::Jump { offset } = instruction else {
                unreachable!("matched unconditional jump");
            };
            let target =
                resolve_jump_target(instructions, remove.as_slice(), jump_target(index, *offset));
            if target == next_retained_index(index + 1, remove.as_slice()) {
                remove[index] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut jump_targets = vec![None; len];
    for (index, instruction) in instructions.iter().enumerate() {
        let offset = match instruction {
            Instruction::Jump { offset }
            | Instruction::JumpTrue { offset, .. }
            | Instruction::JumpFalse { offset, .. }
            | Instruction::JumpNull { offset, .. } => *offset,
            _ => continue,
        };
        jump_targets[index] = Some(resolve_jump_target(
            instructions,
            remove.as_slice(),
            jump_target(index, offset),
        ));
    }

    let mut old_to_new = vec![None; len + 1];
    let mut next_new = 0;
    for index in 0..len {
        if !remove[index] {
            old_to_new[index] = Some(next_new);
            next_new += 1;
        }
    }
    old_to_new[len] = Some(next_new);
    let mut rewritten = Vec::with_capacity(next_new);
    for (index, instruction) in instructions.iter().enumerate() {
        let Some(new_index) = old_to_new[index] else {
            continue;
        };
        let mut instruction = instruction.clone();
        match &mut instruction {
            Instruction::Jump { offset }
            | Instruction::JumpTrue { offset, .. }
            | Instruction::JumpFalse { offset, .. }
            | Instruction::JumpNull { offset, .. } => {
                let target = jump_targets[index].expect("jump target is recorded");
                *offset = old_to_new[target].expect("reachable jump target") as i32
                    - new_index as i32
                    - 1;
            }
            _ => {}
        }
        rewritten.push(instruction);
    }
    let offset_remap = (0..len)
        .map(|index| {
            if !reachable[index] {
                return None;
            }
            if matches!(instructions[index], Instruction::Jump { .. }) && remove[index] {
                return jump_targets[index].and_then(|target| old_to_new[target]);
            }
            old_to_new[next_retained_index(index, remove.as_slice())]
        })
        .collect();
    func.instructions = rewritten;

    compact_registers(func);

    let register_count =
        func.param_count as usize + func.local_count as usize + func.temp_count as usize;
    let mut blocks = liveness::build_cfg(&func.instructions, register_count);
    liveness::compute_liveness(&mut blocks, &func.instructions, register_count);
    let intervals = liveness::compute_intervals(&blocks, &func.instructions, register_count);
    if supports_liveness_allocation(&func.instructions)
        && allocator::allocate_registers(func, &intervals, register_count)
    {
        compact_registers(func);
    }

    offset_remap
}

fn supports_liveness_allocation(instructions: &[Instruction]) -> bool {
    !instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::AllocLocal { .. }
                | Instruction::LoadField { .. }
                | Instruction::StoreField { .. }
                | Instruction::NewArray { .. }
                | Instruction::LoadIndex { .. }
                | Instruction::StoreIndex { .. }
                | Instruction::NewChoice { .. }
                | Instruction::Copy { .. }
                | Instruction::Instanceof { .. }
                | Instruction::Call { .. }
                | Instruction::TailCall { .. }
                | Instruction::CallMethod { .. }
                | Instruction::CallDynamic { .. }
                | Instruction::AwaitFuture { .. }
                | Instruction::CreateFuture { .. }
                | Instruction::CreateAwaitFuture { .. }
                | Instruction::CallInternalThread { .. }
                | Instruction::CreateIndirectFuture { .. }
                | Instruction::AwaitAll { .. }
                | Instruction::AwaitRace { .. }
                | Instruction::Len { .. }
                | Instruction::CopyArray { .. }
        )
    })
}

fn jump_target(index: usize, offset: i32) -> usize {
    usize::try_from(index as i32 + 1 + offset).expect("validated jump target")
}

fn next_retained_index(index: usize, remove: &[bool]) -> usize {
    let mut next = index;
    while next < remove.len() && remove[next] {
        next += 1;
    }
    next
}

fn resolve_jump_target(instructions: &[Instruction], remove: &[bool], mut target: usize) -> usize {
    let mut visited = HashSet::new();
    while target < instructions.len() && visited.insert(target) {
        target = next_retained_index(target, remove);
        let Some(Instruction::Jump { offset }) = instructions.get(target) else {
            break;
        };
        let next = (target as i32 + 1 + *offset) as usize;
        if next == target {
            break;
        }
        target = next;
    }
    target
}

fn compact_registers(func: &mut BytecodeFunction) {
    let register_count =
        func.param_count as usize + func.local_count as usize + func.temp_count as usize;
    let mut used = vec![false; register_count];
    for is_used in used.iter_mut().take(func.param_count as usize) {
        *is_used = true;
    }
    for instruction in &func.instructions {
        let mut registers = Vec::new();
        let mut ranges = Vec::new();
        visit_instruction_registers(
            instruction,
            |reg| registers.push(reg),
            |start, count| {
                ranges.push((start, count));
            },
        );
        for reg in registers {
            used[reg.raw() as usize] = true;
        }
        for (start, count) in ranges {
            for is_used in used
                .iter_mut()
                .skip(start.raw() as usize)
                .take(count as usize)
            {
                *is_used = true;
            }
        }
    }

    let parameter_count = func.param_count as usize;
    let local_end = parameter_count + func.local_count as usize;
    let local_count = used[parameter_count..local_end]
        .iter()
        .filter(|used| **used)
        .count();
    let temp_count = used[local_end..].iter().filter(|used| **used).count();
    let mut remap = vec![None; register_count];
    for (index, target) in remap.iter_mut().enumerate().take(parameter_count) {
        *target = Some(Reg(index as u16));
    }
    let mut next = parameter_count as u16;
    for (index, is_used) in used.iter().enumerate().skip(parameter_count) {
        if *is_used {
            remap[index] = Some(Reg(next));
            next += 1;
        }
    }

    for instruction in &mut func.instructions {
        remap_instruction_registers(instruction, &remap);
    }
    func.local_count = local_count as u16;
    func.temp_count = temp_count as u16;
}

fn visit_instruction_registers(
    instruction: &Instruction,
    mut register: impl FnMut(Reg),
    mut range: impl FnMut(Reg, u8),
) {
    use Instruction::*;

    match instruction {
        LoadConst { dest, .. }
        | LoadGlobal { dest, .. }
        | LoadNull { dest }
        | AllocLocal { dest, .. } => register(*dest),
        Move { dest, src } | Copy { dest, src } | Len { dest, src } => {
            register(*dest);
            register(*src);
        }
        StoreGlobal { src, .. } | Ret { src } => register(*src),
        Add { dest, lhs, rhs }
        | Sub { dest, lhs, rhs }
        | Mul { dest, lhs, rhs }
        | Div { dest, lhs, rhs }
        | Rem { dest, lhs, rhs }
        | Pow { dest, lhs, rhs }
        | Shl { dest, lhs, rhs }
        | Shr { dest, lhs, rhs }
        | And { dest, lhs, rhs }
        | Or { dest, lhs, rhs }
        | Xor { dest, lhs, rhs }
        | Eq { dest, lhs, rhs }
        | Ne { dest, lhs, rhs }
        | Lt { dest, lhs, rhs }
        | Le { dest, lhs, rhs }
        | Gt { dest, lhs, rhs }
        | Ge { dest, lhs, rhs }
        | AddI32 { dest, lhs, rhs }
        | SubI32 { dest, lhs, rhs }
        | MulI32 { dest, lhs, rhs }
        | DivI32 { dest, lhs, rhs }
        | RemI32 { dest, lhs, rhs }
        | EqI32 { dest, lhs, rhs }
        | NeI32 { dest, lhs, rhs }
        | LtI32 { dest, lhs, rhs }
        | LeI32 { dest, lhs, rhs }
        | GtI32 { dest, lhs, rhs }
        | GeI32 { dest, lhs, rhs }
        | AddI64 { dest, lhs, rhs }
        | SubI64 { dest, lhs, rhs }
        | MulI64 { dest, lhs, rhs }
        | DivI64 { dest, lhs, rhs }
        | RemI64 { dest, lhs, rhs }
        | EqI64 { dest, lhs, rhs }
        | NeI64 { dest, lhs, rhs }
        | LtI64 { dest, lhs, rhs }
        | LeI64 { dest, lhs, rhs }
        | GtI64 { dest, lhs, rhs }
        | GeI64 { dest, lhs, rhs }
        | AddF32 { dest, lhs, rhs }
        | SubF32 { dest, lhs, rhs }
        | MulF32 { dest, lhs, rhs }
        | DivF32 { dest, lhs, rhs }
        | RemF32 { dest, lhs, rhs }
        | EqF32 { dest, lhs, rhs }
        | NeF32 { dest, lhs, rhs }
        | LtF32 { dest, lhs, rhs }
        | LeF32 { dest, lhs, rhs }
        | GtF32 { dest, lhs, rhs }
        | GeF32 { dest, lhs, rhs }
        | AddF64 { dest, lhs, rhs }
        | SubF64 { dest, lhs, rhs }
        | MulF64 { dest, lhs, rhs }
        | DivF64 { dest, lhs, rhs }
        | RemF64 { dest, lhs, rhs }
        | EqF64 { dest, lhs, rhs }
        | NeF64 { dest, lhs, rhs }
        | LtF64 { dest, lhs, rhs }
        | LeF64 { dest, lhs, rhs }
        | GtF64 { dest, lhs, rhs }
        | GeF64 { dest, lhs, rhs } => {
            register(*dest);
            register(*lhs);
            register(*rhs);
        }
        BinaryImmediate { dest, lhs, .. } => {
            register(*dest);
            register(*lhs);
        }
        Neg { dest, src }
        | Not { dest, src }
        | BitNot { dest, src }
        | Cast { dest, src, .. }
        | Instanceof { dest, src, .. } => {
            register(*dest);
            register(*src);
        }
        Fallback {
            dest,
            src,
            fallback,
        } => {
            register(*dest);
            register(*src);
            register(*fallback);
        }
        Jump { .. } | RetNull | Panic { .. } => {}
        JumpTrue { cond, .. } | JumpFalse { cond, .. } => register(*cond),
        JumpNull { val, .. } => register(*val),
        Call {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CreateFuture {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CreateAwaitFuture {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CallInternalThread {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CallInternalMath {
            dest,
            args_start,
            arg_count,
            ..
        } => {
            register(*dest);
            if *arg_count > 0 {
                range(*args_start, *arg_count);
            }
        }
        TailCall {
            args_start,
            arg_count,
            ..
        } => {
            if *arg_count > 0 {
                range(*args_start, *arg_count);
            }
        }
        CallMethod {
            dest,
            obj,
            args_start,
            arg_count,
            ..
        } => {
            register(*dest);
            register(*obj);
            if *arg_count > 1 {
                range(*args_start, *arg_count - 1);
            }
        }
        CallDynamic {
            dest,
            func_reg,
            args_start,
            arg_count,
        }
        | CreateIndirectFuture {
            dest,
            func_reg,
            args_start,
            arg_count,
            ..
        } => {
            register(*dest);
            register(*func_reg);
            if *arg_count > 0 {
                range(*args_start, *arg_count);
            }
        }
        LoadField { dest, obj, .. } => {
            register(*dest);
            register(*obj);
        }
        StoreField { obj, val, .. } => {
            register(*obj);
            register(*val);
        }
        NewArray { dest, len_reg, .. } => {
            register(*dest);
            register(*len_reg);
        }
        LoadIndex { dest, arr, idx } => {
            register(*dest);
            register(*arr);
            register(*idx);
        }
        StoreIndex { arr, idx, val } => {
            register(*arr);
            register(*idx);
            register(*val);
        }
        NewTuple {
            dest, start, count, ..
        } => {
            register(*dest);
            if *count > 0 {
                range(*start, *count);
            }
        }
        NewChoice { dest, payload, .. } => {
            register(*dest);
            register(*payload);
        }
        Drop { reg } => register(*reg),
        AwaitFuture {
            dest, future_id, ..
        } => {
            register(*dest);
            register(*future_id);
        }
        AwaitAll {
            dest,
            futures_start,
            count,
            ..
        }
        | AwaitRace {
            dest,
            futures_start,
            count,
            ..
        } => {
            register(*dest);
            if *count > 0 {
                range(*futures_start, *count);
            }
        }
        CopyArray {
            dest,
            dest_start,
            src,
        } => {
            register(*dest);
            register(*dest_start);
            register(*src);
        }
    }
}

fn remap_instruction_registers(instruction: &mut Instruction, remap: &[Option<Reg>]) {
    visit_instruction_registers_mut(instruction, |reg| {
        *reg = remap[reg.raw() as usize].expect("used register must have a compacted index");
    });
}

fn visit_instruction_registers_mut(
    instruction: &mut Instruction,
    mut register: impl FnMut(&mut Reg),
) {
    use Instruction::*;

    match instruction {
        LoadConst { dest, .. }
        | LoadGlobal { dest, .. }
        | LoadNull { dest }
        | AllocLocal { dest, .. } => register(dest),
        Move { dest, src } | Copy { dest, src } | Len { dest, src } => {
            register(dest);
            register(src);
        }
        StoreGlobal { src, .. } | Ret { src } => register(src),
        Add { dest, lhs, rhs }
        | Sub { dest, lhs, rhs }
        | Mul { dest, lhs, rhs }
        | Div { dest, lhs, rhs }
        | Rem { dest, lhs, rhs }
        | Pow { dest, lhs, rhs }
        | Shl { dest, lhs, rhs }
        | Shr { dest, lhs, rhs }
        | And { dest, lhs, rhs }
        | Or { dest, lhs, rhs }
        | Xor { dest, lhs, rhs }
        | Eq { dest, lhs, rhs }
        | Ne { dest, lhs, rhs }
        | Lt { dest, lhs, rhs }
        | Le { dest, lhs, rhs }
        | Gt { dest, lhs, rhs }
        | Ge { dest, lhs, rhs }
        | AddI32 { dest, lhs, rhs }
        | SubI32 { dest, lhs, rhs }
        | MulI32 { dest, lhs, rhs }
        | DivI32 { dest, lhs, rhs }
        | RemI32 { dest, lhs, rhs }
        | EqI32 { dest, lhs, rhs }
        | NeI32 { dest, lhs, rhs }
        | LtI32 { dest, lhs, rhs }
        | LeI32 { dest, lhs, rhs }
        | GtI32 { dest, lhs, rhs }
        | GeI32 { dest, lhs, rhs }
        | AddI64 { dest, lhs, rhs }
        | SubI64 { dest, lhs, rhs }
        | MulI64 { dest, lhs, rhs }
        | DivI64 { dest, lhs, rhs }
        | RemI64 { dest, lhs, rhs }
        | EqI64 { dest, lhs, rhs }
        | NeI64 { dest, lhs, rhs }
        | LtI64 { dest, lhs, rhs }
        | LeI64 { dest, lhs, rhs }
        | GtI64 { dest, lhs, rhs }
        | GeI64 { dest, lhs, rhs }
        | AddF32 { dest, lhs, rhs }
        | SubF32 { dest, lhs, rhs }
        | MulF32 { dest, lhs, rhs }
        | DivF32 { dest, lhs, rhs }
        | RemF32 { dest, lhs, rhs }
        | EqF32 { dest, lhs, rhs }
        | NeF32 { dest, lhs, rhs }
        | LtF32 { dest, lhs, rhs }
        | LeF32 { dest, lhs, rhs }
        | GtF32 { dest, lhs, rhs }
        | GeF32 { dest, lhs, rhs }
        | AddF64 { dest, lhs, rhs }
        | SubF64 { dest, lhs, rhs }
        | MulF64 { dest, lhs, rhs }
        | DivF64 { dest, lhs, rhs }
        | RemF64 { dest, lhs, rhs }
        | EqF64 { dest, lhs, rhs }
        | NeF64 { dest, lhs, rhs }
        | LtF64 { dest, lhs, rhs }
        | LeF64 { dest, lhs, rhs }
        | GtF64 { dest, lhs, rhs }
        | GeF64 { dest, lhs, rhs } => {
            register(dest);
            register(lhs);
            register(rhs);
        }
        BinaryImmediate { dest, lhs, .. } => {
            register(dest);
            register(lhs);
        }
        Neg { dest, src }
        | Not { dest, src }
        | BitNot { dest, src }
        | Cast { dest, src, .. }
        | Instanceof { dest, src, .. } => {
            register(dest);
            register(src);
        }
        Fallback {
            dest,
            src,
            fallback,
        } => {
            register(dest);
            register(src);
            register(fallback);
        }
        Jump { .. } | RetNull | Panic { .. } => {}
        JumpTrue { cond, .. } | JumpFalse { cond, .. } => register(cond),
        JumpNull { val, .. } => register(val),
        Call {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CreateFuture {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CreateAwaitFuture {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CallInternalThread {
            dest,
            args_start,
            arg_count,
            ..
        }
        | CallInternalMath {
            dest,
            args_start,
            arg_count,
            ..
        } => {
            register(dest);
            if *arg_count > 0 {
                register(args_start);
            }
        }
        TailCall {
            args_start,
            arg_count,
            ..
        } => {
            if *arg_count > 0 {
                register(args_start);
            }
        }
        CallMethod {
            dest,
            obj,
            args_start,
            arg_count,
            ..
        } => {
            register(dest);
            register(obj);
            if *arg_count > 1 {
                register(args_start);
            }
        }
        CallDynamic {
            dest,
            func_reg,
            args_start,
            arg_count,
        }
        | CreateIndirectFuture {
            dest,
            func_reg,
            args_start,
            arg_count,
            ..
        } => {
            register(dest);
            register(func_reg);
            if *arg_count > 0 {
                register(args_start);
            }
        }
        LoadField { dest, obj, .. } => {
            register(dest);
            register(obj);
        }
        StoreField { obj, val, .. } => {
            register(obj);
            register(val);
        }
        NewArray { dest, len_reg, .. } => {
            register(dest);
            register(len_reg);
        }
        LoadIndex { dest, arr, idx } => {
            register(dest);
            register(arr);
            register(idx);
        }
        StoreIndex { arr, idx, val } => {
            register(arr);
            register(idx);
            register(val);
        }
        NewTuple {
            dest, start, count, ..
        } => {
            register(dest);
            if *count > 0 {
                register(start);
            }
        }
        NewChoice { dest, payload, .. } => {
            register(dest);
            register(payload);
        }
        Drop { reg } => register(reg),
        AwaitFuture {
            dest, future_id, ..
        } => {
            register(dest);
            register(future_id);
        }
        AwaitAll {
            dest,
            futures_start,
            count,
            ..
        }
        | AwaitRace {
            dest,
            futures_start,
            count,
            ..
        } => {
            register(dest);
            if *count > 0 {
                register(futures_start);
            }
        }
        CopyArray {
            dest,
            dest_start,
            src,
        } => {
            register(dest);
            register(dest_start);
            register(src);
        }
    }
}

fn is_called_method(name: &str, global_method_names: &HashSet<String>) -> bool {
    let mut candidate = name;
    loop {
        if global_method_names.contains(candidate) {
            return true;
        }
        let Some((_, suffix)) = candidate.split_once("::") else {
            return false;
        };
        candidate = suffix;
    }
}

fn module_needs_pruning(module: &BytecodeModule, global_method_names: &HashSet<String>) -> bool {
    let (reachable_functions, reachable_constants) =
        collect_reachable_indices(module, global_method_names);
    reachable_functions.len() != module.functions.len()
        || reachable_constants.len() != module.constants.constants.len()
}

fn collect_reachable_indices(
    module: &BytecodeModule,
    global_method_names: &HashSet<String>,
) -> (HashSet<usize>, HashSet<usize>) {
    let mut reachable_functions = HashSet::new();
    let mut reachable_constants = HashSet::new();
    let mut function_queue = Vec::new();
    let mut constant_queue = Vec::new();
    let function_count = module.functions.len();

    if let Some(init_idx) = module.init_func_idx
        && (init_idx.0 as usize) < function_count
    {
        reachable_functions.insert(init_idx.0 as usize);
        function_queue.push(init_idx.0 as usize);
    }
    for export in &module.exports {
        if let ExportKind::Function(idx) = &export.kind
            && (idx.0 as usize) < function_count
            && reachable_functions.insert(idx.0 as usize)
        {
            function_queue.push(idx.0 as usize);
        }
    }
    for (index, function) in module.functions.iter().enumerate() {
        if (function.adapter_proxy_metadata.is_some()
            || is_called_method(&function.name, global_method_names))
            && reachable_functions.insert(index)
        {
            function_queue.push(index);
        }
    }

    while !function_queue.is_empty() || !constant_queue.is_empty() {
        while let Some(function_index) = function_queue.pop() {
            let function = &module.functions[function_index];
            for instruction in &function.instructions {
                use galfus_bytecode::Instruction::*;
                match instruction {
                    LoadConst { const_idx, .. } | Panic { const_idx } => {
                        if reachable_constants.insert(const_idx.0 as usize) {
                            constant_queue.push(const_idx.0 as usize);
                        }
                    }
                    CallMethod { name_const, .. } => {
                        if reachable_constants.insert(name_const.0 as usize) {
                            constant_queue.push(name_const.0 as usize);
                        }
                    }
                    Call { func, .. } | CreateFuture { func, .. } | TailCall { func, .. }
                        if (func.0 as usize) < function_count
                            && reachable_functions.insert(func.0 as usize) =>
                    {
                        function_queue.push(func.0 as usize);
                    }
                    _ => {}
                }
            }
        }

        while let Some(constant_index) = constant_queue.pop() {
            if let Some(Constant::Function(function_idx)) =
                module.constants.constants.get(constant_index)
                && (function_idx.0 as usize) < function_count
                && reachable_functions.insert(function_idx.0 as usize)
            {
                function_queue.push(function_idx.0 as usize);
            }
        }
    }

    (reachable_functions, reachable_constants)
}

pub(crate) fn prune_module(
    module: &mut BytecodeModule,
    global_method_names: &HashSet<String>,
) -> Vec<Option<galfus_bytecode::FuncIdx>> {
    let old_funcs_len = module.functions.len();
    let (reachable_functions, reachable_constants) =
        collect_reachable_indices(module, global_method_names);

    let mut func_remap = vec![None; module.functions.len()];
    let mut new_funcs = Vec::new();
    for (i, f) in std::mem::take(&mut module.functions)
        .into_iter()
        .enumerate()
    {
        if reachable_functions.contains(&i) {
            func_remap[i] = Some(galfus_bytecode::FuncIdx(new_funcs.len() as u16));
            new_funcs.push(f);
        }
    }
    let new_funcs_len = new_funcs.len();
    module.functions = new_funcs;

    let remap_func_idx = |f: galfus_bytecode::FuncIdx,
                          func_remap: &[Option<galfus_bytecode::FuncIdx>],
                          old_len: usize,
                          new_len: usize|
     -> galfus_bytecode::FuncIdx {
        let idx = f.0 as usize;
        if idx < old_len {
            func_remap[idx].expect("retained bytecode must not reference a pruned function")
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
                Call { func, .. } | CreateFuture { func, .. } | TailCall { func, .. } => {
                    *func = remap_func_idx(*func, &func_remap, old_funcs_len, new_funcs_len);
                }
                _ => {}
            }
        }
    }

    func_remap
}
