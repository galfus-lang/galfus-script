#![no_main]

use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeGraphTransaction, BytecodeModule, BytecodeNode,
    BytecodeType, Constant, ConstantPool, FuncIdx, Instruction, PackageImage, Reg, TypeIdx,
    validate_bytecode_module,
};
use galfus_contract::ExecutionTarget;
use galfus_core::{ModuleId, ModulePath, SemanticRevision};
use libfuzzer_sys::fuzz_target;

fn word(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([
        data.get(offset).copied().unwrap_or_default(),
        data.get(offset + 1).copied().unwrap_or_default(),
    ])
}

fn hostile_module(data: &[u8]) -> BytecodeModule {
    let register = Reg(word(data, 0));
    let function = FuncIdx(word(data, 2));
    let type_idx = TypeIdx(word(data, 4));
    let offset = i32::from_le_bytes([
        data.get(6).copied().unwrap_or_default(),
        data.get(7).copied().unwrap_or_default(),
        data.get(8).copied().unwrap_or_default(),
        data.get(9).copied().unwrap_or_default(),
    ]);
    let instructions = vec![
        Instruction::Call {
            dest: register,
            func: function,
            args_start: register,
            arg_count: data.get(10).copied().unwrap_or_default(),
        },
        Instruction::AllocLocal {
            dest: register,
            type_idx,
        },
        Instruction::Jump { offset },
        Instruction::Ret { src: register },
    ];
    BytecodeModule {
        name: "fuzz".to_string(),
        global_count: u32::from(word(data, 11)),
        constants: ConstantPool {
            constants: vec![Constant::Int32(i32::from_le_bytes([
                data.get(13).copied().unwrap_or_default(),
                data.get(14).copied().unwrap_or_default(),
                data.get(15).copied().unwrap_or_default(),
                data.get(16).copied().unwrap_or_default(),
            ]))],
        },
        functions: vec![BytecodeFunction {
            name: "entry".to_string(),
            param_count: data.get(17).copied().unwrap_or_default(),
            local_count: word(data, 18),
            temp_count: word(data, 20),
            return_ty: type_idx,
            adapter_proxy_metadata: None,
            instructions,
        }],
        types: vec![BytecodeType::Int32],
        struct_layouts: Vec::new(),
        choice_layouts: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        init_func_idx: Some(function),
    }
}

fuzz_target!(|data: &[u8]| {
    let _ = PackageImage::from_bytecode(data);

    let module = hostile_module(data);
    let _ = validate_bytecode_module(&module);

    let graph = BytecodeGraph::new();
    let transaction = BytecodeGraphTransaction {
        base_version: graph.version(),
        semantic_revision: SemanticRevision::new(u64::from(word(data, 22))),
        upserted_modules: vec![BytecodeNode {
            id: ModuleId::new(u32::from(word(data, 24))),
            path: ModulePath::new("fuzz.gfs").expect("static module path is valid"),
            semantic_revision: SemanticRevision::new(u64::from(word(data, 26))),
            module,
            metadata: None,
        }],
        removed_modules: vec![ModuleId::new(u32::from(word(data, 28)))],
        edges: Vec::new(),
    };
    if let Ok(graph) = graph.apply(transaction)
        && let Ok(package) = PackageImage::try_new(
            graph,
            ExecutionTarget::new("fuzz").expect("static execution target is valid"),
            None,
            Vec::new(),
            Vec::new(),
        )
    {
        let _ = package.canonical_bytes();
    }
});
