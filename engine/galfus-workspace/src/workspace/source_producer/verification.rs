use super::WorkspaceSourceProducer;
use crate::state::WorkspaceModuleDescriptor;
use galfus_bytecode::{
    BytecodeNode, ModuleDependency, ModuleDescriptor, ModuleExportDescriptor, ModuleResolveContext,
    ModuleResolveError,
};
use galfus_core::RuntimeExportId;

impl WorkspaceSourceProducer {
    pub(super) fn verify_node(
        &self,
        node: &BytecodeNode,
        descriptor: &WorkspaceModuleDescriptor,
        context: ModuleResolveContext,
    ) -> Result<(), ModuleResolveError> {
        if node.id() != descriptor.module_id()
            || node.path() != descriptor.module_path()
            || self
                .snapshot
                .semantic_graph()
                .get(node.id())
                .is_none_or(|module| module.semantic_revision() != node.semantic_revision())
        {
            return Err(ModuleResolveError::SourceInterfaceMismatch { context });
        }

        for import in &node.module().imports {
            let (Some(target_module_id), Some(_target_export_id)) =
                (import.target_module_id, import.target_export_id)
            else {
                return Err(ModuleResolveError::SourceInterfaceMismatch { context });
            };
            let Some(target) = self.catalog.get(target_module_id) else {
                return Err(ModuleResolveError::SourceInterfaceMismatch { context });
            };
            if !descriptor.dependencies().contains(&target_module_id)
                || target.module_path().as_str() != import.module_name
            {
                return Err(ModuleResolveError::SourceInterfaceMismatch { context });
            }
        }

        let mut exports = node
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
            .collect::<Vec<_>>();
        exports.sort_unstable();
        let dependencies = descriptor
            .dependencies()
            .iter()
            .copied()
            .map(ModuleDependency::new)
            .collect::<Vec<_>>();
        let actual = ModuleDescriptor::new_with_capability_requirements(
            node.id(),
            node.path().clone(),
            dependencies,
            descriptor.provider_modules().to_vec(),
            descriptor.adapter_proxy_modules().to_vec(),
            exports,
            node.module().init_func_idx.is_some(),
            descriptor.interface_hash(),
            galfus_contract::ContentHash::of(&[]),
        )
        .map_err(|_| ModuleResolveError::SourceInterfaceMismatch {
            context: context.clone(),
        })?;
        if actual.has_initializer() != descriptor.has_initializer() {
            return Err(ModuleResolveError::SourceInterfaceMismatch { context });
        }
        Ok(())
    }
}
