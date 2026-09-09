use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::Reg;
use galfus_ir::mir::MirBinaryOp;

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub(super) fn emit_i32_binary_instruction(
        &self,
        op: &MirBinaryOp,
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    ) -> Instruction {
        match op {
            MirBinaryOp::Add => Instruction::AddI32 { dest, lhs, rhs },
            MirBinaryOp::Subtract => Instruction::SubI32 { dest, lhs, rhs },
            MirBinaryOp::Multiply => Instruction::MulI32 { dest, lhs, rhs },
            MirBinaryOp::Divide => Instruction::DivI32 { dest, lhs, rhs },
            MirBinaryOp::Remainder => Instruction::RemI32 { dest, lhs, rhs },
            MirBinaryOp::Equal => Instruction::EqI32 { dest, lhs, rhs },
            MirBinaryOp::NotEqual => Instruction::NeI32 { dest, lhs, rhs },
            MirBinaryOp::Less => Instruction::LtI32 { dest, lhs, rhs },
            MirBinaryOp::LessEqual => Instruction::LeI32 { dest, lhs, rhs },
            MirBinaryOp::Greater => Instruction::GtI32 { dest, lhs, rhs },
            MirBinaryOp::GreaterEqual => Instruction::GeI32 { dest, lhs, rhs },
            MirBinaryOp::Power => Instruction::Pow { dest, lhs, rhs },
            MirBinaryOp::ShiftLeft => Instruction::Shl { dest, lhs, rhs },
            MirBinaryOp::ShiftRight => Instruction::Shr { dest, lhs, rhs },
            MirBinaryOp::BitwiseAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::BitwiseOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::BitwiseXor => Instruction::Xor { dest, lhs, rhs },
            MirBinaryOp::LogicalAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::LogicalOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::NullFallback => Instruction::Fallback {
                dest,
                src: lhs,
                fallback: rhs,
            },
        }
    }

    pub(super) fn emit_i64_binary_instruction(
        &self,
        op: &MirBinaryOp,
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    ) -> Instruction {
        match op {
            MirBinaryOp::Add => Instruction::AddI64 { dest, lhs, rhs },
            MirBinaryOp::Subtract => Instruction::SubI64 { dest, lhs, rhs },
            MirBinaryOp::Multiply => Instruction::MulI64 { dest, lhs, rhs },
            MirBinaryOp::Divide => Instruction::DivI64 { dest, lhs, rhs },
            MirBinaryOp::Remainder => Instruction::RemI64 { dest, lhs, rhs },
            MirBinaryOp::Equal => Instruction::EqI64 { dest, lhs, rhs },
            MirBinaryOp::NotEqual => Instruction::NeI64 { dest, lhs, rhs },
            MirBinaryOp::Less => Instruction::LtI64 { dest, lhs, rhs },
            MirBinaryOp::LessEqual => Instruction::LeI64 { dest, lhs, rhs },
            MirBinaryOp::Greater => Instruction::GtI64 { dest, lhs, rhs },
            MirBinaryOp::GreaterEqual => Instruction::GeI64 { dest, lhs, rhs },
            MirBinaryOp::Power => Instruction::Pow { dest, lhs, rhs },
            MirBinaryOp::ShiftLeft => Instruction::Shl { dest, lhs, rhs },
            MirBinaryOp::ShiftRight => Instruction::Shr { dest, lhs, rhs },
            MirBinaryOp::BitwiseAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::BitwiseOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::BitwiseXor => Instruction::Xor { dest, lhs, rhs },
            MirBinaryOp::LogicalAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::LogicalOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::NullFallback => Instruction::Fallback {
                dest,
                src: lhs,
                fallback: rhs,
            },
        }
    }

    pub(super) fn emit_f32_binary_instruction(
        &self,
        op: &MirBinaryOp,
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    ) -> Instruction {
        match op {
            MirBinaryOp::Add => Instruction::AddF32 { dest, lhs, rhs },
            MirBinaryOp::Subtract => Instruction::SubF32 { dest, lhs, rhs },
            MirBinaryOp::Multiply => Instruction::MulF32 { dest, lhs, rhs },
            MirBinaryOp::Divide => Instruction::DivF32 { dest, lhs, rhs },
            MirBinaryOp::Remainder => Instruction::RemF32 { dest, lhs, rhs },
            MirBinaryOp::Equal => Instruction::EqF32 { dest, lhs, rhs },
            MirBinaryOp::NotEqual => Instruction::NeF32 { dest, lhs, rhs },
            MirBinaryOp::Less => Instruction::LtF32 { dest, lhs, rhs },
            MirBinaryOp::LessEqual => Instruction::LeF32 { dest, lhs, rhs },
            MirBinaryOp::Greater => Instruction::GtF32 { dest, lhs, rhs },
            MirBinaryOp::GreaterEqual => Instruction::GeF32 { dest, lhs, rhs },
            MirBinaryOp::Power => Instruction::Pow { dest, lhs, rhs },
            MirBinaryOp::ShiftLeft => Instruction::Shl { dest, lhs, rhs },
            MirBinaryOp::ShiftRight => Instruction::Shr { dest, lhs, rhs },
            MirBinaryOp::BitwiseAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::BitwiseOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::BitwiseXor => Instruction::Xor { dest, lhs, rhs },
            MirBinaryOp::LogicalAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::LogicalOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::NullFallback => Instruction::Fallback {
                dest,
                src: lhs,
                fallback: rhs,
            },
        }
    }

    pub(super) fn emit_f64_binary_instruction(
        &self,
        op: &MirBinaryOp,
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    ) -> Instruction {
        match op {
            MirBinaryOp::Add => Instruction::AddF64 { dest, lhs, rhs },
            MirBinaryOp::Subtract => Instruction::SubF64 { dest, lhs, rhs },
            MirBinaryOp::Multiply => Instruction::MulF64 { dest, lhs, rhs },
            MirBinaryOp::Divide => Instruction::DivF64 { dest, lhs, rhs },
            MirBinaryOp::Remainder => Instruction::RemF64 { dest, lhs, rhs },
            MirBinaryOp::Equal => Instruction::EqF64 { dest, lhs, rhs },
            MirBinaryOp::NotEqual => Instruction::NeF64 { dest, lhs, rhs },
            MirBinaryOp::Less => Instruction::LtF64 { dest, lhs, rhs },
            MirBinaryOp::LessEqual => Instruction::LeF64 { dest, lhs, rhs },
            MirBinaryOp::Greater => Instruction::GtF64 { dest, lhs, rhs },
            MirBinaryOp::GreaterEqual => Instruction::GeF64 { dest, lhs, rhs },
            MirBinaryOp::Power => Instruction::Pow { dest, lhs, rhs },
            MirBinaryOp::ShiftLeft => Instruction::Shl { dest, lhs, rhs },
            MirBinaryOp::ShiftRight => Instruction::Shr { dest, lhs, rhs },
            MirBinaryOp::BitwiseAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::BitwiseOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::BitwiseXor => Instruction::Xor { dest, lhs, rhs },
            MirBinaryOp::LogicalAnd => Instruction::And { dest, lhs, rhs },
            MirBinaryOp::LogicalOr => Instruction::Or { dest, lhs, rhs },
            MirBinaryOp::NullFallback => Instruction::Fallback {
                dest,
                src: lhs,
                fallback: rhs,
            },
        }
    }
}
