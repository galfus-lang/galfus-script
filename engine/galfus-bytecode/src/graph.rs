/// Optional source mapping attached to a compiled module.
#[cfg(test)]
mod tests;

use crate::ExportKind;
use crate::instruction;

use crate::{BytecodeModule, BytecodeValidationError, validate_bytecode_module};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct ExecutionMetadata {
    pub spans: HashMap<instruction::FuncIdx, HashMap<usize, galfus_core::Span>>,
}

impl ExecutionMetadata {
    pub fn span_for(
        &self,
        function: instruction::FuncIdx,
        instruction_offset: usize,
    ) -> Option<galfus_core::Span> {
        self.spans
            .get(&function)
            .and_then(|spans| spans.get(&instruction_offset))
            .copied()
    }
}

/// The compiled artifact for one source module.
#[derive(Debug, Clone)]
pub struct BytecodeNode {
    pub id: ModuleId,
    pub path: ModulePath,
    pub semantic_revision: SemanticRevision,
    pub module: BytecodeModule,
    pub metadata: Option<ExecutionMetadata>,
}

impl BytecodeNode {
    pub fn id(&self) -> ModuleId {
        self.id
    }

    pub fn path(&self) -> &ModulePath {
        &self.path
    }

    pub fn semantic_revision(&self) -> SemanticRevision {
        self.semantic_revision
    }

    pub fn module(&self) -> &BytecodeModule {
        &self.module
    }
}

/// An edge where `from` imports a symbol from `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEdge {
    pub from: ModuleId,
    pub to: ModuleId,
}

/// A complete update to a bytecode graph snapshot.
#[derive(Debug, Clone)]
pub struct BytecodeGraphTransaction {
    pub base_version: u64,
    pub semantic_revision: SemanticRevision,
    pub upserted_modules: Vec<BytecodeNode>,
    pub removed_modules: Vec<ModuleId>,
    pub edges: Vec<ImportEdge>,
}

#[derive(Debug, thiserror::Error)]
pub enum BytecodeGraphValidationError {
    #[error("module {module_id:?} contains invalid bytecode: {errors:?}")]
    InvalidModule {
        module_id: ModuleId,
        errors: Vec<BytecodeValidationError>,
    },
    #[error("module path `{path}` is registered by both {first:?} and {second:?}")]
    DuplicateModulePath {
        path: ModulePath,
        first: ModuleId,
        second: ModuleId,
    },
    #[error("dependency edge {from:?} -> {to:?} references a module not in the graph")]
    MissingDependencyModule { from: ModuleId, to: ModuleId },
    #[error("module {importer:?} accesses globals owned by missing module {owner:?}")]
    MissingGlobalModule { importer: ModuleId, owner: ModuleId },
    #[error("module {importer:?} imports an invalid module path `{module_path}`")]
    InvalidImportPath {
        importer: ModuleId,
        module_path: String,
    },
    #[error("module {importer:?} imports module `{module_path}`, which is not in the graph")]
    MissingImportedModule {
        importer: ModuleId,
        module_path: String,
    },
    #[error(
        "module {importer:?} imports `{symbol_name}` from `{module_path}`, but it is not exported"
    )]
    MissingImportedExport {
        importer: ModuleId,
        module_path: String,
        symbol_name: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum BytecodeGraphTransactionError {
    #[error("transaction targets graph version {expected}, but current version is {actual}")]
    StaleBaseVersion { expected: u64, actual: u64 },
    #[error(transparent)]
    InvalidGraph(#[from] BytecodeGraphValidationError),
}

/// The immutable executable graph published by a workspace.
#[derive(Debug, Clone, Default)]
pub struct BytecodeGraph {
    version: u64,
    pub(crate) modules: HashMap<ModuleId, BytecodeNode>,
    pub(crate) ids_by_path: HashMap<ModulePath, ModuleId>,
    pub(crate) edges: Vec<ImportEdge>,
}

impl BytecodeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// Construct the first validated graph snapshot from complete module data.
    pub fn from_modules(
        semantic_revision: SemanticRevision,
        modules: Vec<BytecodeNode>,
        edges: Vec<ImportEdge>,
    ) -> Result<Self, BytecodeGraphTransactionError> {
        Self::new().apply(BytecodeGraphTransaction {
            base_version: 0,
            semantic_revision,
            upserted_modules: modules,
            removed_modules: Vec::new(),
            edges,
        })
    }

    /// Validate the complete graph, including bytecode, imports, exports, and edges.
    pub fn validate(&self) -> Result<(), BytecodeGraphValidationError> {
        let mut ids_by_path = HashMap::new();
        for (id, node) in &self.modules {
            if let Some(first) = ids_by_path.insert(node.path.clone(), *id)
                && first != *id
            {
                return Err(BytecodeGraphValidationError::DuplicateModulePath {
                    path: node.path.clone(),
                    first,
                    second: *id,
                });
            }
            if let Err(errors) = validate_bytecode_module(&node.module) {
                return Err(BytecodeGraphValidationError::InvalidModule {
                    module_id: *id,
                    errors,
                });
            }
            for function in &node.module.functions {
                for instruction in &function.instructions {
                    let owner_global = match instruction {
                        instruction::Instruction::LoadGlobal {
                            module_id,
                            global_idx,
                            ..
                        }
                        | instruction::Instruction::StoreGlobal {
                            module_id,
                            global_idx,
                            ..
                        } => Some((*module_id, *global_idx)),
                        _ => None,
                    };
                    if let Some((owner, global_idx)) = owner_global
                        && owner != *id
                    {
                        if let Some(owner_node) = self.modules.get(&owner) {
                            let is_exported = owner_node.module.exports.iter().any(
                                |e| matches!(e.kind, ExportKind::Global(idx) if idx == global_idx),
                            );
                            if !is_exported {
                                return Err(BytecodeGraphValidationError::MissingImportedExport {
                                    importer: *id,
                                    module_path: owner_node.path.as_str().to_string(),
                                    symbol_name: format!("global_{}", global_idx.raw()),
                                });
                            }
                        } else {
                            return Err(BytecodeGraphValidationError::MissingGlobalModule {
                                importer: *id,
                                owner,
                            });
                        }
                    }
                }
            }
        }

        for edge in &self.edges {
            if !self.modules.contains_key(&edge.from) || !self.modules.contains_key(&edge.to) {
                return Err(BytecodeGraphValidationError::MissingDependencyModule {
                    from: edge.from,
                    to: edge.to,
                });
            }
        }

        for (importer, node) in &self.modules {
            for import in &node.module.imports {
                let path = ModulePath::new(import.module_name.as_str()).ok_or_else(|| {
                    BytecodeGraphValidationError::InvalidImportPath {
                        importer: *importer,
                        module_path: import.module_name.clone(),
                    }
                })?;
                let dependency = ids_by_path.get(&path).copied().ok_or_else(|| {
                    BytecodeGraphValidationError::MissingImportedModule {
                        importer: *importer,
                        module_path: import.module_name.clone(),
                    }
                })?;
                let dependency_node = self
                    .modules
                    .get(&dependency)
                    .expect("module path index refers to a graph node");
                if !dependency_node
                    .module
                    .exports
                    .iter()
                    .any(|export| export.symbol_name == import.symbol_name)
                {
                    return Err(BytecodeGraphValidationError::MissingImportedExport {
                        importer: *importer,
                        module_path: import.module_name.clone(),
                        symbol_name: import.symbol_name.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Apply a transaction to a clone and return the next validated snapshot.
    pub fn apply(
        &self,
        transaction: BytecodeGraphTransaction,
    ) -> Result<Self, BytecodeGraphTransactionError> {
        if transaction.base_version != self.version {
            return Err(BytecodeGraphTransactionError::StaleBaseVersion {
                expected: transaction.base_version,
                actual: self.version,
            });
        }

        let mut next = self.clone();
        for id in transaction.removed_modules {
            if let Some(node) = next.modules.remove(&id) {
                next.ids_by_path.remove(&node.path);
            }
        }
        for node in transaction.upserted_modules {
            if let Some(previous) = next.modules.insert(node.id, node.clone()) {
                next.ids_by_path.remove(&previous.path);
            }
            next.ids_by_path.insert(node.path.clone(), node.id);
        }
        next.edges = transaction.edges;
        next.validate()?;
        next.version += 1;
        Ok(next)
    }

    pub fn get(&self, id: ModuleId) -> Option<&BytecodeNode> {
        self.modules.get(&id)
    }

    pub fn modules(&self) -> impl Iterator<Item = &BytecodeNode> {
        self.modules.values()
    }

    pub fn edges(&self) -> &[ImportEdge] {
        self.edges.as_slice()
    }

    pub fn deps_of(&self, id: ModuleId) -> impl Iterator<Item = ModuleId> + '_ {
        self.edges
            .iter()
            .filter(move |edge| edge.from == id)
            .map(|edge| edge.to)
    }

    pub fn dependents_of(&self, id: ModuleId) -> Vec<ModuleId> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.collect_dependents(id, &mut result, &mut visited);
        result
    }

    fn collect_dependents(
        &self,
        id: ModuleId,
        out: &mut Vec<ModuleId>,
        visited: &mut HashSet<ModuleId>,
    ) {
        for edge in &self.edges {
            if edge.to == id && visited.insert(edge.from) {
                out.push(edge.from);
                self.collect_dependents(edge.from, out, visited);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }
}
