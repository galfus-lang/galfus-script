use crate::ExportKind;

use super::*;

fn create_dummy_module(instructions: Vec<Instruction>) -> BytecodeModule {
    BytecodeModule {
        name: "test".to_string(),
        constants: ConstantPool {
            constants: vec![Constant::Int64(42)],
        },
        functions: vec![BytecodeFunction {
            name: "main".to_string(),
            param_count: 1,
            local_count: 2,
            temp_count: 2,
            return_ty: TypeIdx(0),
            instructions,
        }],
        types: vec![
            BytecodeType::Int64,
            BytecodeType::Struct(StructLayoutIdx(0)),
            BytecodeType::Tuple(vec![TypeIdx(0), TypeIdx(0)]),
            BytecodeType::Choice(ChoiceLayoutIdx(0)),
        ],
        struct_layouts: vec![StructLayout {
            name: "Point".to_string(),
            fields: vec![FieldLayout {
                name: "x".to_string(),
                ty: TypeIdx(0),
                offset: 0,
                ownership: OwnershipKind::Value,
            }],
            constraints: vec![],
        }],
        choice_layouts: vec![ChoiceLayout {
            name: "Option".to_string(),
            variants: vec![
                ChoiceVariantLayout {
                    name: "None".to_string(),
                    payload_ty: None,
                },
                ChoiceVariantLayout {
                    name: "Some".to_string(),
                    payload_ty: Some(TypeIdx(0)),
                },
            ],
        }],
        imports: vec![],
        exports: vec![],
        init_func_idx: None,
    }
}

#[test]
fn test_valid_module() {
    let image = create_dummy_module(vec![
        Instruction::Move {
            dest: Reg(1),
            src: Reg(0),
        },
        Instruction::Ret { src: Reg(1) },
    ]);
    assert!(validate_bytecode_module(&image).is_ok());
}

#[test]
fn test_invalid_register() {
    let image = create_dummy_module(vec![Instruction::Move {
        dest: Reg(10),
        src: Reg(0),
    }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::InvalidRegister { ref reg, .. } if reg.raw() == 10
    ));
}

#[test]
fn test_invalid_constant_index() {
    let image = create_dummy_module(vec![Instruction::LoadConst {
        dest: Reg(1),
        const_idx: ConstIdx(5),
    }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::InvalidConstantIndex {
            index: ConstIdx(5),
            ..
        }
    ));
}

#[test]
fn test_invalid_jump_offset() {
    let image = create_dummy_module(vec![Instruction::Jump { offset: 10 }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::InvalidJumpOffset { target_idx: 11, .. }
    ));
}

#[test]
fn test_type_mismatch_alloc() {
    // TypeIdx(0) is Int64, not Struct
    let image = create_dummy_module(vec![Instruction::AllocLocal {
        dest: Reg(1),
        type_idx: TypeIdx(0),
    }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::TypeMismatchAlloc {
            found: TypeIdx(0),
            expected: "Struct",
            ..
        }
    ));
}

#[test]
fn test_tuple_count_mismatch() {
    // TypeIdx(2) is Tuple with 2 elements, but count: 3 is requested
    let image = create_dummy_module(vec![Instruction::NewTuple {
        dest: Reg(1),
        type_idx: TypeIdx(2),
        start: Reg(0),
        count: 3,
    }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::TupleCountMismatch {
            expected_count: 2,
            found_count: 3,
            ..
        }
    ));
}

#[test]
fn test_invalid_function_index() {
    let image = create_dummy_module(vec![Instruction::Call {
        dest: Reg(1),
        func: FuncIdx(10),
        args_start: Reg(0),
        arg_count: 0,
    }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::InvalidFunctionIndex {
            index: FuncIdx(10),
            ..
        }
    ));
}

#[test]
fn test_choice_variant_out_of_bounds() {
    // TypeIdx(3) is Choice Option, which only has variants 0 and 1
    let image = create_dummy_module(vec![Instruction::NewChoice {
        dest: Reg(1),
        type_idx: TypeIdx(3),
        variant_idx: 5,
        payload: Reg(0),
    }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::ChoiceVariantOutOfBounds {
            variant_idx: 5,
            variant_count: 2,
            ..
        }
    ));
}

#[test]
fn test_field_out_of_bounds() {
    // Point struct layout only has field 0
    let image = create_dummy_module(vec![Instruction::LoadField {
        dest: Reg(1),
        obj: Reg(0),
        field: FieldIdx(2),
    }]);
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::FieldOutOfBounds {
            field_idx: FieldIdx(2),
            ..
        }
    ));
}

#[test]
fn test_export_function_out_of_bounds() {
    let mut image = create_dummy_module(vec![Instruction::Ret { src: Reg(0) }]);
    image.exports.push(ExportSlot {
        symbol_name: "compute".to_string(),
        kind: ExportKind::Function(FuncIdx(10)),
    });
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::ExportFunctionOutOfBounds {
            func_idx: FuncIdx(10),
            ..
        }
    ));
}

#[test]
fn test_init_function_out_of_bounds() {
    let mut image = create_dummy_module(vec![Instruction::Ret { src: Reg(0) }]);
    image.init_func_idx = Some(FuncIdx(10));
    let res = validate_bytecode_module(&image);
    assert!(res.is_err());
    let errs = res.unwrap_err();
    assert!(matches!(
        errs[0],
        BytecodeValidationError::InitFunctionOutOfBounds {
            func_idx: FuncIdx(10),
            ..
        }
    ));
}
