use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::Reg;
use galfus_core::TypeId;
use galfus_frontend::{PrimitiveType, TypeKind};
use galfus_ir::mir::Constant as MirConstant;

impl<'a, 'b> FnEmitter<'a, 'b> {
    fn ensure_string_constant_type(&mut self) {
        let table = self.ctx.type_result.layer().table();
        let u8_type = table.primitive(PrimitiveType::Uint8);
        let byte_array_type = (0..table.len())
            .map(|index| TypeId::new(index as u32))
            .find(|type_id| {
                matches!(table.kind(*type_id), Some(TypeKind::Array { element }) if *element == u8_type)
            });

        crate::bytecode_emission::types::lower_type(self.ctx, byte_array_type.unwrap_or(u8_type));
    }

    pub(super) fn load_constant(&mut self, dest: Reg, constant: &MirConstant) {
        if matches!(constant, MirConstant::String(_)) {
            self.ensure_string_constant_type();
        }
        let const_idx =
            crate::bytecode_emission::constants::get_or_create_constant(self.ctx, constant);
        self.instructions
            .push(Instruction::LoadConst { dest, const_idx });
    }
}
