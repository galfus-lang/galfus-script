#[cfg(test)]
mod tests;

use crate::{BytecodeFormatError, BytecodeFormatVersion, Instruction, validate_bytecode_format};

/// Stable wire tag for an instruction in bytecode format v2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Opcode(u16);

impl Opcode {
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Opcodes that belonged to the immediate boundary model and were removed in v2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RemovedOpcode {
    ReceiveFilter,
    MailboxHasMessages,
    MailboxGetMessage,
    Send,
    CreateThread,
    StartThread,
    GetThread,
    ThreadIsRunning,
    ThreadIsExited,
    ThreadExitReason,
    WaitThread,
    CallNative,
    AdapterCall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BytecodeDecodeError {
    #[error(transparent)]
    UnsupportedFormat(#[from] BytecodeFormatError),
    #[error("opcode {opcode:?} was removed from bytecode format v2")]
    RemovedOpcode { opcode: RemovedOpcode },
    #[error("unknown opcode tag {opcode}")]
    UnknownOpcode { opcode: u16 },
}

/// Decode a bytecode v2 instruction tag before decoding its operands.
///
/// The tag assignments retain the positions of the legacy instruction set so
/// a current-format artifact containing an old tag is diagnosed explicitly.
pub fn decode_opcode(
    format_version: BytecodeFormatVersion,
    raw_opcode: u16,
) -> Result<Opcode, BytecodeDecodeError> {
    validate_bytecode_format(format_version)?;

    match raw_opcode {
        35 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::ReceiveFilter,
        }),
        36 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::MailboxHasMessages,
        }),
        37 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::MailboxGetMessage,
        }),
        38 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::Send,
        }),
        39 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::CreateThread,
        }),
        40 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::StartThread,
        }),
        41 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::GetThread,
        }),
        42 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::ThreadIsRunning,
        }),
        43 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::ThreadIsExited,
        }),
        44 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::ThreadExitReason,
        }),
        45 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::WaitThread,
        }),
        59 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::CallNative,
        }),
        60 => Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::AdapterCall,
        }),
        0..=34 | 46..=58 | 61..=67 => Ok(Opcode(raw_opcode)),
        opcode => Err(BytecodeDecodeError::UnknownOpcode { opcode }),
    }
}

impl Instruction {
    pub const fn opcode(&self) -> Opcode {
        let raw = match self {
            Self::LoadConst { .. } => 0,
            Self::Move { .. } => 1,
            Self::LoadGlobal { .. } => 2,
            Self::StoreGlobal { .. } => 3,
            Self::LoadNull { .. } => 4,
            Self::Add { .. } => 5,
            Self::Sub { .. } => 6,
            Self::Mul { .. } => 7,
            Self::Div { .. } => 8,
            Self::Rem { .. } => 9,
            Self::Pow { .. } => 10,
            Self::Neg { .. } => 11,
            Self::Not { .. } => 12,
            Self::BitNot { .. } => 13,
            Self::Shl { .. } => 14,
            Self::Shr { .. } => 15,
            Self::And { .. } => 16,
            Self::Or { .. } => 17,
            Self::Xor { .. } => 18,
            Self::Eq { .. } => 19,
            Self::Ne { .. } => 20,
            Self::Lt { .. } => 21,
            Self::Le { .. } => 22,
            Self::Gt { .. } => 23,
            Self::Ge { .. } => 24,
            Self::Fallback { .. } => 25,
            Self::Jump { .. } => 26,
            Self::JumpTrue { .. } => 27,
            Self::JumpFalse { .. } => 28,
            Self::JumpNull { .. } => 29,
            Self::Call { .. } => 30,
            Self::CallMethod { .. } => 31,
            Self::CallDynamic { .. } => 32,
            Self::Ret { .. } => 33,
            Self::RetNull => 34,
            Self::Panic { .. } => 46,
            Self::AllocLocal { .. } => 47,
            Self::LoadField { .. } => 48,
            Self::StoreField { .. } => 49,
            Self::NewArray { .. } => 50,
            Self::LoadIndex { .. } => 51,
            Self::StoreIndex { .. } => 52,
            Self::NewTuple { .. } => 53,
            Self::NewChoice { .. } => 54,
            Self::Cast { .. } => 55,
            Self::Copy { .. } => 56,
            Self::Instanceof { .. } => 57,
            Self::Drop { .. } => 58,
            Self::AwaitFuture { .. } => 61,
            Self::CreateFuture { .. } => 62,
            Self::CreateIndirectFuture { .. } => 63,
            Self::AwaitAll { .. } => 64,
            Self::AwaitRace { .. } => 65,
            Self::Len { .. } => 66,
            Self::CopyArray { .. } => 67,
            _ => 100,
        };
        Opcode(raw)
    }
}
