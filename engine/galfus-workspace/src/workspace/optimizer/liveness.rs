use galfus_bytecode::{Instruction, Reg};

pub struct BasicBlock {
    pub start: usize,
    pub end: usize,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
    pub live_in: Vec<bool>,
    pub live_out: Vec<bool>,
}

pub fn build_cfg(instructions: &[Instruction], register_count: usize) -> Vec<BasicBlock> {
    // Determine basic block boundaries (leaders)
    let mut is_leader = vec![false; instructions.len()];
    if !instructions.is_empty() {
        is_leader[0] = true;
    }

    for (i, inst) in instructions.iter().enumerate() {
        use Instruction::*;
        let next = i + 1;
        match inst {
            Jump { offset }
            | JumpTrue { offset, .. }
            | JumpFalse { offset, .. }
            | JumpNull { offset, .. } => {
                let target = (i as i32 + 1 + *offset) as usize;
                if target < instructions.len() {
                    is_leader[target] = true;
                }
                if next < instructions.len() {
                    is_leader[next] = true;
                }
            }
            Ret { .. } | RetNull | Panic { .. } if next < instructions.len() => {
                is_leader[next] = true;
            }
            _ => {}
        }
    }

    let mut blocks = Vec::new();
    let mut current_start = 0;
    for (i, is_leader) in is_leader.iter().enumerate().skip(1) {
        if *is_leader {
            blocks.push(BasicBlock {
                start: current_start,
                end: i,
                predecessors: Vec::new(),
                successors: Vec::new(),
                live_in: vec![false; register_count],
                live_out: vec![false; register_count],
            });
            current_start = i;
        }
    }
    if current_start < instructions.len() {
        blocks.push(BasicBlock {
            start: current_start,
            end: instructions.len(),
            predecessors: Vec::new(),
            successors: Vec::new(),
            live_in: vec![false; register_count],
            live_out: vec![false; register_count],
        });
    }

    // Connect blocks
    let mut edges = Vec::new();
    for i in 0..blocks.len() {
        let last_inst = &instructions[blocks[i].end - 1];
        let next_idx = blocks[i].end;
        use Instruction::*;
        match last_inst {
            Jump { offset } => {
                let target = (blocks[i].end as i32 + *offset) as usize;
                edges.push((i, target));
            }
            JumpTrue { offset, .. } | JumpFalse { offset, .. } | JumpNull { offset, .. } => {
                let target = (blocks[i].end as i32 + *offset) as usize;
                edges.push((i, target));
                if next_idx < instructions.len() {
                    edges.push((i, next_idx));
                }
            }
            Ret { .. } | RetNull | Panic { .. } => {}
            _ => {
                if next_idx < instructions.len() {
                    edges.push((i, next_idx));
                }
            }
        }
    }

    for (from, target_idx) in edges {
        if let Some(to) = blocks.iter().position(|b| b.start == target_idx) {
            let mut from_clone = blocks[from].successors.clone();
            from_clone.push(to);
            blocks[from].successors = from_clone;

            let mut to_clone = blocks[to].predecessors.clone();
            to_clone.push(from);
            blocks[to].predecessors = to_clone;
        }
    }
    blocks
}

pub fn instruction_def_use(
    instruction: &Instruction,
    defs: &mut Vec<Reg>,
    uses: &mut Vec<Reg>,
    use_ranges: &mut Vec<(Reg, u8)>,
) {
    use Instruction::*;

    match instruction {
        LoadConst { dest, .. }
        | LoadGlobal { dest, .. }
        | LoadNull { dest }
        | AllocLocal { dest, .. } => defs.push(*dest),

        Move { dest, src }
        | Copy { dest, src }
        | Len { dest, src }
        | Neg { dest, src }
        | Not { dest, src }
        | BitNot { dest, src }
        | Cast { dest, src, .. }
        | Instanceof { dest, src, .. } => {
            defs.push(*dest);
            uses.push(*src);
        }

        StoreGlobal { src, .. } | Ret { src } => uses.push(*src),

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
            defs.push(*dest);
            uses.push(*lhs);
            uses.push(*rhs);
        }

        BinaryImmediate { dest, lhs, .. } => {
            defs.push(*dest);
            uses.push(*lhs);
        }

        Fallback {
            dest,
            src,
            fallback,
        } => {
            defs.push(*dest);
            uses.push(*src);
            uses.push(*fallback);
        }

        Jump { .. } | RetNull | Panic { .. } => {}

        JumpTrue { cond, .. } | JumpFalse { cond, .. } => uses.push(*cond),
        JumpNull { val, .. } => uses.push(*val),

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
            defs.push(*dest);
            if *arg_count > 0 {
                use_ranges.push((*args_start, *arg_count));
            }
        }

        TailCall {
            args_start,
            arg_count,
            ..
        } => {
            if *arg_count > 0 {
                use_ranges.push((*args_start, *arg_count));
            }
        }

        CallMethod {
            dest,
            obj,
            args_start,
            arg_count,
            ..
        } => {
            defs.push(*dest);
            uses.push(*obj);
            if *arg_count > 1 {
                use_ranges.push((*args_start, *arg_count - 1));
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
            defs.push(*dest);
            uses.push(*func_reg);
            if *arg_count > 0 {
                use_ranges.push((*args_start, *arg_count));
            }
        }

        LoadField { dest, obj, .. } => {
            defs.push(*dest);
            uses.push(*obj);
        }

        StoreField { obj, val, .. } => {
            uses.push(*obj);
            uses.push(*val);
        }

        NewArray { dest, len_reg, .. } => {
            defs.push(*dest);
            uses.push(*len_reg);
        }

        LoadIndex { dest, arr, idx } => {
            defs.push(*dest);
            uses.push(*arr);
            uses.push(*idx);
        }

        StoreIndex { arr, idx, val } => {
            uses.push(*arr);
            uses.push(*idx);
            uses.push(*val);
        }

        NewTuple {
            dest, start, count, ..
        } => {
            defs.push(*dest);
            if *count > 0 {
                use_ranges.push((*start, *count));
            }
        }

        NewChoice { dest, payload, .. } => {
            defs.push(*dest);
            uses.push(*payload);
        }

        Drop { reg } => uses.push(*reg),

        AwaitFuture {
            dest, future_id, ..
        } => {
            defs.push(*dest);
            uses.push(*future_id);
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
            defs.push(*dest);
            if *count > 0 {
                use_ranges.push((*futures_start, *count));
            }
        }

        CopyArray {
            dest,
            dest_start,
            src,
        } => {
            uses.push(*dest); // dest array is modified, but its register value (reference) is used
            uses.push(*dest_start);
            uses.push(*src);
        }
    }
}

pub fn compute_liveness(
    blocks: &mut [BasicBlock],
    instructions: &[Instruction],
    register_count: usize,
) {
    let mut changed = true;
    let num_blocks = blocks.len();

    while changed {
        changed = false;

        for i in (0..num_blocks).rev() {
            let mut new_live_out = vec![false; register_count];
            for &succ_idx in &blocks[i].successors {
                for (reg, live_out) in new_live_out.iter_mut().enumerate() {
                    if blocks[succ_idx].live_in[reg] {
                        *live_out = true;
                    }
                }
            }

            blocks[i].live_out = new_live_out.clone();

            let mut live = new_live_out;

            let mut defs = Vec::new();
            let mut uses = Vec::new();
            let mut use_ranges = Vec::new();
            for inst_idx in (blocks[i].start..blocks[i].end).rev() {
                let inst = &instructions[inst_idx];

                defs.clear();
                uses.clear();
                use_ranges.clear();
                instruction_def_use(inst, &mut defs, &mut uses, &mut use_ranges);

                for &reg in &defs {
                    live[reg.raw() as usize] = false;
                }

                for &reg in &uses {
                    live[reg.raw() as usize] = true;
                }

                for &(start, count) in &use_ranges {
                    for j in 0..count {
                        live[(start.raw() + j as u16) as usize] = true;
                    }
                }
            }

            if live != blocks[i].live_in {
                blocks[i].live_in = live;
                changed = true;
            }
        }
    }
}

pub fn compute_intervals(
    blocks: &[BasicBlock],
    instructions: &[Instruction],
    register_count: usize,
) -> Vec<Option<(usize, usize)>> {
    let mut intervals: Vec<Option<(usize, usize)>> = vec![None; register_count];

    // A register's live interval spans from its first definition or use to its last use or definition.
    // However, because CFG can be complex (loops), a register live at the end of a block is live
    // for the entire block if not defined in it.

    // Simplest approach: compute global first and last instruction index where a register is live.
    for block in blocks {
        let mut live = block.live_out.clone();

        let mut update_interval = |reg: usize, inst_idx: usize| {
            if let Some((start, end)) = intervals[reg] {
                intervals[reg] = Some((start.min(inst_idx), end.max(inst_idx)));
            } else {
                intervals[reg] = Some((inst_idx, inst_idx));
            }
        };

        // If it's live out, it must be alive at the block end
        for (reg, is_live) in live.iter().enumerate() {
            if *is_live {
                update_interval(reg, block.end.saturating_sub(1));
            }
        }

        let mut defs = Vec::new();
        let mut uses = Vec::new();
        let mut use_ranges = Vec::new();

        for inst_idx in (block.start..block.end).rev() {
            let inst = &instructions[inst_idx];

            defs.clear();
            uses.clear();
            use_ranges.clear();
            instruction_def_use(inst, &mut defs, &mut uses, &mut use_ranges);

            for &reg in &defs {
                let r = reg.raw() as usize;
                live[r] = false;
                update_interval(r, inst_idx);
            }

            for &reg in &uses {
                let r = reg.raw() as usize;
                live[r] = true;
                update_interval(r, inst_idx);
            }

            for &(start, count) in &use_ranges {
                for j in 0..count {
                    let r = (start.raw() + j as u16) as usize;
                    live[r] = true;
                    update_interval(r, inst_idx);
                }
            }

            for (reg, is_live) in live.iter().enumerate() {
                if *is_live {
                    update_interval(reg, inst_idx);
                }
            }
        }
    }

    intervals
}
