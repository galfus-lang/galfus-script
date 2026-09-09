use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::Reg;
use galfus_ir::mir::{Constant as MirConstant, Operand};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub fn operand_reg(&mut self, operand: &Operand) -> Reg {
        match operand {
            Operand::Local(local_id) => Reg(local_id.raw() as u16),
            Operand::ConstRef(index) => {
                let constant = &self.ctx.mir_constants[*index];
                let temp = self.alloc_temp();
                match constant {
                    MirConstant::Null => {
                        self.instructions.push(Instruction::LoadNull { dest: temp })
                    }
                    _ => self.load_constant(temp, constant),
                }
                temp
            }
            Operand::Constant(constant) => {
                let temp = self.alloc_temp();
                match constant {
                    MirConstant::Null => {
                        self.instructions.push(Instruction::LoadNull { dest: temp })
                    }
                    _ => self.load_constant(temp, constant),
                }
                temp
            }
        }
    }

    pub fn load_operand_to(&mut self, operand: &Operand, dest: Reg) {
        match operand {
            Operand::Local(local_id) => {
                let src = Reg(local_id.raw() as u16);
                if src != dest {
                    self.instructions.push(Instruction::Move { dest, src });
                }
            }
            Operand::ConstRef(index) => {
                let constant = &self.ctx.mir_constants[*index];
                match constant {
                    MirConstant::Null => self.instructions.push(Instruction::LoadNull { dest }),
                    _ => self.load_constant(dest, constant),
                }
            }
            Operand::Constant(constant) => match constant {
                MirConstant::Null => self.instructions.push(Instruction::LoadNull { dest }),
                _ => self.load_constant(dest, constant),
            },
        }
    }

    pub fn free_temp_if_operand(&mut self, operand: &Operand) {
        if matches!(operand, Operand::Constant(_)) {
            self.free_temps(1);
        }
    }
}
