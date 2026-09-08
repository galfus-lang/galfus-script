#![allow(clippy::result_large_err)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod bytecode_emission;
pub mod compile;
pub mod gfp;
pub mod input;
pub mod semantic_to_mir;

#[cfg(test)]
mod mir_tests;

pub use compile::module::{compile_changed_modules, compile_modules, compile_transaction};
pub use input::CompiledModule;

use galfus_core::{FunctionId, ModuleId, SymbolId, TypeId};
use galfus_ir::mir::MirFunction;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct CompilerState {
    pub specialisations: HashMap<(ModuleId, SymbolId, Vec<TypeId>), FunctionId>,
    pub specialised_functions: HashMap<ModuleId, Vec<MirFunction>>,
    pub specialised_id_to_target: HashMap<FunctionId, (ModuleId, FunctionId)>,
    pub pending_specialised_modules: HashSet<ModuleId>,
    pub next_specialised_id: u32,
    /// Interns concrete generic choice identities across every module emitted
    /// by this compiler session.
    pub generic_choice_layouts: bytecode_emission::GenericChoiceLayoutCache,
}

impl Default for CompilerState {
    fn default() -> Self {
        Self {
            specialisations: HashMap::new(),
            specialised_functions: HashMap::new(),
            specialised_id_to_target: HashMap::new(),
            pending_specialised_modules: HashSet::new(),
            next_specialised_id: 0x4000_0000,
            generic_choice_layouts: bytecode_emission::GenericChoiceLayoutCache::default(),
        }
    }
}

impl CompilerState {
    pub fn begin_compilation(
        &mut self,
        live_modules: &HashSet<ModuleId>,
        changed_modules: &HashSet<ModuleId>,
    ) {
        self.specialisations.retain(|(module, _, _), _| {
            live_modules.contains(module) && !changed_modules.contains(module)
        });
        self.specialised_functions
            .retain(|module, _| live_modules.contains(module) && !changed_modules.contains(module));
        self.specialised_id_to_target.retain(|_, (module, _)| {
            live_modules.contains(module) && !changed_modules.contains(module)
        });
        self.pending_specialised_modules.clear();
        self.generic_choice_layouts
            .retain_modules(live_modules, changed_modules);
    }

    pub(crate) fn mark_specialised_module(&mut self, module: ModuleId) {
        self.pending_specialised_modules.insert(module);
    }
}
