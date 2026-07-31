pub mod bytecode_emission;
pub mod compile;
pub mod input;
pub mod semantic_to_mir;

#[cfg(test)]
mod mir_tests;

pub use compile::module::{compile_changed_modules, compile_modules, compile_transaction};
pub use input::CompiledModule;

use galfus_core::{FunctionId, ModuleId, SymbolId, TypeId};
use galfus_ir::mir::MirFunction;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CompilerState {
    pub specialisations: HashMap<(ModuleId, SymbolId, Vec<TypeId>), FunctionId>,
    pub specialised_functions: HashMap<ModuleId, Vec<MirFunction>>,
    pub specialised_id_to_target: HashMap<FunctionId, (ModuleId, FunctionId)>,
    pub next_specialised_id: u32,
}

impl Default for CompilerState {
    fn default() -> Self {
        Self {
            specialisations: HashMap::new(),
            specialised_functions: HashMap::new(),
            specialised_id_to_target: HashMap::new(),
            next_specialised_id: 0x4000_0000,
        }
    }
}
