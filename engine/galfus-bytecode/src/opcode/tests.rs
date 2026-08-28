use super::*;
use crate::{BytecodeFormatVersion, CURRENT_BYTECODE_FORMAT_VERSION};

#[test]
fn decoder_distinguishes_incompatible_removed_and_unknown_input() {
    assert_eq!(
        decode_opcode(BytecodeFormatVersion::new(1, 0, 0), 0),
        Err(BytecodeDecodeError::UnsupportedFormat(
            BytecodeFormatError {
                supported: CURRENT_BYTECODE_FORMAT_VERSION,
                actual: BytecodeFormatVersion::new(1, 0, 0),
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
    for opcode in (0..=34).chain(46..=58).chain(61..=72) {
        assert_eq!(
            decode_opcode(CURRENT_BYTECODE_FORMAT_VERSION, opcode)
                .expect("current opcode must decode")
                .raw(),
            opcode
        );
    }
}
