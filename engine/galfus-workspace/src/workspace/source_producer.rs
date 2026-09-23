use crate::state::WorkspaceModuleCatalog;
use galfus_bytecode::{
    BytecodeNode, ModuleDependency, ModuleDescriptor, ModuleExportDescriptor, ModuleResolveContext,
    ModuleResolveError,
};
use galfus_compiler::{CompiledModule, CompilerState, compile_changed_modules};
use galfus_core::{ModuleId, Revision, RuntimeExportId};
use galfus_frontend::modules::FrontendSnapshot;
use galfus_runtime::ModuleProducer;
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

/// Lazily compiles source bodies from one checked workspace snapshot.
#[allow(dead_code)]
pub(crate) struct WorkspaceSourceProducer {
    snapshot: FrontendSnapshot,
    catalog: Arc<WorkspaceModuleCatalog>,
    revision_guard: Arc<AtomicU64>,
    module_guard: Arc<std::sync::RwLock<HashSet<ModuleId>>>,
    compiler_state: Mutex<CompilerState>,
    compiled_nodes: Mutex<HashMap<ModuleId, Arc<BytecodeNode>>>,
    #[cfg(test)]
    production_counts: Mutex<HashMap<ModuleId, usize>>,
}

#[allow(dead_code)]
impl WorkspaceSourceProducer {
    pub(crate) fn new(
        snapshot: FrontendSnapshot,
        catalog: Arc<WorkspaceModuleCatalog>,
        revision_guard: Arc<AtomicU64>,
        module_guard: Arc<std::sync::RwLock<HashSet<ModuleId>>>,
    ) -> Self {
        Self {
            snapshot,
            catalog,
            revision_guard,
            module_guard,
            compiler_state: Mutex::new(CompilerState::default()),
            compiled_nodes: Mutex::new(HashMap::new()),
            #[cfg(test)]
            production_counts: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    pub(crate) fn production_count(&self, module_id: ModuleId) -> usize {
        self.production_counts
            .lock()
            .expect("workspace source producer test counters are available")
            .get(&module_id)
            .copied()
            .unwrap_or_default()
    }

    fn context(&self, module_id: ModuleId) -> ModuleResolveContext {
        self.catalog.get(module_id).map_or_else(
            || ModuleResolveContext::new(module_id),
            |descriptor| {
                ModuleResolveContext::with_path(module_id, descriptor.module_path().clone())
            },
        )
    }

    fn ensure_snapshot_current(
        &self,
        context: ModuleResolveContext,
    ) -> Result<(), ModuleResolveError> {
        let actual = Revision::new(self.revision_guard.load(Ordering::Acquire));
        let expected = self.catalog.source_revision();
        if actual == expected {
            Ok(())
        } else {
            Err(ModuleResolveError::SourceSnapshotChanged {
                context,
                expected,
                actual,
            })
        }
    }

    fn ensure_source_available(
        &self,
        module_id: ModuleId,
        context: ModuleResolveContext,
    ) -> Result<(), ModuleResolveError> {
        self.module_guard
            .read()
            .expect("workspace source module guard is available")
            .contains(&module_id)
            .then_some(())
            .ok_or(ModuleResolveError::SourceModuleUnavailable { context })
    }

    fn compile_module(&self, module_id: ModuleId) -> Result<BytecodeNode, ModuleResolveError> {
        let context = self.context(module_id);
        let descriptor =
            self.catalog
                .get(module_id)
                .ok_or_else(|| ModuleResolveError::UnknownModule {
                    context: context.clone(),
                })?;
        self.ensure_source_available(module_id, context.clone())?;
        self.ensure_snapshot_current(context.clone())?;
        if let Some(node) = self
            .compiled_nodes
            .lock()
            .expect("workspace source producer node cache is available")
            .get(&module_id)
            .cloned()
        {
            self.verify_node(node.as_ref(), descriptor, context)?;
            return Ok(node.as_ref().clone());
        }
        #[cfg(test)]
        {
            let mut counts = self
                .production_counts
                .lock()
                .expect("workspace source producer test counters are available");
            *counts.entry(module_id).or_default() += 1;
        }
        let mut modules = self
            .snapshot
            .modules()
            .iter()
            .map(|module| {
                let descriptor = self.catalog.get(module.id()).ok_or_else(|| {
                    ModuleResolveError::SourceModuleUnavailable {
                        context: self.context(module.id()),
                    }
                })?;
                Ok(CompiledModule::new(
                    descriptor.module_id(),
                    descriptor.module_path().clone(),
                    module.semantic_revision(),
                    module.source().clone(),
                    module.graph().clone(),
                    module.type_result().cloned(),
                    module.source().name().ends_with(".gfp"),
                ))
            })
            .collect::<Result<Vec<_>, ModuleResolveError>>()?;
        if !modules.iter().any(|module| module.id() == module_id) {
            return Err(ModuleResolveError::SourceModuleUnavailable { context });
        }

        let targets = self.dependency_closure(module_id)?;
        let mut compiler_state = self
            .compiler_state
            .lock()
            .expect("workspace source producer compiler state is available");
        let nodes = compile_changed_modules(
            modules.as_mut_slice(),
            &mut compiler_state,
            &targets,
            self.snapshot.string_table(),
        )
        .map_err(|_| ModuleResolveError::ProducerFailed {
            context: context.clone(),
        })?;
        drop(compiler_state);

        self.ensure_snapshot_current(context.clone())?;
        let mut cached_nodes = self
            .compiled_nodes
            .lock()
            .expect("workspace source producer node cache is available");
        for node in nodes {
            cached_nodes.insert(node.id(), Arc::new(node));
        }
        let node = cached_nodes.get(&module_id).cloned().ok_or_else(|| {
            ModuleResolveError::SourceModuleUnavailable {
                context: context.clone(),
            }
        })?;
        drop(cached_nodes);
        self.verify_node(node.as_ref(), descriptor, context)?;
        Ok(node.as_ref().clone())
    }

    fn dependency_closure(
        &self,
        module_id: ModuleId,
    ) -> Result<HashSet<ModuleId>, ModuleResolveError> {
        let mut pending = vec![module_id];
        let mut closure = HashSet::new();
        while let Some(current) = pending.pop() {
            if !closure.insert(current) {
                continue;
            }
            let descriptor =
                self.catalog
                    .get(current)
                    .ok_or_else(|| ModuleResolveError::UnknownModule {
                        context: self.context(current),
                    })?;
            pending.extend(descriptor.dependencies().iter().copied());
        }
        Ok(closure)
    }

    fn verify_node(
        &self,
        node: &BytecodeNode,
        descriptor: &crate::state::WorkspaceModuleDescriptor,
        context: ModuleResolveContext,
    ) -> Result<(), ModuleResolveError> {
        if node.id() != descriptor.module_id() || node.path() != descriptor.module_path() {
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

impl ModuleProducer for WorkspaceSourceProducer {
    fn produce(&self, module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        self.compile_module(module_id).map(Arc::new)
    }
}
