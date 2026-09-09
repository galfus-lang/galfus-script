use super::function::FnEmitter;
use galfus_bytecode::instruction::FieldIdx;
use galfus_core::{SymbolId, TypeId};
use galfus_frontend::{SymbolKind, TypeKind};
use galfus_ir::mir::Operand;

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub fn field_idx_for_member(&self, obj_operand: &Operand, field_name: &str) -> FieldIdx {
        let obj_type = self.get_operand_type(obj_operand);
        let table = self.ctx.type_result.layer().table();
        let resolved_type = crate::bytecode_emission::types::resolve_alias_type(self.ctx, obj_type);
        let imported_fields = match table.kind(resolved_type) {
            Some(TypeKind::Named { symbol }) => self.ctx.imported_struct_fields.get(symbol),
            _ => None,
        };
        let field_idx = if matches!(table.kind(resolved_type), Some(TypeKind::Tuple { .. })) {
            field_name.parse::<u16>().unwrap_or(0)
        } else if let Some(symbol) = self.struct_symbol_for_type(obj_type) {
            let struct_fields =
                crate::bytecode_emission::types::get_struct_fields(self.ctx, symbol);
            struct_fields
                .iter()
                .position(|(name, _)| name == field_name)
                .unwrap_or(0) as u16
        } else if let Some(fields) = imported_fields {
            fields
                .iter()
                .position(|(name, _)| name == field_name)
                .unwrap_or(0) as u16
        } else {
            let mut matching_indices = self
                .ctx
                .struct_layouts
                .iter()
                .filter_map(|layout| {
                    layout
                        .fields
                        .iter()
                        .position(|field| field.name == field_name)
                })
                .map(|index| index as u16);
            let Some(index) = matching_indices.next() else {
                return FieldIdx(0);
            };
            if matching_indices.all(|candidate| candidate == index) {
                index
            } else {
                0
            }
        };

        FieldIdx(field_idx)
    }

    pub(super) fn struct_symbol_for_type(&self, ty: TypeId) -> Option<SymbolId> {
        let ty = crate::bytecode_emission::types::resolve_alias_type(self.ctx, ty);
        let layer = self.ctx.type_result.layer();
        let table = layer.table();
        let mut current = ty;
        loop {
            match table.kind(current) {
                Some(TypeKind::Named { symbol }) => {
                    let resolution = self.ctx.graph.resolution()?;
                    let is_struct_or_choice = resolution.symbol(*symbol).is_some_and(|sd| {
                        sd.kind() == SymbolKind::Struct || sd.kind() == SymbolKind::Choice
                    }) || self
                        .ctx
                        .type_result
                        .imported_struct_fields
                        .contains_key(symbol);
                    if is_struct_or_choice {
                        return Some(*symbol);
                    }
                    break;
                }
                Some(TypeKind::GenericInstance { base, .. }) => {
                    current = *base;
                }
                _ => break,
            }
        }
        None
    }
}
