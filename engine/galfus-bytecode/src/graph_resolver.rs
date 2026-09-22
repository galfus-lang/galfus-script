#[cfg(test)]
mod tests;

/// A resolved runtime target for one import slot.
use crate::ExportKind;
use crate::graph;
use crate::instruction;

use crate::instruction::FuncIdx;
use galfus_core::{ModuleId, ModulePath, RuntimeExportId, RuntimeExportKind};
use std::collections::HashSet;
use std::sync::Arc;

/// Selects the import identity contract used by a package image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImportResolutionMode {
    /// Resolve the legacy `(module path, symbol name)` import representation.
    Legacy,
    /// Resolve only the direct `(ModuleId, RuntimeExportId)` import representation.
    #[default]
    Direct,
}

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
    /// Resolve each import slot to an exported function in another loaded module.
    pub fn resolve_imports(&self, id: ModuleId) -> Result<ModuleImports, GraphResolutionError> {
        self.resolve_imports_with_mode(id, ImportResolutionMode::Legacy)
    }

    /// Resolves imports according to the identity contract selected by the package format.
    pub fn resolve_imports_with_mode(
        &self,
        id: ModuleId,
        mode: ImportResolutionMode,
    ) -> Result<ModuleImports, GraphResolutionError> {
        let image = self
            .modules
            .get(&id)
            .ok_or(GraphResolutionError::ModuleNotLoaded(id))?;
        let mut imports = Vec::with_capacity(image.module.imports.len());

        for (slot, import) in image.module.imports.iter().enumerate() {
            let (module_id, kind) = match mode {
                ImportResolutionMode::Legacy => self.resolve_legacy_import(id, import)?,
                ImportResolutionMode::Direct => self.resolve_direct_import(id, slot, import)?,
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

    fn resolve_legacy_import(
        &self,
        importer: ModuleId,
        import: &crate::ImportSlot,
    ) -> Result<(ModuleId, ResolvedImportKind), GraphResolutionError> {
        let module_id = ModulePath::new(import.module_name.as_str())
            .and_then(|path| self.ids_by_path.get(&path).copied())
            .ok_or_else(|| GraphResolutionError::ImportModuleNotLoaded {
                importer,
                module_path: import.module_name.clone(),
            })?;
        let target = self
            .modules
            .get(&module_id)
            .expect("path index refers to loaded module");
        let target_export = self
            .export_indexes
            .get(&module_id)
            .and_then(|exports| exports.get(&import.symbol_name))
            .and_then(|index| target.module.exports.get(*index))
            .ok_or_else(|| GraphResolutionError::ImportSymbolNotExported {
                importer,
                module_path: import.module_name.clone(),
                symbol_name: import.symbol_name.clone(),
            })?;
        Ok((module_id, resolved_export_kind(&target_export.kind)))
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

    /// Fills direct target IDs from legacy path/name imports during package migration.
    pub fn populate_direct_import_targets(&mut self) -> Result<(), GraphResolutionError> {
        let module_ids = self
            .modules()
            .map(graph::BytecodeNode::id)
            .collect::<Vec<_>>();
        let mut targets = Vec::new();

        for importer in module_ids {
            for resolved in self
                .resolve_imports_with_mode(importer, ImportResolutionMode::Legacy)?
                .imports
            {
                let target = self
                    .modules
                    .get(&resolved.module_id)
                    .expect("legacy import resolution returns a loaded target module");
                let export = match resolved.kind {
                    ResolvedImportKind::Function(index) => target
                        .module
                        .exports
                        .iter()
                        .find(|export| matches!(export.kind, ExportKind::Function(found) if found == index)),
                    ResolvedImportKind::Global(index) => target
                        .module
                        .exports
                        .iter()
                        .find(|export| matches!(export.kind, ExportKind::Global(found) if found == index)),
                }
                .expect("legacy import resolution returns an exported target");
                targets.push((
                    importer,
                    resolved.slot,
                    resolved.module_id,
                    RuntimeExportId::new(
                        resolved.module_id,
                        export.kind.runtime_export_kind(),
                        export.symbol_name.as_str(),
                    ),
                ));
            }
        }

        for (importer, slot, target_module_id, target_export_id) in targets {
            let importer = self
                .modules
                .get_mut(&importer)
                .expect("legacy import resolution returns a loaded importer module");
            let import = &mut Arc::make_mut(importer).module.imports[slot];
            import.target_module_id = Some(target_module_id);
            import.target_export_id = Some(target_export_id);
        }
        Ok(())
    }

    /// Return modules in dependency-first initialization order for `id`.
    pub fn initialization_order(
        &self,
        id: ModuleId,
    ) -> Result<Vec<ModuleId>, GraphResolutionError> {
        self.initialization_order_with_mode(id, ImportResolutionMode::Legacy)
    }

    /// Returns dependency-first initialization order using the selected import contract.
    pub fn initialization_order_with_mode(
        &self,
        id: ModuleId,
        mode: ImportResolutionMode,
    ) -> Result<Vec<ModuleId>, GraphResolutionError> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        self.collect_initialization_order(id, mode, &mut order, &mut visited, &mut visiting)?;
        Ok(order)
    }

    fn collect_initialization_order(
        &self,
        id: ModuleId,
        mode: ImportResolutionMode,
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

        for import in self.resolve_imports_with_mode(id, mode)?.imports {
            self.collect_initialization_order(import.module_id, mode, order, visited, visiting)?;
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
