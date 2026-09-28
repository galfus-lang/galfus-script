#[cfg(test)]
#[path = "source_producer/metrics.rs"]
mod metrics;
mod verification;

use crate::state::WorkspaceModuleCatalog;
use crate::workspace::WorkspaceTimingCollector;
use galfus_bytecode::{BytecodeNode, ModuleResolveContext, ModuleResolveError};
use galfus_compiler::{CompiledModule, CompilerState, compile_changed_modules};
use galfus_core::{ModuleId, Revision};
use galfus_frontend::modules::FrontendSnapshot;
use galfus_runtime::ModuleProducer;
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

#[cfg(test)]
use metrics::{SourceProducerCounterSet, SourceProducerWorkCounters};

/// Lazily compiles source bodies from one checked workspace snapshot.
#[allow(dead_code)]
pub(crate) struct WorkspaceSourceProducer {
    snapshot: FrontendSnapshot,
    catalog: Arc<WorkspaceModuleCatalog>,
    revision_guard: Arc<AtomicU64>,
    module_guard: Arc<std::sync::RwLock<HashSet<ModuleId>>>,
    timing_collector: Option<Arc<WorkspaceTimingCollector>>,
    compiler_state: Mutex<CompilerState>,
    compiled_nodes: Mutex<HashMap<ModuleId, Arc<BytecodeNode>>>,
    #[cfg(test)]
    work_counters: SourceProducerCounterSet,
}

#[allow(dead_code)]
impl WorkspaceSourceProducer {
    pub(crate) fn new(
        snapshot: FrontendSnapshot,
        catalog: Arc<WorkspaceModuleCatalog>,
        revision_guard: Arc<AtomicU64>,
        module_guard: Arc<std::sync::RwLock<HashSet<ModuleId>>>,
        timing_collector: Option<Arc<WorkspaceTimingCollector>>,
    ) -> Self {
        Self {
            snapshot,
            catalog,
            revision_guard,
            module_guard,
            timing_collector,
            compiler_state: Mutex::new(CompilerState::default()),
            compiled_nodes: Mutex::new(HashMap::new()),
            #[cfg(test)]
            work_counters: SourceProducerCounterSet::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn production_count(&self, module_id: ModuleId) -> usize {
        self.work_counters.production_count(module_id)
    }

    #[cfg(test)]
    pub(crate) fn work_counters(&self) -> SourceProducerWorkCounters {
        self.work_counters.snapshot()
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
            #[cfg(test)]
            self.work_counters.record_cached_node_hit();
            self.verify_node(node.as_ref(), descriptor, context)?;
            return Ok(node.as_ref().clone());
        }
        #[cfg(test)]
        self.work_counters.record_production(module_id);
        let started = Instant::now();
        let targets = self.dependency_closure(module_id)?;
        let mut target_module_ids = targets.iter().copied().collect::<Vec<_>>();
        target_module_ids.sort_by_key(|target_module_id| target_module_id.raw());
        #[cfg(test)]
        self.work_counters
            .record_semantic_graph_inspection(target_module_ids.len());
        let mut modules = target_module_ids
            .into_iter()
            .map(|target_module_id| self.compiled_module(target_module_id))
            .collect::<Result<Vec<_>, ModuleResolveError>>()?;
        #[cfg(test)]
        self.work_counters.record_dependency_closure(targets.len());
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
        #[cfg(test)]
        self.work_counters.record_compiled_nodes(nodes.len());
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
        if let Some(timing_collector) = &self.timing_collector {
            timing_collector.record_first_source_node_production(started.elapsed());
        }
        Ok(node.as_ref().clone())
    }

    fn compiled_module(&self, module_id: ModuleId) -> Result<CompiledModule, ModuleResolveError> {
        let descriptor = self.catalog.get(module_id).ok_or_else(|| {
            ModuleResolveError::SourceModuleUnavailable {
                context: self.context(module_id),
            }
        })?;
        let module = self
            .snapshot
            .semantic_graph()
            .get(module_id)
            .ok_or_else(|| ModuleResolveError::SourceModuleUnavailable {
                context: self.context(module_id),
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
}

impl ModuleProducer for WorkspaceSourceProducer {
    fn produce(&self, module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        self.compile_module(module_id).map(Arc::new)
    }
}
