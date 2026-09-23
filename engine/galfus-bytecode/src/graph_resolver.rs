#[cfg(test)]
mod tests;

/// A resolved runtime target for one import slot.
use crate::ExportKind;
use crate::graph;
use crate::instruction;

use crate::instruction::FuncIdx;
use galfus_core::{ModuleId, RuntimeExportId, RuntimeExportKind};
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
    #[error("module {importer:?} import slot {slot} has no direct target IDs")]
    MissingDirectImportTarget { importer: ModuleId, slot: usize },
    #[error("module {importer:?} import slot {slot} targets unloaded module {target:?}")]
    DirectImportModuleNotLoaded {
        importer: ModuleId,
        slot: usize,
        target: ModuleId,
    },
    #[error(
        "module {importer:?} import slot {slot} targets missing export {export_id:?} in module {target:?}"
    )]
    DirectImportExportNotFound {
        importer: ModuleId,
        slot: usize,
        target: ModuleId,
        export_id: RuntimeExportId,
    },
    #[error(
        "module {importer:?} import slot {slot} expects export kind {expected:?}, found {actual:?}"
    )]
    DirectImportKindMismatch {
        importer: ModuleId,
        slot: usize,
        expected: RuntimeExportKind,
        actual: RuntimeExportKind,
    },
    #[error("module initialization cycle includes {0:?}")]
    InitializationCycle(ModuleId),
}

impl graph::BytecodeGraph {
    /// Resolves direct module and export IDs for every import slot in one module.
    pub fn resolve_imports(&self, id: ModuleId) -> Result<ModuleImports, GraphResolutionError> {
        let image = self
            .modules
            .get(&id)
            .ok_or(GraphResolutionError::ModuleNotLoaded(id))?;
        let mut imports = Vec::with_capacity(image.module.imports.len());

        for (slot, import) in image.module.imports.iter().enumerate() {
            let (module_id, kind) = self.resolve_direct_import(id, slot, import)?;
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

    fn resolve_direct_import(
        &self,
        importer: ModuleId,
        slot: usize,
        import: &crate::ImportSlot,
    ) -> Result<(ModuleId, ResolvedImportKind), GraphResolutionError> {
        let (target_module_id, target_export_id) =
            match (import.target_module_id, import.target_export_id) {
                (Some(module_id), Some(export_id)) => (module_id, export_id),
                _ => {
                    return Err(GraphResolutionError::MissingDirectImportTarget { importer, slot });
                }
            };
        let target = self.modules.get(&target_module_id).ok_or(
            GraphResolutionError::DirectImportModuleNotLoaded {
                importer,
                slot,
                target: target_module_id,
            },
        )?;
        let target_export = target.module.exports.iter().find(|export| {
            RuntimeExportId::new(
                target_module_id,
                export.kind.runtime_export_kind(),
                export.symbol_name.as_str(),
            ) == target_export_id
        });
        let target_export =
            target_export.ok_or(GraphResolutionError::DirectImportExportNotFound {
                importer,
                slot,
                target: target_module_id,
                export_id: target_export_id,
            })?;
        let expected = import.kind.runtime_export_kind();
        let actual = target_export.kind.runtime_export_kind();
        if expected != actual {
            return Err(GraphResolutionError::DirectImportKindMismatch {
                importer,
                slot,
                expected,
                actual,
            });
        }
        Ok((target_module_id, resolved_export_kind(&target_export.kind)))
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

fn resolved_export_kind(kind: &ExportKind) -> ResolvedImportKind {
    match kind {
        ExportKind::Function(index) => ResolvedImportKind::Function(*index),
        ExportKind::Global(index) => ResolvedImportKind::Global(*index),
    }
}
