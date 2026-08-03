use crate::StringTable;
use crate::modules::{SemanticModule, SemanticModuleGraph};
use galfus_core::SemanticRevision;

/// Immutable frontend result consumed by the compilation phase.
#[derive(Clone)]
pub struct FrontendSnapshot {
    semantic_revision: SemanticRevision,
    modules: Vec<SemanticModule>,
    semantic_graph: SemanticModuleGraph,
    string_table: StringTable,
}

impl FrontendSnapshot {
    pub(crate) fn new(
        semantic_revision: SemanticRevision,
        modules: Vec<SemanticModule>,
        semantic_graph: SemanticModuleGraph,
        string_table: StringTable,
    ) -> Self {
        Self {
            semantic_revision,
            modules,
            semantic_graph,
            string_table,
        }
    }

    pub fn semantic_revision(&self) -> SemanticRevision {
        self.semantic_revision
    }

    pub fn modules(&self) -> &[SemanticModule] {
        self.modules.as_slice()
    }

    pub fn semantic_graph(&self) -> &SemanticModuleGraph {
        &self.semantic_graph
    }

    pub fn string_table(&self) -> &StringTable {
        &self.string_table
    }
}
