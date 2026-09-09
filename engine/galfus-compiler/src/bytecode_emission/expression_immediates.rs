use super::function::FnEmitter;
use galfus_bytecode::instruction::{ImmediateBinaryOp, ImmediateValue, Reg};
use galfus_core::TypeId;
use galfus_frontend::{PrimitiveType, TypeKind};
use galfus_ir::mir::{Constant as MirConstant, MirBinaryOp, Operand};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub(super) fn is_numeric_constant(&self, constant: &MirConstant) -> bool {
        matches!(
            constant,
            MirConstant::Int8(_)
                | MirConstant::Int16(_)
                | MirConstant::Int32(_)
                | MirConstant::Int64(_)
                | MirConstant::Uint8(_)
                | MirConstant::Uint16(_)
                | MirConstant::Uint32(_)
                | MirConstant::Uint64(_)
                | MirConstant::Float32(_)
                | MirConstant::Float64(_)
        )
    }

    pub(super) fn immediate_value(&self, operand: &Operand) -> Option<ImmediateValue> {
        if let Operand::Local(local) = operand {
            return self.known_immediates.get(&Reg(local.raw() as u16)).copied();
        }
        let Operand::Constant(constant) = operand else {
            return None;
        };
        match constant {
            MirConstant::Int32(value) => Some(ImmediateValue::I32(*value)),
            MirConstant::Int64(value) => Some(ImmediateValue::I64(*value)),
            MirConstant::Uint32(value) => Some(ImmediateValue::U32(*value)),
            MirConstant::Uint64(value) => Some(ImmediateValue::U64(*value)),
            MirConstant::Float32(value) => Some(ImmediateValue::F32(value.to_bits())),
            MirConstant::Float64(value) => Some(ImmediateValue::F64(value.to_bits())),
            _ => None,
        }
    }

    pub(super) fn cast_immediate(
        &self,
        immediate: ImmediateValue,
        ty: TypeId,
    ) -> Option<ImmediateValue> {
        let ty = crate::bytecode_emission::types::resolve_type_with_substitutions(self.ctx, ty);
        let table = self.ctx.type_result.layer().table();
        let TypeKind::Primitive(primitive) = table.kind(ty)? else {
            return None;
        };
        match (immediate, primitive) {
            (ImmediateValue::I32(value), PrimitiveType::Int32) => Some(ImmediateValue::I32(value)),
            (ImmediateValue::I32(value), PrimitiveType::Int64) => {
                Some(ImmediateValue::I64(value as i64))
            }
            (ImmediateValue::I32(value), PrimitiveType::Uint32) => {
                Some(ImmediateValue::U32(value as u32))
            }
            (ImmediateValue::I32(value), PrimitiveType::Uint64) => {
                Some(ImmediateValue::U64(value as u64))
            }
            (ImmediateValue::I32(value), PrimitiveType::Float32) => {
                Some(ImmediateValue::F32((value as f32).to_bits()))
            }
            (ImmediateValue::I32(value), PrimitiveType::Float64) => {
                Some(ImmediateValue::F64((value as f64).to_bits()))
            }
            (ImmediateValue::F64(value), PrimitiveType::Float32) => Some(ImmediateValue::F32(
                (f64::from_bits(value) as f32).to_bits(),
            )),
            (ImmediateValue::F64(value), PrimitiveType::Float64) => {
                Some(ImmediateValue::F64(value))
            }
            (ImmediateValue::F32(value), PrimitiveType::Float64) => Some(ImmediateValue::F64(
                (f32::from_bits(value) as f64).to_bits(),
            )),
            (ImmediateValue::I64(value), PrimitiveType::Int64) => Some(ImmediateValue::I64(value)),
            (ImmediateValue::U32(value), PrimitiveType::Uint32) => Some(ImmediateValue::U32(value)),
            (ImmediateValue::U64(value), PrimitiveType::Uint64) => Some(ImmediateValue::U64(value)),
            (ImmediateValue::F32(value), PrimitiveType::Float32) => {
                Some(ImmediateValue::F32(value))
            }
            _ => None,
        }
    }

    pub(super) fn immediate_binary_operation(
        &self,
        operation: &MirBinaryOp,
    ) -> Option<ImmediateBinaryOp> {
        match operation {
            MirBinaryOp::Add => Some(ImmediateBinaryOp::Add),
            MirBinaryOp::Subtract => Some(ImmediateBinaryOp::Subtract),
            MirBinaryOp::Multiply => Some(ImmediateBinaryOp::Multiply),
            MirBinaryOp::Divide => Some(ImmediateBinaryOp::Divide),
            MirBinaryOp::Remainder => Some(ImmediateBinaryOp::Remainder),
            MirBinaryOp::Equal => Some(ImmediateBinaryOp::Equal),
            MirBinaryOp::NotEqual => Some(ImmediateBinaryOp::NotEqual),
            MirBinaryOp::Less => Some(ImmediateBinaryOp::Less),
            MirBinaryOp::LessEqual => Some(ImmediateBinaryOp::LessEqual),
            MirBinaryOp::Greater => Some(ImmediateBinaryOp::Greater),
            MirBinaryOp::GreaterEqual => Some(ImmediateBinaryOp::GreaterEqual),
            MirBinaryOp::ShiftLeft => Some(ImmediateBinaryOp::ShiftLeft),
            MirBinaryOp::ShiftRight => Some(ImmediateBinaryOp::ShiftRight),
            MirBinaryOp::BitwiseAnd => Some(ImmediateBinaryOp::BitwiseAnd),
            MirBinaryOp::BitwiseOr => Some(ImmediateBinaryOp::BitwiseOr),
            MirBinaryOp::BitwiseXor => Some(ImmediateBinaryOp::BitwiseXor),
            MirBinaryOp::Power
            | MirBinaryOp::LogicalAnd
            | MirBinaryOp::LogicalOr
            | MirBinaryOp::NullFallback => None,
        }
    }

    pub(super) fn constant_matches_destination_type(
        &self,
        constant: &MirConstant,
        dest: Reg,
    ) -> bool {
        let Some(local) = self
            .func
            .locals
            .iter()
            .find(|local| local.id.raw() as u16 == dest.raw())
        else {
            return false;
        };
        let table = self.ctx.type_result.layer().table();
        let constant_type = match constant {
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
            _ => return false,
        };
        crate::bytecode_emission::types::resolve_type_with_substitutions(self.ctx, local.ty)
            == table.primitive(constant_type)
    }
}
