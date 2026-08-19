use galfus_ir::mir::{Instruction, MirModule, Operand, Terminator};

pub fn optimize_tail_calls(module: &mut MirModule) {
    for func in &mut module.functions {
        for block in &mut func.blocks {
            let mut is_tco = false;
            let mut tail_call_data = None;
            let mut call_instr_index = None;

            if let Terminator::Return(ret_op) = &block.terminator.0 {
                // Search backwards for the Call
                for (i, instr) in block.instructions.iter().enumerate().rev() {
                    match &instr.0 {
                        Instruction::Drop(_) => continue,
                        Instruction::Call {
                            func: call_func,
                            args,
                            destination,
                            is_external,
                        } => {
                            let can_tco = match ret_op {
                                Some(Operand::Local(ret_local)) => *ret_local == *destination,
                                None => true,
                                _ => false,
                            };

                            if can_tco {
                                is_tco = true;
                                tail_call_data = Some((*call_func, args.clone(), *is_external));
                                call_instr_index = Some(i);
                            }
                            break; // Stop at the first Call or any other instruction
                        }
                        _ => break, // Any other instruction breaks the tail-call position
                    }
                }
            }

            if is_tco && let Some((call_func, args, is_external)) = tail_call_data {
                let idx = call_instr_index.unwrap();
                // Remove the Call and everything after it
                block.instructions.truncate(idx);

                block.terminator.0 = Terminator::TailCall {
                    func: call_func,
                    args,
                    is_external,
                };
            }
        }
    }
}
