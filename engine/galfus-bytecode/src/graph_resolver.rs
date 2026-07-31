/// A resolved runtime target for one import slot.
use crate::ExportKind;
use crate::graph;
use crate::instruction;

use crate::instruction::FuncIdx;
use galfus_core::{ModuleId, ModulePath};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedImportKind {
    Function(FuncIdx),
    Global(instruction::GlobalIdx),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    pub slot: usize,
    pub module_id: ModuleId,
    pub kind: ResolvedImportKind,
}

/// The dynamic linking result for one loaded module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleImports {
    pub module_id: ModuleId,
    pub imports: Vec<ResolvedImport>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GraphResolutionError {
    #[error("module {0:?} is not loaded")]
    ModuleNotLoaded(ModuleId),
    #[error("module {importer:?} imports unloaded module `{module_path}`")]
    ImportModuleNotLoaded {
        importer: ModuleId,
        module_path: String,
    },
    #[error("module {importer:?} imports missing function `{symbol_name}` from `{module_path}`")]
    ImportSymbolNotExported {
        importer: ModuleId,
        module_path: String,
        symbol_name: String,
    },
    #[error("module initialization cycle includes {0:?}")]
    InitializationCycle(ModuleId),
}

impl graph::BytecodeGraph {
    /// Resolve each import slot to an exported function in another loaded module.
    pub fn resolve_imports(&self, id: ModuleId) -> Result<ModuleImports, GraphResolutionError> {
        let image = self
            .modules
            .get(&id)
            .ok_or(GraphResolutionError::ModuleNotLoaded(id))?;
        let mut imports = Vec::with_capacity(image.module.imports.len());

        for (slot, import) in image.module.imports.iter().enumerate() {
            let module_id = ModulePath::new(import.module_name.as_str())
                .and_then(|path| self.ids_by_path.get(&path).copied())
                .ok_or_else(|| GraphResolutionError::ImportModuleNotLoaded {
                    importer: id,
                    module_path: import.module_name.clone(),
                })?;
            let target = self
                .modules
                .get(&module_id)
                .expect("path index refers to loaded module");
            let target_export = target
                .module
                .exports
                .iter()
                .find(|export| export.symbol_name == import.symbol_name)
                .ok_or_else(|| GraphResolutionError::ImportSymbolNotExported {
                    importer: id,
                    module_path: import.module_name.clone(),
                    symbol_name: import.symbol_name.clone(),
                })?;

            let kind = match target_export.kind {
                ExportKind::Function(idx) => ResolvedImportKind::Function(idx),
                ExportKind::Global(idx) => ResolvedImportKind::Global(idx),
            };

            imports.push(ResolvedImport {
                slot,
                module_id,
                kind,
            });
        }

        Ok(ModuleImports {
            module_id: id,
            imports,
        })
    }

    /// Return modules in dependency-first initialization order for `id`.
    pub fn initialization_order(
        &self,
        id: ModuleId,
    ) -> Result<Vec<ModuleId>, GraphResolutionError> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        self.collect_initialization_order(id, &mut order, &mut visited, &mut visiting)?;
        Ok(order)
    }

    fn collect_initialization_order(
        &self,
        id: ModuleId,
        order: &mut Vec<ModuleId>,
        visited: &mut HashSet<ModuleId>,
        visiting: &mut HashSet<ModuleId>,
    ) -> Result<(), GraphResolutionError> {
        if visited.contains(&id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err(GraphResolutionError::InitializationCycle(id));
        }

        for import in self.resolve_imports(id)?.imports {
            self.collect_initialization_order(import.module_id, order, visited, visiting)?;
        }

        visiting.remove(&id);
        visited.insert(id);
        order.push(id);
        Ok(())
    }
}
