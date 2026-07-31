use crate::thread;

use super::*;

impl VirtualMachine {
    pub(super) fn execute_cast(
        &self,
        thread: &thread::VmThreadState,
        val: &Value,
        target_ty: TypeIdx,
    ) -> Result<Value, VmError> {
        let ty = self
            .current_image(thread)
            .unwrap()
            .types
            .get(target_ty.raw() as usize)
            .ok_or(VmError::TypeOutOfBounds { index: target_ty })?;

        if self.check_value_type(thread, val, target_ty) {
            return Ok(val.clone());
        }

        // Numeric casting / conversions
        let casted = match val {
            Value::Int8(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Int16(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Int32(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Int64(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Uint8(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Uint16(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Uint32(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Uint64(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Float32(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x)),
                BytecodeType::Float64 => Some(Value::Float64(*x as f64)),
                _ => None,
            },
            Value::Float64(x) => match ty {
                BytecodeType::Int8 => Some(Value::Int8(*x as i8)),
                BytecodeType::Int16 => Some(Value::Int16(*x as i16)),
                BytecodeType::Int32 => Some(Value::Int32(*x as i32)),
                BytecodeType::Int64 => Some(Value::Int64(*x as i64)),
                BytecodeType::Uint8 => Some(Value::Uint8(*x as u8)),
                BytecodeType::Uint16 => Some(Value::Uint16(*x as u16)),
                BytecodeType::Uint32 => Some(Value::Uint32(*x as u32)),
                BytecodeType::Uint64 => Some(Value::Uint64(*x as u64)),
                BytecodeType::Float32 => Some(Value::Float32(*x as f32)),
                BytecodeType::Float64 => Some(Value::Float64(*x)),
                _ => None,
            },
            _ => None,
        };

        casted.ok_or_else(|| VmError::TypeMismatch {
            expected: format!("{:?}", ty),
            found: format!("{:?}", val),
        })
    }
}
