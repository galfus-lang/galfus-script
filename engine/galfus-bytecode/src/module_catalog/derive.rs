#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use galfus_contract::ContentHash;
use galfus_core::{
    RuntimeExportId, RuntimeExportIdCollision, RuntimeExportIdRegistry, RuntimeExportIdentity,
};
use serde::Serialize;

use super::{
    ModuleCatalog, ModuleCatalogError, ModuleDependency, ModuleDescriptor, ModuleDescriptorError,
    ModuleExportDescriptor,
};
use crate::{
    BytecodeGraph, BytecodeGraphValidationErrors, BytecodeNode, canonical_node_content_hash,
};

/// Derives the immutable module catalog that exactly describes a validated graph.
pub fn derive_module_catalog(
    graph: &BytecodeGraph,
) -> Result<ModuleCatalog, ModuleCatalogDerivationError> {
    graph.validate()?;

    let graph_edges = graph
        .edges()
        .iter()
        .map(|edge| (edge.from, edge.to))
        .collect::<BTreeSet<_>>();
    let mut descriptors = Vec::with_capacity(graph.len());
    for node in graph.modules() {
        descriptors.push(derive_descriptor(graph, node, &graph_edges)?);
    }

    validate_export_ids(descriptors.as_slice())?;
    let catalog = ModuleCatalog::new(descriptors)?;
    cross_check_catalog(graph, &catalog, &graph_edges)?;
    Ok(catalog)
}

fn derive_descriptor(
    graph: &BytecodeGraph,
    node: &BytecodeNode,
    graph_edges: &BTreeSet<(galfus_core::ModuleId, galfus_core::ModuleId)>,
) -> Result<ModuleDescriptor, ModuleCatalogDerivationError> {
    let dependencies = direct_dependencies(graph, node, graph_edges)?;
    let exports = node
        .module()
        .exports
        .iter()
        .map(|export| {
            let kind = export.kind.runtime_export_kind();
            ModuleExportDescriptor::new(
                RuntimeExportId::new(node.id(), kind, export.symbol_name.as_str()),
                export.symbol_name.clone(),
                kind,
            )
        })
        .collect();
    let has_initializer = node.module().init_func_idx.is_some();
    let placeholder_hash = ContentHash::of(&[]);
    let descriptor = ModuleDescriptor::new(
        node.id(),
        node.path().clone(),
        dependencies,
        exports,
        has_initializer,
        placeholder_hash,
        placeholder_hash,
    )?;
    let interface_hash = interface_hash(&descriptor)?;
    let chunk_hash = chunk_hash(node)?;

    Ok(ModuleDescriptor::new(
        descriptor.module_id(),
        descriptor.module_path().clone(),
        descriptor.dependencies().to_vec(),
        descriptor.exports().to_vec(),
        descriptor.has_initializer(),
        interface_hash,
        chunk_hash,
    )?)
}

fn direct_dependencies(
    graph: &BytecodeGraph,
    node: &BytecodeNode,
    graph_edges: &BTreeSet<(galfus_core::ModuleId, galfus_core::ModuleId)>,
) -> Result<Vec<ModuleDependency>, ModuleCatalogDerivationError> {
    let mut dependencies = BTreeSet::new();
    for import in &node.module().imports {
        let (Some(target_module_id), Some(target_export_id)) =
            (import.target_module_id, import.target_export_id)
        else {
            return Err(ModuleCatalogDerivationError::MissingDirectImportTarget {
                importer: node.id(),
                module_path: import.module_name.clone(),
                symbol_name: import.symbol_name.clone(),
            });
        };
        let target = graph.get(target_module_id).ok_or(
            ModuleCatalogDerivationError::MissingDirectTargetModule {
                importer: node.id(),
                target: target_module_id,
            },
        )?;
        if target.path().as_str() != import.module_name {
            return Err(ModuleCatalogDerivationError::DirectTargetPathMismatch {
                importer: node.id(),
                target: target_module_id,
                import_path: import.module_name.clone(),
                target_path: target.path().clone(),
            });
        }
        let target_kind = target.module().exports.iter().find_map(|export| {
            let kind = export.kind.runtime_export_kind();
            let export_id =
                RuntimeExportId::new(target_module_id, kind, export.symbol_name.as_str());
            (export_id == target_export_id).then_some(kind)
        });
        let Some(target_kind) = target_kind else {
            return Err(ModuleCatalogDerivationError::MissingDirectTargetExport {
                importer: node.id(),
                target: target_module_id,
                export_id: target_export_id,
            });
        };
        let import_kind = import.kind.runtime_export_kind();
        if target_kind != import_kind {
            return Err(ModuleCatalogDerivationError::DirectTargetKindMismatch {
                importer: node.id(),
                target: target_module_id,
                import_kind,
                target_kind,
            });
        }
        if !graph_edges.contains(&(node.id(), target_module_id)) {
            return Err(ModuleCatalogDerivationError::MissingGraphEdge {
                importer: node.id(),
                dependency: target_module_id,
            });
        }
        dependencies.insert(ModuleDependency::new(target_module_id));
    }
    Ok(dependencies.into_iter().collect())
}

fn validate_export_ids(
    descriptors: &[ModuleDescriptor],
) -> Result<(), ModuleCatalogDerivationError> {
    let mut identities = descriptors
        .iter()
        .flat_map(|descriptor| {
            descriptor.exports().iter().map(move |export| {
                RuntimeExportIdentity::new(descriptor.module_id(), export.kind(), export.name())
            })
        })
        .collect::<Vec<_>>();
    identities.sort_unstable();

    let mut registry = RuntimeExportIdRegistry::default();
    for identity in identities {
        registry.register(identity)?;
    }
    Ok(())
}

fn cross_check_catalog(
    graph: &BytecodeGraph,
    catalog: &ModuleCatalog,
    graph_edges: &BTreeSet<(galfus_core::ModuleId, galfus_core::ModuleId)>,
) -> Result<(), ModuleCatalogDerivationError> {
    if catalog.len() != graph.len() {
        return Err(ModuleCatalogDerivationError::ModuleCountMismatch {
            catalog: catalog.len(),
            graph: graph.len(),
        });
    }
    for descriptor in catalog.iter() {
        let node = graph.get(descriptor.module_id()).ok_or(
            ModuleCatalogDerivationError::MissingGraphModule {
                module_id: descriptor.module_id(),
            },
        )?;
        if node.path() != descriptor.module_path() {
            return Err(ModuleCatalogDerivationError::ModulePathMismatch {
                module_id: descriptor.module_id(),
                catalog_path: descriptor.module_path().clone(),
                graph_path: node.path().clone(),
            });
        }
        for dependency in descriptor.dependencies() {
            if !graph_edges.contains(&(descriptor.module_id(), dependency.module_id())) {
                return Err(ModuleCatalogDerivationError::MissingGraphEdge {
                    importer: descriptor.module_id(),
                    dependency: dependency.module_id(),
                });
            }
        }
    }
    Ok(())
}

fn interface_hash(
    descriptor: &ModuleDescriptor,
) -> Result<ContentHash, ModuleCatalogDerivationError> {
    let bytes = postcard::to_stdvec(&ModuleInterface {
        module_id: descriptor.module_id(),
        module_path: descriptor.module_path(),
        dependencies: descriptor.dependencies(),
        exports: descriptor.exports(),
        has_initializer: descriptor.has_initializer(),
    })?;
    Ok(ContentHash::of(bytes.as_slice()))
}

fn chunk_hash(node: &BytecodeNode) -> Result<ContentHash, ModuleCatalogDerivationError> {
    Ok(canonical_node_content_hash(node)?)
}

#[derive(Serialize)]
struct ModuleInterface<'a> {
    module_id: galfus_core::ModuleId,
    module_path: &'a galfus_core::ModulePath,
    dependencies: &'a [ModuleDependency],
    exports: &'a [ModuleExportDescriptor],
    has_initializer: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleCatalogDerivationError {
    #[error(transparent)]
    Graph(#[from] BytecodeGraphValidationErrors),
    #[error(
        "module {importer:?} import `{symbol_name}` from `{module_path}` has no direct target IDs"
    )]
    MissingDirectImportTarget {
        importer: galfus_core::ModuleId,
        module_path: String,
        symbol_name: String,
    },
    #[error("module {importer:?} refers to absent direct target module {target:?}")]
    MissingDirectTargetModule {
        importer: galfus_core::ModuleId,
        target: galfus_core::ModuleId,
    },
    #[error(
        "module {importer:?} names `{import_path}` for direct target {target:?}, whose path is `{target_path}`"
    )]
    DirectTargetPathMismatch {
        importer: galfus_core::ModuleId,
        target: galfus_core::ModuleId,
        import_path: String,
        target_path: galfus_core::ModulePath,
    },
    #[error(
        "module {importer:?} refers to absent direct export {export_id:?} in module {target:?}"
    )]
    MissingDirectTargetExport {
        importer: galfus_core::ModuleId,
        target: galfus_core::ModuleId,
        export_id: RuntimeExportId,
    },
    #[error(
        "module {importer:?} imports {import_kind:?} from module {target:?}, whose direct target has kind {target_kind:?}"
    )]
    DirectTargetKindMismatch {
        importer: galfus_core::ModuleId,
        target: galfus_core::ModuleId,
        import_kind: galfus_core::RuntimeExportKind,
        target_kind: galfus_core::RuntimeExportKind,
    },
    #[error("module {importer:?} depends on {dependency:?}, but the graph has no matching edge")]
    MissingGraphEdge {
        importer: galfus_core::ModuleId,
        dependency: galfus_core::ModuleId,
    },
    #[error("catalog contains {catalog} modules, but graph contains {graph}")]
    ModuleCountMismatch { catalog: usize, graph: usize },
    #[error("catalog module {module_id:?} is absent from the graph")]
    MissingGraphModule { module_id: galfus_core::ModuleId },
    #[error("catalog module {module_id:?} has path `{catalog_path}`, but graph has `{graph_path}`")]
    ModulePathMismatch {
        module_id: galfus_core::ModuleId,
        catalog_path: galfus_core::ModulePath,
        graph_path: galfus_core::ModulePath,
    },
    #[error(transparent)]
    RuntimeExportIdCollision(#[from] RuntimeExportIdCollision),
    #[error(transparent)]
    Descriptor(#[from] ModuleDescriptorError),
    #[error(transparent)]
    Catalog(#[from] ModuleCatalogError),
    #[error("could not encode module catalog material: {0}")]
    Postcard(#[from] postcard::Error),
}
