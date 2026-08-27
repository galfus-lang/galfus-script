use galfus_bytecode::{Instruction, Reg, BytecodeFunction};


pub fn allocate_registers(func: &mut BytecodeFunction, intervals: &[Option<(usize, usize)>], register_count: usize) {
    let param_count = func.param_count as usize;
    let mut remap = vec![None; register_count];
    
    // Pin parameters
    for i in 0..param_count {
        remap[i] = Some(Reg(i as u16));
    }
    
    let mut pinned = vec![false; register_count];
    for i in 0..param_count {
        pinned[i] = true;
    }
    
    // Identify contiguous windows
    for inst in &func.instructions {
        let mut defs = Vec::new();
        let mut uses = Vec::new();
        let mut use_ranges = Vec::new();
        use super::liveness::instruction_def_use;
        instruction_def_use(inst, &mut defs, &mut uses, &mut use_ranges);
        for &(start, count) in &use_ranges {
            if count > 1 {
                for i in 0..count {
                    let reg = (start.raw() + i as u16) as usize;
                    pinned[reg] = true;
                    remap[reg] = Some(Reg(reg as u16));
                }
            }
        }
    }
    
    let mut phys_reg_intervals: Vec<Vec<(usize, usize)>> = vec![Vec::new(); register_count];
    let mut max_phys_reg = param_count as u16;
    
    for r in 0..register_count {
        if pinned[r] {
            max_phys_reg = max_phys_reg.max(r as u16 + 1);
            if let Some(span) = intervals[r] {
                phys_reg_intervals[r].push(span);
            }
        }
    }
    
    let mut sorted_intervals: Vec<(usize, (usize, usize))> = intervals
        .iter()
        .enumerate()
        .filter_map(|(i, &int)| int.map(|span| (i, span)))
        .filter(|&(i, _)| !pinned[i])
        .collect();
        
    sorted_intervals.sort_by_key(|&(_, span)| span.0);
    
    let allocate = |span: (usize, usize), phys_reg_intervals: &mut Vec<Vec<(usize, usize)>>, max_phys_reg: &mut u16| -> u16 {
        let mut r = param_count;
        loop {
            if r >= phys_reg_intervals.len() {
                phys_reg_intervals.resize(r + 1, Vec::new());
            }
            if r >= phys_reg_intervals.len() {
                phys_reg_intervals.push(vec![span]);
                *max_phys_reg = (*max_phys_reg).max(r as u16 + 1);
                return r as u16;
            }
            
            let is_free = phys_reg_intervals[r].iter().all(|&existing| {
                existing.1 < span.0 || existing.0 > span.1
            });
            
            if is_free {
                phys_reg_intervals[r].push(span);
                *max_phys_reg = (*max_phys_reg).max(r as u16 + 1);
                return r as u16;
            }
            r += 1;
        }
    };
    
    for &(old_reg, span) in &sorted_intervals {
        if remap[old_reg].is_none() {
            let phys = allocate(span, &mut phys_reg_intervals, &mut max_phys_reg);
            remap[old_reg] = Some(Reg(phys));
        }
    }
    
    // For any register that has NO interval (completely dead), we still need to map it if it's used as a dummy.
    // We map it to itself.
    for r in 0..register_count {
        if remap[r].is_none() {
            remap[r] = Some(Reg(r as u16));
        }
    }
    
    for instruction in &mut func.instructions {
        super::remap_instruction_registers(instruction, &remap);
    }
    
    func.local_count = max_phys_reg.saturating_sub(param_count as u16);
    func.temp_count = 0;
}
