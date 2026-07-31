#[cfg(test)]
mod tests;

use std::error;
use std::fmt;

use galfus_bytecode::instruction::{ConstIdx, FieldIdx, FuncIdx, Reg, TypeIdx};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    #[error("Type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },

    #[error("Register index {reg:?} is out of bounds")]
    RegisterOutOfBounds { reg: Reg },

    #[error("Constant index {index:?} is out of bounds")]
    ConstantOutOfBounds { index: ConstIdx },

    #[error("Function index {index:?} is out of bounds")]
    FunctionOutOfBounds { index: FuncIdx },

    #[error("Type index {index:?} is out of bounds")]
    TypeOutOfBounds { index: TypeIdx },

    #[error("Field index {index:?} is out of bounds")]
    FieldOutOfBounds { index: FieldIdx },

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Call stack overflow")]
    CallStackOverflow,

    #[error("Empty call stack")]
    EmptyCallStack,
    #[error("Unresolved host block")]
    UnresolvedHostBlocked,
    #[error("Invalid object reference")]
    InvalidObjectReference,

    #[error("Invalid jump target: pc {pc}")]
    InvalidJumpTarget { pc: usize },

    #[error("Instruction pointer {pc} is out of bounds")]
    InstructionPointerOutOfBounds { pc: usize },

    #[error("Array index {index} is out of bounds (length {len})")]
    IndexOutOfBounds { index: i128, len: usize },

    #[error("Variant payload mismatch: variant has no payload")]
    NoVariantPayload,

    #[error("Explicit panic: {message}")]
    Panic { message: String },

    #[error("Invalid bytecode module")]
    InvalidModule,

    #[error("Unimplemented instruction: {instruction}")]
    UnimplementedInstruction { instruction: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrameInfo {
    pub module_id: galfus_core::ModuleId,
    pub func_idx: FuncIdx,
    pub instruction_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmPanic {
    pub error: VmError,
    pub stack_trace: Vec<StackFrameInfo>,
}

impl fmt::Display for VmPanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "VM Panic: {}", self.error)?;
        writeln!(f, "Stack trace:")?;
        for (i, frame) in self.stack_trace.iter().enumerate() {
            writeln!(
                f,
                "  #{}: Module {:?} Func {:?} (at instruction {})",
                i, frame.module_id, frame.func_idx, frame.instruction_offset
            )?;
        }
        Ok(())
    }
}

impl error::Error for VmPanic {}
