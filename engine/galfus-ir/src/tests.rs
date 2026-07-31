use super::*;
use galfus_core::{FunctionId, TypeId};

#[test]
fn test_pure_mir_construction() {
    let mut func = mir::MirFunction {
        id: FunctionId::new(0),
        name: "test_pure".to_string(),
        return_type: TypeId::new(0),
        parameter_types: vec![],
        locals: vec![],
        blocks: vec![],
        type_substitutions: std::collections::HashMap::new(),
        is_async: false,
    };

    let block = mir::BasicBlock {
        id: mir::BlockId::new(0),
        parameters: vec![],
        instructions: vec![],
        terminator: (mir::Terminator::Return(None), None),
    };
    func.blocks.push(block);

    let module = mir::MirModule {
        functions: vec![func],
        globals: vec![],
        constant_pool: vec![],
    };

    let result = validator::validate_module(&module);
    assert!(result.is_ok());
}
