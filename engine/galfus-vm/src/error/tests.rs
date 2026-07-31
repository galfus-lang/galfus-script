use super::*;

#[test]
fn test_vm_panic_display() {
    let panic = VmPanic {
        error: VmError::DivisionByZero,
        stack_trace: vec![
            StackFrameInfo {
                module_id: galfus_core::ModuleId::new(0),
                func_idx: galfus_bytecode::instruction::FuncIdx(0),
                instruction_offset: 12,
            },
            StackFrameInfo {
                module_id: galfus_core::ModuleId::new(0),
                func_idx: galfus_bytecode::instruction::FuncIdx(0),
                instruction_offset: 4,
            },
        ],
    };
    let display_str = format!("{}", panic);
    assert!(display_str.contains("VM Panic: Division by zero"));
    assert!(display_str.contains("  #0: Module ModuleId(0) Func FuncIdx(0) (at instruction 12)"));
    assert!(display_str.contains("  #1: Module ModuleId(0) Func FuncIdx(0) (at instruction 4)"));
}
