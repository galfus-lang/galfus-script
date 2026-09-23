use std::collections::HashMap;
use std::sync::Arc;

use galfus_bytecode::instruction::TypeIdx;
use galfus_bytecode::{BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType};
use galfus_core::ModuleId;

use crate::error::VmError;

/// Immutable module lookup table for one VM execution.
#[derive(Clone)]
pub(super) struct VmModuleRegistry {
    modules: HashMap<ModuleId, Arc<BytecodeNode>>,
    uint8_type_indexes: HashMap<ModuleId, Option<TypeIdx>>,
}

impl VmModuleRegistry {
    pub(super) fn from_graph(graph: &BytecodeGraph) -> Self {
        let modules = graph.modules().map(BytecodeNode::id).map(|module_id| {
            graph
                .node_handle(module_id)
                .expect("graph module IDs always have immutable node handles")
        });
        Self::from_ready_modules(modules)
    }

    pub(super) fn from_ready_modules(modules: impl IntoIterator<Item = Arc<BytecodeNode>>) -> Self {
        let modules = modules
            .into_iter()
            .map(|module| (module.id(), module))
            .collect::<HashMap<_, _>>();
        let uint8_type_indexes = modules
            .iter()
            .map(|(module_id, node)| {
                (
                    *module_id,
                    node.module
                        .types
                        .iter()
                        .position(|ty| matches!(ty, BytecodeType::Uint8))
                        .map(|index| TypeIdx(index as u16)),
                )
            })
            .collect();
        Self {
            modules,
            uint8_type_indexes,
        }
    }

    pub(super) fn get_module(&self, module_id: ModuleId) -> Result<&BytecodeModule, VmError> {
        self.modules
            .get(&module_id)
            .map(|node| &node.module)
            .ok_or(VmError::ModuleNotReady { module_id })
    }

    pub(super) fn get_node(&self, module_id: ModuleId) -> Result<Arc<BytecodeNode>, VmError> {
        self.modules
            .get(&module_id)
            .cloned()
            .ok_or(VmError::ModuleNotReady { module_id })
    }

    pub(super) fn contains(&self, module_id: ModuleId) -> bool {
        self.modules.contains_key(&module_id)
    }

    pub(super) fn modules(&self) -> impl Iterator<Item = (ModuleId, &BytecodeModule)> {
        self.modules
            .iter()
            .map(|(module_id, node)| (*module_id, &node.module))
    }

    pub(super) fn uint8_type_idx(&self, module_id: ModuleId) -> Option<TypeIdx> {
        self.uint8_type_indexes.get(&module_id).copied().flatten()
    }
}
