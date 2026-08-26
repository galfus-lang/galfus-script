use std::collections;
use std::mem;

use galfus_ir::mir;

use super::LowerCtx;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::{GlobalIdx, Reg};
use galfus_ir::mir::{
    Constant as MirConstant, Instruction as MirInstruction, MirFunction, Operand, Terminator,
};

#[allow(dead_code)]
pub enum JumpKind {
    Unconditional,
    IfTrue(Reg),
    IfFalse(Reg),
}

pub struct FnEmitter<'a, 'b> {
    pub ctx: &'b mut LowerCtx<'a>,
    pub func: &'a MirFunction,
    pub param_count: u16,
    pub local_count: u16,
    pub instructions: Vec<Instruction>,
    pub temp_count_current: u16,
    pub temp_count_max: u16,
    next_label_id: usize,
    label_pcs: collections::HashMap<usize, usize>,
    pending_jumps: Vec<(usize, usize, JumpKind)>,
    direct_await_candidates: collections::HashMap<Reg, Box<str>>,
    pub instruction_spans: collections::HashMap<usize, galfus_core::Span>,
}

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub fn new(
        ctx: &'b mut LowerCtx<'a>,
        func: &'a MirFunction,
        param_count: u16,
        local_count: u16,
    ) -> Self {
        Self {
            ctx,
            func,
            param_count,
            local_count,
            instructions: Vec::new(),
            temp_count_current: 0,
            temp_count_max: 0,
            next_label_id: 0,
            label_pcs: collections::HashMap::new(),
            pending_jumps: Vec::new(),
            direct_await_candidates: collections::HashMap::new(),
            instruction_spans: collections::HashMap::new(),
        }
    }

    pub fn alloc_temp(&mut self) -> Reg {
        let id = self.param_count + self.local_count + self.temp_count_current;
        self.temp_count_current += 1;
        if self.temp_count_current > self.temp_count_max {
            self.temp_count_max = self.temp_count_current;
        }
        Reg(id)
    }

    pub fn free_temps(&mut self, count: u16) {
        self.temp_count_current = self.temp_count_current.saturating_sub(count);
    }

    fn emit_parallel_copies(&mut self, dests: &[Reg], srcs: &[Operand]) {
        assert_eq!(dests.len(), srcs.len());
        if dests.is_empty() {
            return;
        }

        let temps_before = self.temp_count_current;

        let mut src_regs = Vec::new();
        for src in srcs {
            let reg = match src {
                Operand::Local(loc) => Reg(loc.raw() as u16),
                _ => {
                    let temp = self.alloc_temp();
                    self.load_operand_to(src, temp);
                    temp
                }
            };
            src_regs.push(reg);
        }

        let mut in_degree = collections::BTreeMap::new();
        let mut edges = collections::BTreeMap::new();

        for i in 0..dests.len() {
            let d = dests[i];
            let s = src_regs[i];
            if d != s {
                edges.insert(d, s);
                *in_degree.entry(s).or_insert(0) += 1;
                in_degree.entry(d).or_insert(0);
            }
        }

        let mut ready = collections::BTreeSet::new();
        for (node, deg) in &in_degree {
            if *deg == 0 && edges.contains_key(node) {
                ready.insert(*node);
            }
        }

        while !edges.is_empty() {
            if let Some(d) = ready.pop_first() {
                let s = edges.remove(&d).unwrap();
                self.instructions
                    .push(Instruction::Move { dest: d, src: s });

                let deg = in_degree.get_mut(&s).unwrap();
                *deg -= 1;
                if *deg == 0 && edges.contains_key(&s) {
                    ready.insert(s);
                }
            } else {
                let d = *edges.keys().next().unwrap();
                let s = edges.remove(&d).unwrap();

                let temp = self.alloc_temp();
                self.instructions
                    .push(Instruction::Move { dest: temp, src: s });

                edges.insert(d, temp);
                in_degree.insert(temp, 1);

                let deg = in_degree.get_mut(&s).unwrap();
                *deg -= 1;
                if *deg == 0 && edges.contains_key(&s) {
                    ready.insert(s);
                }
            }
        }

        self.temp_count_current = temps_before;
    }

    fn target_params(&self, target: mir::BlockId) -> Vec<Reg> {
        self.func
            .blocks
            .iter()
            .find(|block| block.id == target)
            .expect("MIR terminator references a missing block")
            .parameters
            .iter()
            .map(|param| Reg(param.id.raw() as u16))
            .collect()
    }

    pub fn new_label(&mut self) -> usize {
        let id = self.next_label_id;
        self.next_label_id += 1;
        id
    }

    pub fn emit_label(&mut self, label: usize) {
        let pc = self.instructions.len();
        self.label_pcs.insert(label, pc);
    }

    pub fn emit_jump(&mut self, target_label: usize, kind: JumpKind) {
        let pc = self.instructions.len();
        self.pending_jumps.push((pc, target_label, kind));
        self.instructions.push(Instruction::RetNull);
    }

    pub fn emit(
        &mut self,
    ) -> (
        Vec<Instruction>,
        collections::HashMap<usize, galfus_core::Span>,
    ) {
        let mut block_labels = collections::HashMap::new();
        for bb in &self.func.blocks {
            block_labels.insert(bb.id, self.new_label());
        }

        for bb in &self.func.blocks {
            let label = block_labels[&bb.id];
            self.emit_label(label);

            for (inst, span_opt) in &bb.instructions {
                let initial_pc = self.instructions.len();
                match inst {
                    MirInstruction::Assign(dest, rvalue) => {
                        self.emit_rvalue(Reg(dest.raw() as u16), rvalue);
                    }
                    MirInstruction::Drop(local) => {
                        self.instructions.push(Instruction::Drop {
                            reg: Reg(local.raw() as u16),
                        });
                    }
                    MirInstruction::StoreGlobal(_name, op) => {
                        let global_idx = 0;
                        let val_reg = self.operand_reg(op);
                        self.instructions.push(Instruction::StoreGlobal {
                            module_id: galfus_core::ModuleId::new(0),
                            global_idx: GlobalIdx(global_idx as u16),
                            src: val_reg,
                        });
                        self.free_temp_if_operand(op);
                    }
                    MirInstruction::StoreIndex { arr, idx, val } => {
                        let arr_reg = self.operand_reg(arr);
                        let idx_reg = self.operand_reg(idx);
                        let val_reg = self.operand_reg(val);

                        self.instructions.push(Instruction::StoreIndex {
                            arr: arr_reg,
                            idx: idx_reg,
                            val: val_reg,
                        });

                        self.free_temp_if_operand(val);
                        self.free_temp_if_operand(idx);
                        self.free_temp_if_operand(arr);
                    }
                    MirInstruction::StoreField {
                        obj,
                        field_name,
                        val,
                    } => {
                        let obj_reg = self.operand_reg(obj);
                        let val_reg = self.operand_reg(val);
                        let field = self.field_idx_for_member(obj, field_name);

                        self.instructions.push(Instruction::StoreField {
                            obj: obj_reg,
                            field,
                            val: val_reg,
                        });

                        self.free_temp_if_operand(val);
                        self.free_temp_if_operand(obj);
                    }

                    MirInstruction::Call {
                        func,
                        args,
                        destination,
                        is_external,
                    } => {
                        let builtin_name = self.ctx.function_names.get(func).map(|s| s.to_string());
                        if let Some(_name) = builtin_name {
                            let native_math_name = _name
                                .rsplit("::")
                                .next()
                                .filter(|name| name.starts_with("__internal_math_"));
                            if let Some(native_math_name) = native_math_name {
                                let start_reg = if args.is_empty() {
                                    Reg(0)
                                } else {
                                    let reg = self.alloc_temp();
                                    let mut temp_regs = vec![reg];
                                    for _ in 1..args.len() {
                                        temp_regs.push(self.alloc_temp());
                                    }
                                    for (i, arg_op) in args.iter().enumerate() {
                                        self.load_operand_to(arg_op, temp_regs[i]);
                                    }
                                    reg
                                };
                                let operation = match native_math_name {
                                    "__internal_math_is_nan" => 0,
                                    "__internal_math_is_finite" => 1,
                                    "__internal_math_is_infinite" => 2,
                                    "__internal_math_sqrt" => 3,
                                    "__internal_math_hypot" => 4,
                                    "__internal_math_sin" => 5,
                                    "__internal_math_cos" => 6,
                                    "__internal_math_tan" => 7,
                                    "__internal_math_log" => 8,
                                    "__internal_math_log2" => 9,
                                    "__internal_math_log10" => 10,
                                    _ => unreachable!("recognized math intrinsic"),
                                };
                                self.instructions.push(Instruction::CallInternalMath {
                                    dest: Reg(destination.raw() as u16),
                                    operation,
                                    args_start: start_reg,
                                    arg_count: args.len() as u8,
                                });
                                if !args.is_empty() {
                                    self.free_temps(args.len() as u16);
                                }
                                continue;
                            }
                            let native_thread_name = _name.rsplit("::").next().filter(|name| {
                                matches!(
                                    *name,
                                    "__internal_thread_get"
                                        | "__internal_thread_is_running"
                                        | "__internal_thread_is_exited"
                                        | "__internal_thread_exit_reason"
                                        | "__internal_thread_send"
                                        | "__internal_thread_has_messages"
                                        | "__internal_thread_get_message"
                                        | "__internal_thread_try_receive"
                                )
                            });
                            if let Some(operation) = native_thread_name {
                                let start_reg = if args.is_empty() {
                                    Reg(0)
                                } else {
                                    let reg = self.alloc_temp();
                                    let mut temp_regs = vec![reg];
                                    for _ in 1..args.len() {
                                        temp_regs.push(self.alloc_temp());
                                    }
                                    for (i, arg_op) in args.iter().enumerate() {
                                        self.load_operand_to(arg_op, temp_regs[i]);
                                    }
                                    reg
                                };
                                let return_type = self
                                    .func
                                    .locals
                                    .iter()
                                    .find(|local| local.id == *destination)
                                    .expect("native call destination must be a local")
                                    .ty;
                                let return_type =
                                    match self.ctx.type_result.layer().table().kind(return_type) {
                                        Some(galfus_frontend::TypeKind::GenericInstance {
                                            arguments,
                                            ..
                                        }) => arguments.first().copied().unwrap_or(return_type),
                                        _ => return_type,
                                    };
                                let arg_types = args
                                    .iter()
                                    .map(|argument| {
                                        let ty = self.get_operand_type(argument);
                                        crate::bytecode_emission::types::lower_type(self.ctx, ty)
                                    })
                                    .collect();
                                self.instructions.push(Instruction::CallInternalThread {
                                    dest: Reg(destination.raw() as u16),
                                    operation: operation.into(),
                                    args_start: start_reg,
                                    arg_count: args.len() as u8,
                                    arg_types,
                                    return_type: crate::bytecode_emission::types::lower_type(
                                        self.ctx,
                                        return_type,
                                    ),
                                });
                                self.direct_await_candidates
                                    .insert(Reg(destination.raw() as u16), operation.into());
                                if !args.is_empty() {
                                    self.free_temps(args.len() as u16);
                                }
                                continue;
                            }
                            let native_async_name = _name
                                .rsplit("::")
                                .next()
                                .filter(|name| *is_external || name.starts_with("__internal_"));

                            if native_async_name.is_some() {
                                let start_reg = if args.is_empty() {
                                    Reg(0) // Dummy if no args
                                } else {
                                    let reg = self.alloc_temp();
                                    let mut temp_regs = vec![reg];
                                    for _ in 1..args.len() {
                                        temp_regs.push(self.alloc_temp());
                                    }

                                    for (i, arg_op) in args.iter().enumerate() {
                                        self.load_operand_to(arg_op, temp_regs[i]);
                                    }
                                    reg
                                };

                                let return_type = self
                                    .func
                                    .locals
                                    .iter()
                                    .find(|local| local.id == *destination)
                                    .expect("native call destination must be a local")
                                    .ty;
                                let return_type =
                                    match self.ctx.type_result.layer().table().kind(return_type) {
                                        Some(galfus_frontend::TypeKind::GenericInstance {
                                            arguments,
                                            ..
                                        }) => arguments.first().copied().unwrap_or(return_type),
                                        _ => return_type,
                                    };
                                let arg_types = args
                                    .iter()
                                    .map(|argument| {
                                        let is_string_const = match argument {
                                            mir::Operand::ConstRef(idx) => matches!(
                                                self.ctx.mir_constants.get(*idx),
                                                Some(mir::Constant::String(_))
                                            ),
                                            mir::Operand::Constant(mir::Constant::String(_)) => {
                                                true
                                            }
                                            _ => false,
                                        };
                                        if is_string_const {
                                            let u8_ty =
                                                self.ctx.type_result.layer().table().primitive(
                                                    galfus_frontend::PrimitiveType::Uint8,
                                                );
                                            let u8_idx =
                                                crate::bytecode_emission::types::lower_type(
                                                    self.ctx, u8_ty,
                                                );
                                            let type_idx = galfus_bytecode::instruction::TypeIdx(
                                                self.ctx.types.len() as u16,
                                            );
                                            self.ctx
                                                .types
                                                .push(galfus_bytecode::BytecodeType::Array(u8_idx));
                                            type_idx
                                        } else {
                                            let ty = self.get_operand_type(argument);
                                            crate::bytecode_emission::types::lower_type(
                                                self.ctx, ty,
                                            )
                                        }
                                    })
                                    .collect();
                                let return_type = crate::bytecode_emission::types::lower_type(
                                    self.ctx,
                                    return_type,
                                );

                                let func_idx = *self.ctx.function_map.get(func).unwrap_or_else(|| panic!("missing lowered function mapping for {:?} while emitting {} ({:?})",
                                    func, self.func.name, self.func.id));
                                self.instructions.push(Instruction::CreateFuture {
                                    dest: Reg(destination.raw() as u16),
                                    func: func_idx,
                                    args_start: start_reg,
                                    arg_count: args.len() as u8,
                                    arg_types,
                                    return_type,
                                });
                                if let Some(name) =
                                    native_async_name.filter(|name| name.starts_with("__internal_"))
                                {
                                    self.direct_await_candidates
                                        .insert(Reg(destination.raw() as u16), name.into());
                                }
                                if !args.is_empty() {
                                    self.free_temps(args.len() as u16);
                                }
                                continue;
                            }
                        }

                        let start_reg = self.alloc_temp();
                        let mut temp_regs = vec![start_reg];
                        for _ in 1..args.len() {
                            temp_regs.push(self.alloc_temp());
                        }

                        for (i, arg_op) in args.iter().enumerate() {
                            self.load_operand_to(arg_op, temp_regs[i]);
                        }

                        let func_idx = *self.ctx.function_map.get(func).unwrap_or_else(|| panic!("missing lowered function mapping for {:?} while emitting {} ({:?})",
                            func, self.func.name, self.func.id));
                        self.instructions.push(Instruction::Call {
                            dest: Reg(destination.raw() as u16),
                            func: func_idx,
                            args_start: start_reg,
                            arg_count: args.len() as u8,
                        });

                        self.free_temps(args.len() as u16);
                    }
                    MirInstruction::ConstraintCall {
                        method_name,
                        obj,
                        args,
                        destination,
                    } => {
                        let obj_reg = self.alloc_temp();
                        self.load_operand_to(obj, obj_reg);

                        let mut extra_regs: Vec<Reg> = Vec::with_capacity(args.len());
                        for _ in 0..args.len() {
                            extra_regs.push(self.alloc_temp());
                        }
                        for (i, arg_op) in args.iter().enumerate() {
                            self.load_operand_to(arg_op, extra_regs[i]);
                        }

                        let name_const =
                            crate::bytecode_emission::constants::get_or_create_constant(
                                self.ctx,
                                &MirConstant::String(method_name.clone()),
                            );

                        self.instructions.push(Instruction::CallMethod {
                            dest: Reg(destination.raw() as u16),
                            obj: obj_reg,
                            name_const,
                            args_start: obj_reg,
                            arg_count: (1 + args.len()) as u8,
                        });

                        self.free_temps(1 + extra_regs.len() as u16);
                    }
                    MirInstruction::IndirectCall {
                        func,
                        args,
                        destination,
                    } => {
                        let func_reg = self.alloc_temp();
                        self.load_operand_to(func, func_reg);

                        let start_reg = self.alloc_temp();
                        let mut temp_regs = vec![start_reg];
                        for _ in 1..args.len() {
                            temp_regs.push(self.alloc_temp());
                        }

                        for (i, arg_op) in args.iter().enumerate() {
                            self.load_operand_to(arg_op, temp_regs[i]);
                        }

                        self.instructions.push(Instruction::CallDynamic {
                            dest: Reg(destination.raw() as u16),
                            func_reg,
                            args_start: start_reg,
                            arg_count: args.len() as u8,
                        });

                        self.free_temps(1 + args.len() as u16);
                        self.free_temp_if_operand(func);
                    }
                    MirInstruction::Await {
                        future,
                        destination,
                    } => {
                        let fut_reg = self.operand_reg(future);
                        let payload_type = self
                            .func
                            .locals
                            .iter()
                            .find(|local| local.id == *destination)
                            .expect("await destination must be a local")
                            .ty;
                        let return_type =
                            crate::bytecode_emission::types::lower_type(self.ctx, payload_type);
                        if let Some(Instruction::CallInternalThread { dest, .. }) =
                            self.instructions.last_mut()
                            && *dest == fut_reg
                        {
                            *dest = Reg(destination.raw() as u16);
                            self.direct_await_candidates.remove(&fut_reg);
                            continue;
                        }
                        if let Some(operation) = self.direct_await_candidates.remove(&fut_reg)
                            && let Some(Instruction::CreateFuture {
                                dest,
                                func: _,
                                args_start,
                                arg_count,
                                arg_types,
                                return_type: future_return_type,
                            }) = self.instructions.last_mut()
                            && *dest == fut_reg
                            && *future_return_type == return_type
                        {
                            let args_start = *args_start;
                            let arg_count = *arg_count;
                            let arg_types = std::mem::take(arg_types);
                            *self
                                .instructions
                                .last_mut()
                                .expect("future instruction exists") =
                                Instruction::CreateAwaitFuture {
                                    dest: Reg(destination.raw() as u16),
                                    operation,
                                    args_start,
                                    arg_count,
                                    arg_types,
                                    return_type,
                                };
                            continue;
                        }
                        self.instructions.push(Instruction::AwaitFuture {
                            dest: Reg(destination.raw() as u16),
                            future_id: fut_reg,
                            return_type,
                        });
                        self.free_temp_if_operand(future);
                    }
                    MirInstruction::AwaitAll {
                        futures,
                        destination,
                    } => {
                        let payload_type = self
                            .func
                            .locals
                            .iter()
                            .find(|local| local.id == *destination)
                            .expect("await(all) destination must be a local")
                            .ty;
                        let return_type =
                            crate::bytecode_emission::types::lower_type(self.ctx, payload_type);
                        let start_reg = if futures.is_empty() {
                            Reg(0)
                        } else {
                            let first = self.alloc_temp();
                            let mut temp_regs = vec![first];
                            for _ in 1..futures.len() {
                                temp_regs.push(self.alloc_temp());
                            }
                            for (i, fut_op) in futures.iter().enumerate() {
                                self.load_operand_to(fut_op, temp_regs[i]);
                            }
                            first
                        };
                        self.instructions.push(Instruction::AwaitAll {
                            dest: Reg(destination.raw() as u16),
                            futures_start: start_reg,
                            count: futures.len() as u8,
                            return_type,
                        });
                        if !futures.is_empty() {
                            self.free_temps(futures.len() as u16);
                        }
                    }
                    MirInstruction::AwaitRace {
                        futures,
                        destination,
                    } => {
                        let payload_type = self
                            .func
                            .locals
                            .iter()
                            .find(|local| local.id == *destination)
                            .expect("await(race) destination must be a local")
                            .ty;
                        let return_type =
                            crate::bytecode_emission::types::lower_type(self.ctx, payload_type);
                        let start_reg = if futures.is_empty() {
                            Reg(0)
                        } else {
                            let first = self.alloc_temp();
                            let mut temp_regs = vec![first];
                            for _ in 1..futures.len() {
                                temp_regs.push(self.alloc_temp());
                            }
                            for (i, fut_op) in futures.iter().enumerate() {
                                self.load_operand_to(fut_op, temp_regs[i]);
                            }
                            first
                        };
                        self.instructions.push(Instruction::AwaitRace {
                            dest: Reg(destination.raw() as u16),
                            futures_start: start_reg,
                            count: futures.len() as u8,
                            return_type,
                        });
                        if !futures.is_empty() {
                            self.free_temps(futures.len() as u16);
                        }
                    }
                }
                if let Some(span) = span_opt {
                    for pc in initial_pc..self.instructions.len() {
                        self.instruction_spans.insert(pc, *span);
                    }
                }
            }

            let initial_pc = self.instructions.len();
            match &bb.terminator.0 {
                Terminator::Return(opt_operand) => {
                    if let Some(op) = opt_operand {
                        let src = self.operand_reg(op);
                        self.instructions.push(Instruction::Ret { src });
                        self.free_temp_if_operand(op);
                    } else {
                        self.instructions.push(Instruction::RetNull);
                    }
                }
                Terminator::TailCall {
                    func,
                    args,
                    is_external: _,
                } => {
                    let start_reg = if args.is_empty() {
                        Reg(0) // Dummy if no args
                    } else {
                        let reg = self.alloc_temp();
                        let mut temp_regs = vec![reg];
                        for _ in 1..args.len() {
                            temp_regs.push(self.alloc_temp());
                        }
                        for (i, arg_op) in args.iter().enumerate() {
                            self.load_operand_to(arg_op, temp_regs[i]);
                        }
                        reg
                    };

                    let func_idx = *self.ctx.function_map.get(func).unwrap_or_else(|| {
                        panic!(
                            "missing lowered function mapping for {:?} while emitting {} ({:?})",
                            func, self.func.name, self.func.id
                        )
                    });

                    self.instructions.push(Instruction::TailCall {
                        func: func_idx,
                        args_start: start_reg,
                        arg_count: args.len() as u8,
                    });

                    if !args.is_empty() {
                        self.free_temps(args.len() as u16);
                    }
                }

                Terminator::Panic(msg) => {
                    let const_idx = crate::bytecode_emission::constants::get_or_create_constant(
                        self.ctx,
                        &MirConstant::String(msg.clone()),
                    );
                    self.instructions.push(Instruction::Panic { const_idx });
                }
                Terminator::Jump { target, args } => {
                    let target_params = self.target_params(*target);
                    self.emit_parallel_copies(&target_params, args);
                    self.emit_jump(block_labels[target], JumpKind::Unconditional);
                }
                Terminator::Branch {
                    cond,
                    true_block,
                    true_args,
                    false_block,
                    false_args,
                } => {
                    let cond_reg = self.operand_reg(cond);

                    let true_trampoline = self.new_label();
                    self.emit_jump(true_trampoline, JumpKind::IfTrue(cond_reg));

                    let false_target_params = self.target_params(*false_block);
                    self.emit_parallel_copies(&false_target_params, false_args);
                    self.emit_jump(block_labels[false_block], JumpKind::Unconditional);

                    self.emit_label(true_trampoline);
                    let true_target_params = self.target_params(*true_block);
                    self.emit_parallel_copies(&true_target_params, true_args);
                    self.emit_jump(block_labels[true_block], JumpKind::Unconditional);

                    self.free_temp_if_operand(cond);
                }
            }
            if let Some(span) = &bb.terminator.1 {
                for pc in initial_pc..self.instructions.len() {
                    self.instruction_spans.insert(pc, *span);
                }
            }
        }

        for (pc, target_label, kind) in &self.pending_jumps {
            let target_pc = self.label_pcs[target_label];
            let offset = target_pc as i32 - (*pc as i32 + 1);
            let patched_instr = match kind {
                JumpKind::Unconditional => Instruction::Jump { offset },
                JumpKind::IfTrue(cond) => Instruction::JumpTrue {
                    cond: *cond,
                    offset,
                },
                JumpKind::IfFalse(cond) => Instruction::JumpFalse {
                    cond: *cond,
                    offset,
                },
            };
            self.instructions[*pc] = patched_instr;
        }

        (
            mem::take(&mut self.instructions),
            mem::take(&mut self.instruction_spans),
        )
    }
}
