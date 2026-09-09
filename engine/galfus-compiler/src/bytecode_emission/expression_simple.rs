use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::{GlobalIdx, Reg};
use galfus_core::{SymbolId, TypeId};
use galfus_ir::mir::{MirUnaryOp, Operand};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub(super) fn emit_use(&mut self, dest: Reg, operand: &Operand) {
        self.load_operand_to(operand, dest);

        if let Some(immediate) = self.immediate_value(operand) {
            self.known_immediates.insert(dest, immediate);
        }

        if let Operand::Constant(constant) = operand
            && self.is_numeric_constant(constant)
            && !self.constant_matches_destination_type(constant, dest)
            && let Some(local) = self
                .func
                .locals
                .iter()
                .find(|local| local.id.raw() as u16 == dest.raw())
        {
            let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, local.ty);
            self.instructions.push(Instruction::Cast {
                dest,
                src: dest,
                type_idx,
            });
        }
    }

    pub(super) fn emit_unary_operation(&mut self, dest: Reg, op: &MirUnaryOp, operand: &Operand) {
        let src = self.operand_reg(operand);
        let instr = match op {
            MirUnaryOp::Negate => Instruction::Neg { dest, src },
            MirUnaryOp::Not => Instruction::Not { dest, src },
            MirUnaryOp::BitwiseNot => Instruction::BitNot { dest, src },
        };
        self.instructions.push(instr);
        self.free_temp_if_operand(operand);
    }

    pub(super) fn emit_cast(&mut self, dest: Reg, operand: &Operand, ty: TypeId) {
        let immediate = self
            .immediate_value(operand)
            .and_then(|immediate| self.cast_immediate(immediate, ty));
        let src = self.operand_reg(operand);
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, ty);
        self.instructions.push(Instruction::Cast {
            dest,
            src,
            type_idx,
        });
        self.free_temp_if_operand(operand);
        if let Some(immediate) = immediate {
            self.known_immediates.insert(dest, immediate);
        }
    }

    pub(super) fn emit_copy(&mut self, dest: Reg, operand: &Operand) {
        let src = self.operand_reg(operand);
        self.instructions.push(Instruction::Copy { dest, src });
        self.free_temp_if_operand(operand);
    }

    pub(super) fn emit_instanceof(&mut self, dest: Reg, operand: &Operand, ty: TypeId) {
        let src = self.operand_reg(operand);
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, ty);
        self.instructions.push(Instruction::Instanceof {
            dest,
            src,
            type_idx,
        });
        self.free_temp_if_operand(operand);
    }

    pub(super) fn emit_choice_variant_is(
        &mut self,
        dest: Reg,
        operand: &Operand,
        variant: SymbolId,
    ) {
        let src = self.operand_reg(operand);
        let operand_ty = self.get_operand_type(operand);
        let type_idx = crate::bytecode_emission::types::lower_choice_variant_type(
            self.ctx, operand_ty, variant,
        );
        self.instructions.push(Instruction::Instanceof {
            dest,
            src,
            type_idx,
        });
        self.free_temp_if_operand(operand);
    }

    pub(super) fn emit_imported_choice_variant_is(
        &mut self,
        dest: Reg,
        operand: &Operand,
        choice_name: &str,
        variant_name: &str,
    ) {
        let src = self.operand_reg(operand);
        let operand_ty = self.get_operand_type(operand);
        let type_idx = crate::bytecode_emission::types::lower_imported_choice_variant_type(
            self.ctx,
            operand_ty,
            choice_name,
            variant_name,
        );
        self.instructions.push(Instruction::Instanceof {
            dest,
            src,
            type_idx,
        });
        self.free_temp_if_operand(operand);
    }

    pub(super) fn emit_load_global(&mut self, dest: Reg, name: &str) {
        let global_idx = self
            .ctx
            .graph
            .resolution()
            .and_then(|res| {
                let name_id = self.ctx.string_table.get(name);
                res.symbols()
                    .iter()
                    .find(|symbol| name_id.is_some() && symbol.name() == name_id.unwrap())
                    .map(|symbol| symbol.id().raw() as u16)
            })
            .unwrap_or(0);
        self.instructions.push(Instruction::LoadGlobal {
            dest,
            module_id: galfus_core::ModuleId::new(0),
            global_idx: GlobalIdx(global_idx),
        });
    }
}
