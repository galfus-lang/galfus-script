use std::collections;
use std::mem;

use galfus_ir::mir;

use super::LowerCtx;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::{GlobalIdx, ImmediateValue, Reg};
use galfus_ir::mir::{Constant as MirConstant, Instruction as MirInstruction, MirFunction};

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
    pub(super) direct_await_candidates: collections::HashMap<Reg, Box<str>>,
    pub(super) direct_galfus_await_candidates: collections::HashSet<Reg>,
    pub(super) known_immediates: collections::HashMap<Reg, ImmediateValue>,
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
            direct_galfus_await_candidates: collections::HashSet::new(),
            known_immediates: collections::HashMap::new(),
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

    fn is_future_type(&self, ty: galfus_core::TypeId) -> bool {
        match self.ctx.type_result.layer().table().kind(ty) {
            Some(galfus_frontend::TypeKind::Named { symbol }) => {
                self.ctx
                    .graph
                    .resolution()
                    .and_then(|resolution| resolution.symbol(*symbol))
                    .and_then(|symbol| self.ctx.string_table.resolve(symbol.name()))
                    == Some("Future")
            }
            Some(galfus_frontend::TypeKind::Path { segments, .. }) => {
                segments.last().is_some_and(|segment| segment == "Future")
            }
            _ => false,
        }
    }

    pub(super) fn target_params(&self, target: mir::BlockId) -> Vec<Reg> {
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

        for (block_index, bb) in self.func.blocks.iter().enumerate() {
            let next_block = self.func.blocks.get(block_index + 1).map(|block| block.id);
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
                    MirInstruction::StoreGlobal(name, op) => {
                        let global_idx = self
                            .ctx
                            .graph
                            .resolution()
                            .and_then(|res| {
                                let name_id = self.ctx.string_table.get(name);
                                res.symbols()
                                    .iter()
                                    .find(|symbol| {
                                        name_id.is_some() && symbol.name() == name_id.unwrap()
                                    })
                                    .map(|symbol| symbol.id().raw() as u16)
                            })
                            .unwrap_or(0);
                        let val_reg = self.operand_reg(op);
                        self.instructions.push(Instruction::StoreGlobal {
                            module_id: galfus_core::ModuleId::new(0),
                            global_idx: GlobalIdx(global_idx),
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
                        if self.emit_call(func, args, *destination, *is_external) {
                            continue;
                        }
                    }
                    MirInstruction::ConstraintCall {
                        method_name,
                        obj,
                        args,
                        destination,
                        return_type: constraint_return_type,
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
                        let future_payload = match self
                            .ctx
                            .type_result
                            .layer()
                            .table()
                            .kind(*constraint_return_type)
                        {
                            Some(galfus_frontend::TypeKind::GenericInstance {
                                base,
                                arguments,
                            }) if self.is_future_type(*base) => arguments.first().copied(),
                            _ => None,
                        };

                        let mut arg_types = Vec::with_capacity(1 + args.len());
                        arg_types.push(crate::bytecode_emission::types::lower_type(
                            self.ctx,
                            self.get_operand_type(obj),
                        ));
                        arg_types.extend(args.iter().map(|argument| {
                            crate::bytecode_emission::types::lower_type(
                                self.ctx,
                                self.get_operand_type(argument),
                            )
                        }));

                        self.instructions.push(Instruction::CallMethod {
                            dest: Reg(destination.raw() as u16),
                            obj: obj_reg,
                            name_const,
                            args_start: obj_reg,
                            arg_count: (1 + args.len()) as u8,
                            arg_types: arg_types.into_boxed_slice(),
                            return_type: future_payload.map(|ty| {
                                crate::bytecode_emission::types::lower_type(self.ctx, ty)
                            }),
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
                        drop_future,
                    } => {
                        let fut_reg = self.operand_reg(future);
                        let payload_type = self
                            .func
                            .locals
                            .iter()
                            .find(|local| local.id == *destination)
                            .expect("await destination must be a local")
                            .ty;
                        let mut return_type =
                            crate::bytecode_emission::types::lower_type(self.ctx, payload_type);
                        if let Some(Instruction::CreateFuture {
                            dest,
                            return_type: future_return_type,
                            ..
                        }) = self.instructions.last()
                            && *dest == fut_reg
                        {
                            return_type = *future_return_type;
                        }
                        if let Some(Instruction::CallMethod {
                            dest,
                            return_type: method_return_type,
                            ..
                        }) = self.instructions.last_mut()
                            && *dest == fut_reg
                        {
                            *method_return_type = Some(return_type);
                        }
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
                        if self.direct_galfus_await_candidates.remove(&fut_reg)
                            && let Some(Instruction::CreateFuture {
                                dest,
                                func,
                                args_start,
                                arg_count,
                                ..
                            }) = self.instructions.last_mut()
                            && *dest == fut_reg
                        {
                            *self
                                .instructions
                                .last_mut()
                                .expect("future instruction exists") = Instruction::Call {
                                dest: Reg(destination.raw() as u16),
                                func: *func,
                                args_start: *args_start,
                                arg_count: *arg_count,
                            };
                            continue;
                        }
                        self.instructions.push(Instruction::AwaitFuture {
                            dest: Reg(destination.raw() as u16),
                            future_id: fut_reg,
                            return_type,
                        });
                        if *drop_future {
                            self.instructions.push(Instruction::Drop { reg: fut_reg });
                        }
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
            self.emit_terminator(&bb.terminator.0, next_block, &block_labels);
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
