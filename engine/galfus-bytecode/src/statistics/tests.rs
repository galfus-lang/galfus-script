use super::*;
use crate::{BytecodeFunction, ConstantPool, FuncIdx, Reg, TypeIdx};

#[test]
fn module_statistics_classify_calls_branches_and_frames() {
    let module = BytecodeModule {
        name: "stats.gfs".to_string(),
        global_count: 0,
        constants: ConstantPool::default(),
        functions: vec![
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 1,
                local_count: 2,
                temp_count: 3,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![
                    Instruction::Call {
                        dest: Reg(1),
                        func: FuncIdx(1),
                        args_start: Reg(0),
                        arg_count: 1,
                    },
                    Instruction::Call {
                        dest: Reg(1),
                        func: FuncIdx(2),
                        args_start: Reg(0),
                        arg_count: 1,
                    },
                    Instruction::CallDynamic {
                        dest: Reg(1),
                        func_reg: Reg(0),
                        args_start: Reg(0),
                        arg_count: 1,
                    },
                    Instruction::JumpFalse {
                        cond: Reg(0),
                        offset: 0,
                    },
                    Instruction::CreateFuture {
                        dest: Reg(1),
                        func: FuncIdx(1),
                        args_start: Reg(0),
                        arg_count: 1,
                        arg_types: Box::new([]),
                        return_type: TypeIdx(0),
                    },
                ],
            },
            BytecodeFunction {
                name: "callee".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![Instruction::RetNull],
            },
        ],
        types: Vec::new(),
        struct_layouts: Vec::new(),
        choice_layouts: Vec::new(),
        imports: vec![],
        exports: vec![],
        init_func_idx: None,
    };

    let statistics = collect_module_statistics(&module, "stats.gfs");

    assert_eq!(statistics.function_count, 2);
    assert_eq!(statistics.instruction_count, 6);
    assert_eq!(statistics.frame_register_count, 6);
    assert_eq!(statistics.local_call_count, 1);
    assert_eq!(statistics.import_call_count, 1);
    assert_eq!(statistics.dynamic_call_count, 1);
    assert_eq!(statistics.branch_count, 1);
    assert_eq!(statistics.future_creation_count, 1);
}
