use super::LowerCtx;
use galfus_bytecode::Constant;
use galfus_bytecode::instruction::ConstIdx;
use galfus_ir::mir::Constant as MirConstant;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HashableConstant {
    Bool(bool),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Uint8(u8),
    Uint16(u16),
    Uint32(u32),
    Uint64(u64),
    Float32Bits(u32),
    Float64Bits(u64),
    String(String),
    Function(u32),
}

impl HashableConstant {
    pub fn from_mir(constant: &MirConstant) -> Option<Self> {
        match constant {
            MirConstant::Null => None,
            MirConstant::Bool(b) => Some(Self::Bool(*b)),
            MirConstant::Int8(i) => Some(Self::Int8(*i)),
            MirConstant::Int16(i) => Some(Self::Int16(*i)),
            MirConstant::Int32(i) => Some(Self::Int32(*i)),
            MirConstant::Int64(i) => Some(Self::Int64(*i)),
            MirConstant::Uint8(i) => Some(Self::Uint8(*i)),
            MirConstant::Uint16(i) => Some(Self::Uint16(*i)),
            MirConstant::Uint32(i) => Some(Self::Uint32(*i)),
            MirConstant::Uint64(i) => Some(Self::Uint64(*i)),
            MirConstant::Float32(f) => Some(Self::Float32Bits(f.to_bits())),
            MirConstant::Float64(f) => Some(Self::Float64Bits(f.to_bits())),
            MirConstant::String(s) => Some(Self::String(s.clone())),
            MirConstant::Function(id) => Some(Self::Function(id.raw())),
        }
    }
}

pub fn get_or_create_constant(ctx: &mut LowerCtx, constant: &MirConstant) -> ConstIdx {
    let hashable = match HashableConstant::from_mir(constant) {
        Some(h) => h,
        None => return ConstIdx(0), // Placeholder for Null
    };

    if let Some(&idx) = ctx.constants_map.get(&hashable) {
        return idx;
    }

    let next_idx = ConstIdx(ctx.constant_pool.constants.len() as u16);
    ctx.constants_map.insert(hashable, next_idx);

    let c = match constant {
        MirConstant::Null => unreachable!(),
        MirConstant::Bool(b) => Constant::Bool(*b),
        MirConstant::Int8(i) => Constant::Int8(*i),
        MirConstant::Int16(i) => Constant::Int16(*i),
        MirConstant::Int32(i) => Constant::Int32(*i),
        MirConstant::Int64(i) => Constant::Int64(*i),
        MirConstant::Uint8(i) => Constant::Uint8(*i),
        MirConstant::Uint16(i) => Constant::Uint16(*i),
        MirConstant::Uint32(i) => Constant::Uint32(*i),
        MirConstant::Uint64(i) => Constant::Uint64(*i),
        MirConstant::Float32(f) => Constant::Float32(galfus_core::normalize_f32(*f)),
        MirConstant::Float64(f) => Constant::Float64(galfus_core::normalize_f64(*f)),
        MirConstant::String(s) => Constant::String(s.clone()),
        MirConstant::Function(id) => {
            let func_idx = *ctx.function_map.get(id).unwrap_or_else(|| {
                panic!(
                    "FunctionId {:?} not found in function_map during constant lowering",
                    id
                )
            });
            Constant::Function(func_idx)
        }
    };

    ctx.constant_pool.constants.push(c);
    next_idx
}
