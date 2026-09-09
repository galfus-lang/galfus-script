use super::function::FnEmitter;
use galfus_bytecode::instruction::TypeIdx;
use galfus_core::TypeId;
use galfus_frontend::{PrimitiveType, TypeKind};
use galfus_ir::mir::{Constant as MirConstant, Operand};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub(super) fn future_payload_type_index(&mut self, future_ty: TypeId) -> TypeIdx {
        let future_ty =
            crate::bytecode_emission::types::resolve_type_with_substitutions(self.ctx, future_ty);
        let table = self.ctx.type_result.layer().table();
        let TypeKind::GenericInstance { arguments, .. } = table
            .kind(future_ty)
            .unwrap_or_else(|| panic!("async function must return Future<T>"))
        else {
            panic!("async function must return Future<T>");
        };
        let payload_ty = *arguments
            .first()
            .expect("Future<T> must carry a payload type");
        crate::bytecode_emission::types::lower_type(self.ctx, payload_ty)
    }

    pub(crate) fn get_operand_type(&self, operand: &Operand) -> TypeId {
        match operand {
            Operand::Local(local_id) => {
                let local_decl = self.func.locals.iter().find(|l| l.id == *local_id).unwrap();
                local_decl.ty
            }
            Operand::ConstRef(idx) => {
                let constant = &self.ctx.mir_constants[*idx];
                let layer = self.ctx.type_result.layer();
                let table = layer.table();
                if matches!(constant, MirConstant::String(_)) {
                    let u8_ty = table.primitive(PrimitiveType::Uint8);
                    for i in 0..table.len() {
                        let ty_id = TypeId::new(i as u32);
                        if let Some(TypeKind::Array { element }) = table.kind(ty_id)
                            && *element == u8_ty
                        {
                            return ty_id;
                        }
                    }
                }
                let prim = match constant {
                    MirConstant::Null => PrimitiveType::Null,
                    MirConstant::Bool(_) => PrimitiveType::Bool,
                    MirConstant::Int8(_) => PrimitiveType::Int8,
                    MirConstant::Int16(_) => PrimitiveType::Int16,
                    MirConstant::Int32(_) => PrimitiveType::Int32,
                    MirConstant::Int64(_) => PrimitiveType::Int64,
                    MirConstant::Uint8(_) => PrimitiveType::Uint8,
                    MirConstant::Uint16(_) => PrimitiveType::Uint16,
                    MirConstant::Uint32(_) => PrimitiveType::Uint32,
                    MirConstant::Uint64(_) => PrimitiveType::Uint64,
                    MirConstant::Float32(_) => PrimitiveType::Float32,
                    MirConstant::Float64(_) => PrimitiveType::Float64,
                    MirConstant::Function(_) => PrimitiveType::Null,
                    MirConstant::String(_) => unreachable!(),
                };
                table.primitive(prim)
            }
            Operand::Constant(constant) => {
                let layer = self.ctx.type_result.layer();
                let table = layer.table();
                if matches!(constant, MirConstant::String(_)) {
                    let u8_ty = table.primitive(PrimitiveType::Uint8);
                    for i in 0..table.len() {
                        let ty_id = TypeId::new(i as u32);
                        if let Some(TypeKind::Array { element }) = table.kind(ty_id)
                            && *element == u8_ty
                        {
                            return ty_id;
                        }
                    }
                }
                let prim = match constant {
                    MirConstant::Null => PrimitiveType::Null,
                    MirConstant::Bool(_) => PrimitiveType::Bool,
                    MirConstant::Int8(_) => PrimitiveType::Int8,
                    MirConstant::Int16(_) => PrimitiveType::Int16,
                    MirConstant::Int32(_) => PrimitiveType::Int32,
                    MirConstant::Int64(_) => PrimitiveType::Int64,
                    MirConstant::Uint8(_) => PrimitiveType::Uint8,
                    MirConstant::Uint16(_) => PrimitiveType::Uint16,
                    MirConstant::Uint32(_) => PrimitiveType::Uint32,
                    MirConstant::Uint64(_) => PrimitiveType::Uint64,
                    MirConstant::Float32(_) => PrimitiveType::Float32,
                    MirConstant::Float64(_) => PrimitiveType::Float64,
                    MirConstant::Function(_) => PrimitiveType::Null,
                    MirConstant::String(_) => unreachable!(),
                };
                for i in 0..table.len() {
                    let ty_id = TypeId::new(i as u32);
                    if matches!(table.kind(ty_id), Some(TypeKind::Primitive(p)) if p == &prim) {
                        return ty_id;
                    }
                }
                TypeId::new(0)
            }
        }
    }
}
