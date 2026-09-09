use std::collections::HashMap;

use super::function::{FnEmitter, JumpKind};
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::Reg;
use galfus_ir::mir::{BlockId, Constant as MirConstant, Terminator};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub(super) fn emit_terminator(
        &mut self,
        terminator: &Terminator,
        next_block: Option<BlockId>,
        block_labels: &HashMap<BlockId, usize>,
    ) {
        match terminator {
            Terminator::Return(operand) => {
                if let Some(operand) = operand {
                    let src = self.operand_reg(operand);
                    self.instructions.push(Instruction::Ret { src });
                    self.free_temp_if_operand(operand);
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
                    Reg(0)
                } else {
                    let reg = self.alloc_temp();
                    let mut temp_regs = vec![reg];
                    for _ in 1..args.len() {
                        temp_regs.push(self.alloc_temp());
                    }
                    for (index, argument) in args.iter().enumerate() {
                        self.load_operand_to(argument, temp_regs[index]);
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
            Terminator::Panic(message) => {
                let const_idx = crate::bytecode_emission::constants::get_or_create_constant(
                    self.ctx,
                    &MirConstant::String(message.clone()),
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
                if true_args.is_empty() && false_args.is_empty() && next_block == Some(*true_block)
                {
                    self.emit_jump(block_labels[false_block], JumpKind::IfFalse(cond_reg));
                } else if true_args.is_empty() {
                    self.emit_jump(block_labels[true_block], JumpKind::IfTrue(cond_reg));
                    let false_target_params = self.target_params(*false_block);
                    self.emit_parallel_copies(&false_target_params, false_args);
                    if next_block != Some(*false_block) {
                        self.emit_jump(block_labels[false_block], JumpKind::Unconditional);
                    }
                } else if false_args.is_empty() {
                    self.emit_jump(block_labels[false_block], JumpKind::IfFalse(cond_reg));
                    let true_target_params = self.target_params(*true_block);
                    self.emit_parallel_copies(&true_target_params, true_args);
                    if next_block != Some(*true_block) {
                        self.emit_jump(block_labels[true_block], JumpKind::Unconditional);
                    }
                } else {
                    let true_trampoline = self.new_label();
                    self.emit_jump(true_trampoline, JumpKind::IfTrue(cond_reg));
                    let false_target_params = self.target_params(*false_block);
                    self.emit_parallel_copies(&false_target_params, false_args);
                    self.emit_jump(block_labels[false_block], JumpKind::Unconditional);
                    self.emit_label(true_trampoline);
                    let true_target_params = self.target_params(*true_block);
                    self.emit_parallel_copies(&true_target_params, true_args);
                    self.emit_jump(block_labels[true_block], JumpKind::Unconditional);
                }
                self.free_temp_if_operand(cond);
            }
        }
    }
}
