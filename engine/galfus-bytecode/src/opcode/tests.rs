use super::*;
use crate::{BytecodeFormatVersion, CURRENT_BYTECODE_FORMAT_VERSION};

#[test]
fn decoder_distinguishes_legacy_removed_and_unknown_input() {
    assert_eq!(
        decode_opcode(BytecodeFormatVersion::new(1), 0),
        Err(BytecodeDecodeError::UnsupportedFormat(
            BytecodeFormatError::LegacyVersion {
                supported: CURRENT_BYTECODE_FORMAT_VERSION,
                actual: BytecodeFormatVersion::new(1),
            }
        ))
    );
    assert_eq!(
        decode_opcode(CURRENT_BYTECODE_FORMAT_VERSION, 35),
        Err(BytecodeDecodeError::RemovedOpcode {
            opcode: RemovedOpcode::ReceiveFilter,
        })
    );
    assert_eq!(
        decode_opcode(CURRENT_BYTECODE_FORMAT_VERSION, 255),
        Err(BytecodeDecodeError::UnknownOpcode { opcode: 255 })
    );
}

#[test]
fn decoder_accepts_current_instruction_tags() {
    assert_eq!(
        decode_opcode(CURRENT_BYTECODE_FORMAT_VERSION, 62)
            .expect("CreateFuture is a current opcode")
            .raw(),
        62
    );
}
