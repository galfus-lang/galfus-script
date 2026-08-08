#[cfg(test)]
mod tests;

use crate::ExportKind;

use crate::instruction::{ConstIdx, FieldIdx, FuncIdx, Reg, TypeIdx};
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BytecodeValidationError {
    FutureArgumentTypeCountMismatch {
        func_name: String,
        instr_idx: usize,
        expected_count: usize,
        found_count: usize,
    },
    InvalidConstantIndex {
        func_name: String,
        instr_idx: usize,
        index: ConstIdx,
    },
    InvalidTypeIndex {
        func_name: String,
        instr_idx: usize,
        index: TypeIdx,
    },
    InvalidFunctionIndex {
        func_name: String,
        instr_idx: usize,
        index: FuncIdx,
    },
    InvalidRegister {
        func_name: String,
        instr_idx: usize,
        reg: Reg,
        max_allowed: u16,
    },
    InvalidJumpOffset {
        func_name: String,
        instr_idx: usize,
        target_idx: i32,
        instr_count: usize,
    },
    TypeMismatchAlloc {
        func_name: String,
        instr_idx: usize,
        expected: &'static str,
        found: TypeIdx,
    },
    LayoutOutOfBounds {
        func_name: String,
        instr_idx: usize,
        expected: &'static str,
        layout_idx: u16,
    },
    ChoiceVariantOutOfBounds {
        func_name: String,
        instr_idx: usize,
        variant_idx: u16,
        variant_count: usize,
    },
    TupleCountMismatch {
        func_name: String,
        instr_idx: usize,
        expected_count: usize,
        found_count: usize,
    },
    FieldOutOfBounds {
        func_name: String,
        instr_idx: usize,
        field_idx: FieldIdx,
    },
    ExportFunctionOutOfBounds {
        symbol_name: String,
        func_idx: FuncIdx,
    },
    ExportGlobalOutOfBounds {
        symbol_name: String,
        global_idx: GlobalIdx,
        global_count: u32,
    },
    InitFunctionOutOfBounds {
        func_idx: FuncIdx,
    },
}

pub fn validate_bytecode_module(
    module: &BytecodeModule,
) -> Result<(), Vec<BytecodeValidationError>> {
    let mut errors = Vec::new();

    // 1. Validate init function
    if let Some(init_idx) = module.init_func_idx
        && init_idx.raw() as usize >= module.functions.len()
    {
        errors.push(BytecodeValidationError::InitFunctionOutOfBounds { func_idx: init_idx });
    }

    // 2. Validate exports
    for export in &module.exports {
        match export.kind {
            ExportKind::Function(func_idx) => {
                if func_idx.raw() as usize >= module.functions.len() {
                    errors.push(BytecodeValidationError::ExportFunctionOutOfBounds {
                        symbol_name: export.symbol_name.clone(),
                        func_idx,
                    });
                }
            }
            ExportKind::Global(global_idx) => {
                if u32::from(global_idx.raw()) >= module.global_count {
                    errors.push(BytecodeValidationError::ExportGlobalOutOfBounds {
                        symbol_name: export.symbol_name.clone(),
                        global_idx,
                        global_count: module.global_count,
                    });
                }
            }
        }
    }

    // 3. Helper to determine max fields in any struct or choice variant
    let max_struct_fields = module
        .struct_layouts
        .iter()
        .map(|l| l.fields.len())
        .max()
        .unwrap_or(0);
    let max_choice_payloads = module
        .choice_layouts
        .iter()
        .map(|l| {
            l.variants
                .iter()
                .map(|v| if v.payload_ty.is_some() { 1 } else { 0 })
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);
    let max_fields = max_struct_fields.max(max_choice_payloads);

    // 4. Validate instructions of each function
    for func in &module.functions {
        let max_regs = func.param_count as u16 + func.local_count + func.temp_count;
        let func_name = &func.name;

        for (instr_idx, instr) in func.instructions.iter().cloned().enumerate() {
            let check_reg = |reg: Reg, errors: &mut Vec<BytecodeValidationError>| {
                if reg.raw() >= max_regs {
                    errors.push(BytecodeValidationError::InvalidRegister {
                        func_name: func_name.clone(),
                        instr_idx,
                        reg,
                        max_allowed: max_regs,
                    });
                }
            };

            let check_const = |idx: ConstIdx, errors: &mut Vec<BytecodeValidationError>| {
                if idx.raw() as usize >= module.constants.constants.len() {
                    errors.push(BytecodeValidationError::InvalidConstantIndex {
                        func_name: func_name.clone(),
                        instr_idx,
                        index: idx,
                    });
                }
            };

            let check_type = |idx: TypeIdx, errors: &mut Vec<BytecodeValidationError>| {
                if idx.raw() as usize >= module.types.len() {
                    errors.push(BytecodeValidationError::InvalidTypeIndex {
                        func_name: func_name.clone(),
                        instr_idx,
                        index: idx,
                    });
                }
            };

            let check_func = |idx: FuncIdx, errors: &mut Vec<BytecodeValidationError>| {
                let limit = module.functions.len() + module.imports.len();
                if idx.raw() as usize >= limit {
                    errors.push(BytecodeValidationError::InvalidFunctionIndex {
                        func_name: func_name.clone(),
                        instr_idx,
                        index: idx,
                    });
                }
            };

            let check_jump = |offset: i32, errors: &mut Vec<BytecodeValidationError>| {
                let target = instr_idx as i32 + 1 + offset;
                if target < 0 || target >= func.instructions.len() as i32 {
                    errors.push(BytecodeValidationError::InvalidJumpOffset {
                        func_name: func_name.clone(),
                        instr_idx,
                        target_idx: target,
                        instr_count: func.instructions.len(),
                    });
                }
            };

            match instr {
                // Category A
                Instruction::LoadConst { dest, const_idx } => {
                    check_reg(dest, &mut errors);
                    check_const(const_idx, &mut errors);
                }
                Instruction::Move { dest, src } => {
                    check_reg(dest, &mut errors);
                    check_reg(src, &mut errors);
                }
                Instruction::LoadGlobal { dest, .. } => {
                    check_reg(dest, &mut errors);
                }
                Instruction::StoreGlobal {
                    global_idx: _, src, ..
                } => {
                    check_reg(src, &mut errors);
                }
                Instruction::LoadNull { dest } => {
                    check_reg(dest, &mut errors);
                }

                // Category B
                Instruction::Add { dest, lhs, rhs }
                | Instruction::Sub { dest, lhs, rhs }
                | Instruction::Mul { dest, lhs, rhs }
                | Instruction::Div { dest, lhs, rhs }
                | Instruction::Rem { dest, lhs, rhs }
                | Instruction::Pow { dest, lhs, rhs }
                | Instruction::Shl { dest, lhs, rhs }
                | Instruction::Shr { dest, lhs, rhs }
                | Instruction::And { dest, lhs, rhs }
                | Instruction::Or { dest, lhs, rhs }
                | Instruction::Xor { dest, lhs, rhs }
                | Instruction::Eq { dest, lhs, rhs }
                | Instruction::Ne { dest, lhs, rhs }
                | Instruction::Lt { dest, lhs, rhs }
                | Instruction::Le { dest, lhs, rhs }
                | Instruction::Gt { dest, lhs, rhs }
                | Instruction::Ge { dest, lhs, rhs } => {
                    check_reg(dest, &mut errors);
                    check_reg(lhs, &mut errors);
                    check_reg(rhs, &mut errors);
                }
                Instruction::Neg { dest, src }
                | Instruction::Not { dest, src }
                | Instruction::BitNot { dest, src } => {
                    check_reg(dest, &mut errors);
                    check_reg(src, &mut errors);
                }
                Instruction::Fallback {
                    dest,
                    src,
                    fallback,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(src, &mut errors);
                    check_reg(fallback, &mut errors);
                }

                // Category C
                Instruction::Jump { offset } => {
                    check_jump(offset, &mut errors);
                }
                Instruction::JumpTrue { cond, offset }
                | Instruction::JumpFalse { cond, offset } => {
                    check_reg(cond, &mut errors);
                    check_jump(offset, &mut errors);
                }
                Instruction::JumpNull { val, offset } => {
                    check_reg(val, &mut errors);
                    check_jump(offset, &mut errors);
                }
                Instruction::Call {
                    dest,
                    func: func_idx,
                    args_start,
                    arg_count,
                } => {
                    check_reg(dest, &mut errors);
                    check_func(func_idx, &mut errors);
                    if arg_count > 0 {
                        check_reg(args_start, &mut errors);
                        let end_reg = args_start.raw() as u32 + arg_count as u32 - 1;
                        if end_reg >= max_regs as u32 {
                            errors.push(BytecodeValidationError::InvalidRegister {
                                func_name: func_name.clone(),
                                instr_idx,
                                reg: Reg(end_reg as u16),
                                max_allowed: max_regs,
                            });
                        }
                    }
                }
                Instruction::CallMethod {
                    dest,
                    obj,
                    name_const,
                    args_start,
                    arg_count,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(obj, &mut errors);
                    check_const(name_const, &mut errors);
                    if arg_count > 1 {
                        check_reg(Reg(args_start.raw() + arg_count as u16 - 2), &mut errors);
                    }
                }
                Instruction::CallDynamic {
                    dest,
                    func_reg,
                    args_start,
                    arg_count,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(func_reg, &mut errors);
                    if arg_count > 0 {
                        check_reg(args_start, &mut errors);
                        let end_reg = args_start.raw() as u32 + arg_count as u32 - 1;
                        if end_reg >= max_regs as u32 {
                            errors.push(BytecodeValidationError::InvalidRegister {
                                func_name: func_name.clone(),
                                instr_idx,
                                reg: Reg(end_reg as u16),
                                max_allowed: max_regs,
                            });
                        }
                    }
                }
                Instruction::Ret { src } => {
                    check_reg(src, &mut errors);
                }
                Instruction::RetNull => {}
                Instruction::ReceiveFilter {
                    dest: _,
                    sender: _,
                    timeout: _,
                } => {
                    // Requires no validation (arguments are not bounds-checked here)
                }
                Instruction::MailboxHasMessages { dest: _ }
                | Instruction::MailboxGetMessage { dest: _ } => {
                    // Requires no validation
                }
                Instruction::Send {
                    dest: _,
                    target: _,
                    msg: _,
                } => {
                    // Requires no validation
                }
                Instruction::CreateThread {
                    dest: _,
                    func: _,
                    key: _,
                } => {
                    // Requires no validation
                }
                Instruction::StartThread {
                    dest: _,
                    thread_id: _,
                    arg: _,
                } => {
                    // Requires no validation
                }
                Instruction::GetThread { dest: _, key: _ }
                | Instruction::ThreadIsRunning {
                    dest: _,
                    thread_id: _,
                }
                | Instruction::ThreadIsExited {
                    dest: _,
                    thread_id: _,
                }
                | Instruction::ThreadExitReason {
                    dest: _,
                    thread_id: _,
                }
                | Instruction::WaitThread {
                    dest: _,
                    thread_id: _,
                } => {
                    // Requires no validation
                }
                Instruction::Panic { const_idx } => {
                    check_const(const_idx, &mut errors);
                }

                // Category D
                Instruction::AllocLocal { dest, type_idx } => {
                    check_reg(dest, &mut errors);
                    check_type(type_idx, &mut errors);
                    if (type_idx.raw() as usize) < module.types.len() {
                        match &module.types[type_idx.raw() as usize] {
                            BytecodeType::Struct(layout_idx) => {
                                if layout_idx.raw() as usize >= module.struct_layouts.len() {
                                    errors.push(BytecodeValidationError::LayoutOutOfBounds {
                                        func_name: func_name.clone(),
                                        instr_idx,
                                        expected: "StructLayout",
                                        layout_idx: layout_idx.raw(),
                                    });
                                }
                            }
                            _ => {
                                errors.push(BytecodeValidationError::TypeMismatchAlloc {
                                    func_name: func_name.clone(),
                                    instr_idx,
                                    expected: "Struct",
                                    found: type_idx,
                                });
                            }
                        }
                    }
                }
                Instruction::LoadField { dest, obj, field } => {
                    check_reg(dest, &mut errors);
                    check_reg(obj, &mut errors);
                    if field.raw() as usize >= max_fields {
                        errors.push(BytecodeValidationError::FieldOutOfBounds {
                            func_name: func_name.clone(),
                            instr_idx,
                            field_idx: field,
                        });
                    }
                }
                Instruction::StoreField { obj, field, val } => {
                    check_reg(obj, &mut errors);
                    check_reg(val, &mut errors);
                    if field.raw() as usize >= max_fields {
                        errors.push(BytecodeValidationError::FieldOutOfBounds {
                            func_name: func_name.clone(),
                            instr_idx,
                            field_idx: field,
                        });
                    }
                }
                Instruction::NewArray {
                    dest,
                    type_idx,
                    len_reg,
                } => {
                    check_reg(dest, &mut errors);
                    check_type(type_idx, &mut errors);
                    check_reg(len_reg, &mut errors);
                }
                Instruction::LoadIndex { dest, arr, idx } => {
                    check_reg(dest, &mut errors);
                    check_reg(arr, &mut errors);
                    check_reg(idx, &mut errors);
                }
                Instruction::StoreIndex { arr, idx, val } => {
                    check_reg(arr, &mut errors);
                    check_reg(idx, &mut errors);
                    check_reg(val, &mut errors);
                }
                Instruction::NewTuple {
                    dest,
                    type_idx,
                    start,
                    count,
                } => {
                    check_reg(dest, &mut errors);
                    check_type(type_idx, &mut errors);
                    if count > 0 {
                        check_reg(start, &mut errors);
                        let end_reg = start.raw() as u32 + count as u32 - 1;
                        if end_reg >= max_regs as u32 {
                            errors.push(BytecodeValidationError::InvalidRegister {
                                func_name: func_name.clone(),
                                instr_idx,
                                reg: Reg(end_reg as u16),
                                max_allowed: max_regs,
                            });
                        }
                    }
                    if (type_idx.raw() as usize) < module.types.len() {
                        match &module.types[type_idx.raw() as usize] {
                            BytecodeType::Tuple(elements) => {
                                if elements.len() != count as usize {
                                    errors.push(BytecodeValidationError::TupleCountMismatch {
                                        func_name: func_name.clone(),
                                        instr_idx,
                                        expected_count: elements.len(),
                                        found_count: count as usize,
                                    });
                                }
                            }
                            _ => {
                                errors.push(BytecodeValidationError::TypeMismatchAlloc {
                                    func_name: func_name.clone(),
                                    instr_idx,
                                    expected: "Tuple",
                                    found: type_idx,
                                });
                            }
                        }
                    }
                }
                Instruction::NewChoice {
                    dest,
                    type_idx,
                    variant_idx,
                    payload,
                } => {
                    check_reg(dest, &mut errors);
                    check_type(type_idx, &mut errors);
                    check_reg(payload, &mut errors);
                    if (type_idx.raw() as usize) < module.types.len() {
                        match &module.types[type_idx.raw() as usize] {
                            BytecodeType::Choice(layout_idx) => {
                                if layout_idx.raw() as usize >= module.choice_layouts.len() {
                                    errors.push(BytecodeValidationError::LayoutOutOfBounds {
                                        func_name: func_name.clone(),
                                        instr_idx,
                                        expected: "ChoiceLayout",
                                        layout_idx: layout_idx.raw(),
                                    });
                                } else {
                                    let layout = &module.choice_layouts[layout_idx.raw() as usize];
                                    if variant_idx as usize >= layout.variants.len() {
                                        errors.push(
                                            BytecodeValidationError::ChoiceVariantOutOfBounds {
                                                func_name: func_name.clone(),
                                                instr_idx,
                                                variant_idx,
                                                variant_count: layout.variants.len(),
                                            },
                                        );
                                    }
                                }
                            }
                            _ => {
                                errors.push(BytecodeValidationError::TypeMismatchAlloc {
                                    func_name: func_name.clone(),
                                    instr_idx,
                                    expected: "Choice",
                                    found: type_idx,
                                });
                            }
                        }
                    }
                }
                Instruction::Cast {
                    dest,
                    src,
                    type_idx,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(src, &mut errors);
                    check_type(type_idx, &mut errors);
                }
                Instruction::Copy { dest, src } => {
                    check_reg(dest, &mut errors);
                    check_reg(src, &mut errors);
                }
                Instruction::Instanceof {
                    dest,
                    src,
                    type_idx,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(src, &mut errors);
                    check_type(type_idx, &mut errors);
                }

                // Category E
                Instruction::Drop { reg } => {
                    check_reg(reg, &mut errors);
                }

                Instruction::CallNative {
                    dest,
                    name_const,
                    args_start,
                    arg_count,
                    arg_types,
                    return_type,
                } => {
                    check_reg(dest, &mut errors);
                    check_const(name_const, &mut errors);
                    check_type(return_type, &mut errors);
                    if arg_types.len() != arg_count as usize {
                        errors.push(BytecodeValidationError::TupleCountMismatch {
                            func_name: func_name.clone(),
                            instr_idx,
                            expected_count: arg_count as usize,
                            found_count: arg_types.len(),
                        });
                    }
                    for arg_type in arg_types {
                        check_type(arg_type, &mut errors);
                    }
                    if arg_count > 0 {
                        check_reg(Reg(args_start.raw() + arg_count as u16 - 1), &mut errors);
                    }
                }
                Instruction::AdapterCall {
                    dest,
                    proxy_module_const,
                    symbol_const,
                    args_start,
                    arg_count,
                    arg_types,
                    return_type,
                } => {
                    check_reg(dest, &mut errors);
                    check_const(proxy_module_const, &mut errors);
                    check_const(symbol_const, &mut errors);
                    check_type(return_type, &mut errors);
                    if arg_types.len() != arg_count as usize {
                        errors.push(BytecodeValidationError::FutureArgumentTypeCountMismatch {
                            func_name: func_name.clone(),
                            instr_idx,
                            expected_count: arg_count as usize,
                            found_count: arg_types.len(),
                        });
                    }
                    for arg_type in arg_types {
                        check_type(arg_type, &mut errors);
                    }
                    if arg_count > 0 {
                        check_reg(Reg(args_start.raw() + arg_count as u16 - 1), &mut errors);
                    }
                }
                Instruction::AwaitFuture {
                    dest,
                    future_id,
                    return_type,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(future_id, &mut errors);
                    check_type(return_type, &mut errors);
                }
                Instruction::CreateFuture {
                    dest,
                    func: func_idx,
                    args_start,
                    arg_count,
                    ref arg_types,
                    return_type,
                } => {
                    check_reg(dest, &mut errors);
                    check_func(func_idx, &mut errors);
                    if arg_types.len() != arg_count as usize {
                        errors.push(BytecodeValidationError::FutureArgumentTypeCountMismatch {
                            func_name: func_name.clone(),
                            instr_idx,
                            expected_count: arg_count as usize,
                            found_count: arg_types.len(),
                        });
                    }
                    for &ty in arg_types {
                        check_type(ty, &mut errors);
                    }
                    check_type(return_type, &mut errors);
                    if arg_count > 0 {
                        check_reg(Reg(args_start.raw() + arg_count as u16 - 1), &mut errors);
                    }
                }
                Instruction::CreateIndirectFuture {
                    dest,
                    func_reg,
                    args_start,
                    arg_count,
                    ref arg_types,
                    return_type,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(func_reg, &mut errors);
                    if arg_types.len() != arg_count as usize {
                        errors.push(BytecodeValidationError::FutureArgumentTypeCountMismatch {
                            func_name: func_name.clone(),
                            instr_idx,
                            expected_count: arg_count as usize,
                            found_count: arg_types.len(),
                        });
                    }
                    for &ty in arg_types {
                        check_type(ty, &mut errors);
                    }
                    check_type(return_type, &mut errors);
                    if arg_count > 0 {
                        check_reg(Reg(args_start.raw() + arg_count as u16 - 1), &mut errors);
                    }
                }
                Instruction::AwaitAll {
                    dest,
                    futures_start,
                    count,
                    return_type,
                }
                | Instruction::AwaitRace {
                    dest,
                    futures_start,
                    count,
                    return_type,
                } => {
                    check_reg(dest, &mut errors);
                    check_type(return_type, &mut errors);
                    if count > 0 {
                        check_reg(Reg(futures_start.raw() + count as u16 - 1), &mut errors);
                    }
                }
                Instruction::Len { dest, src } => {
                    check_reg(dest, &mut errors);
                    check_reg(src, &mut errors);
                }
                Instruction::CopyArray {
                    dest,
                    dest_start,
                    src,
                } => {
                    check_reg(dest, &mut errors);
                    check_reg(dest_start, &mut errors);
                    check_reg(src, &mut errors);
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
