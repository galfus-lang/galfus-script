#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};

use galfus_bytecode::{
    BytecodeGraph, BytecodeNode, ModuleCatalog, ModuleCatalogDerivationError, ModuleResolveContext,
    ModuleResolveError, derive_module_catalog,
};
use galfus_core::ModuleId;

/// Produces one immutable bytecode node for a catalog-declared module.
pub(crate) trait ModuleProducer: Send + Sync {
    fn produce(&self, module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError>;
}

/// Coordinates one materialization attempt for a catalog-declared module.
struct ModuleCell {
    state: Mutex<ModuleCellState>,
    state_changed: Condvar,
}

impl ModuleCell {
    fn unloaded() -> Self {
        Self {
            state: Mutex::new(ModuleCellState::Unloaded),
            state_changed: Condvar::new(),
        }
    }
}

/// Monotonic state for one module during an execution.
enum ModuleCellState {
    Unloaded,
    Loading,
    Ready(Arc<BytecodeNode>),
    Failed(ModuleResolveError),
}

/// Resolves catalog-declared modules with synchronous single-flight production.
pub(crate) struct ModuleResolver {
    catalog: ModuleCatalog,
    cells: HashMap<ModuleId, Arc<ModuleCell>>,
    module_ids: Vec<ModuleId>,
    producer: Arc<dyn ModuleProducer>,
}

/// Deterministic failures while deriving an initializer plan from catalog IDs.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ModuleInitializationPlanError {
    #[error("module {module_id:?} is not declared in the module catalog")]
    UnknownModule { module_id: ModuleId },
    #[error("module initialization dependency cycle: {cycle:?}")]
    DependencyCycle { cycle: Vec<ModuleId> },
}

impl ModuleResolver {
    pub(crate) fn new(catalog: &ModuleCatalog, producer: Arc<dyn ModuleProducer>) -> Self {
        let module_ids = catalog
            .iter()
            .map(|descriptor| descriptor.module_id())
            .collect::<Vec<_>>();
        let cells = module_ids
            .iter()
            .copied()
            .map(|module_id| (module_id, Arc::new(ModuleCell::unloaded())))
            .collect();
        Self {
            catalog: catalog.clone(),
            cells,
            module_ids,
            producer,
        }
    }

    pub(crate) fn ensure_module(
        &self,
        module_id: ModuleId,
    ) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        let cell = self
            .cells
            .get(&module_id)
            .ok_or_else(|| ModuleResolveError::UnknownModule {
                context: ModuleResolveContext::new(module_id),
            })?;

        loop {
            let mut state = cell
                .state
                .lock()
                .expect("module resolver cell state is available");
            match &*state {
                ModuleCellState::Ready(node) => return Ok(node.clone()),
                ModuleCellState::Failed(error) => return Err(error.clone()),
                ModuleCellState::Loading => {
                    drop(
                        cell.state_changed
                            .wait(state)
                            .expect("module resolver cell state is available"),
                    );
                }
                ModuleCellState::Unloaded => {
                    *state = ModuleCellState::Loading;
                    drop(state);

                    let result = self.producer.produce(module_id);
                    let mut state = cell
                        .state
                        .lock()
                        .expect("module resolver cell state is available");
                    match &result {
                        Ok(node) => *state = ModuleCellState::Ready(node.clone()),
                        Err(error) => *state = ModuleCellState::Failed(error.clone()),
                    }
                    cell.state_changed.notify_all();
                    return result;
                }
            }
        }
    }

    /// Materializes every catalog module in canonical ModuleId order.
    pub(crate) fn preload_all(&self) -> Result<(), ModuleResolveError> {
        for module_id in &self.module_ids {
            self.ensure_module(*module_id)?;
        }
        Ok(())
    }

    /// Returns the immutable nodes previously materialized for this execution.
    pub(crate) fn ready_modules(&self) -> Result<Vec<Arc<BytecodeNode>>, ModuleResolveError> {
        self.module_ids
            .iter()
            .copied()
            .map(|module_id| {
                let cell = self
                    .cells
                    .get(&module_id)
                    .expect("module resolver cells are created from module IDs");
                let state = cell
                    .state
                    .lock()
                    .expect("module resolver cell state is available");
                match &*state {
                    ModuleCellState::Ready(node) => Ok(node.clone()),
                    ModuleCellState::Failed(error) => Err(error.clone()),
                    ModuleCellState::Unloaded | ModuleCellState::Loading => {
                        Err(ModuleResolveError::ProducerFailed {
                            context: ModuleResolveContext::new(module_id),
                        })
                    }
                }
            })
            .collect()
    }

    /// Returns every module in dependency-first initializer order for one entry module.
    pub(crate) fn initialization_plan(
        &self,
        entry_module_id: ModuleId,
    ) -> Result<Vec<ModuleId>, ModuleInitializationPlanError> {
        let mut plan = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = Vec::new();
        self.collect_initialization_plan(entry_module_id, &mut plan, &mut visited, &mut visiting)?;
        Ok(plan)
    }

    fn collect_initialization_plan(
        &self,
        module_id: ModuleId,
        plan: &mut Vec<ModuleId>,
        visited: &mut HashSet<ModuleId>,
        visiting: &mut Vec<ModuleId>,
    ) -> Result<(), ModuleInitializationPlanError> {
        if visited.contains(&module_id) {
            return Ok(());
        }
        if let Some(cycle_start) = visiting.iter().position(|id| *id == module_id) {
            let mut cycle = visiting[cycle_start..].to_vec();
            cycle.push(module_id);
            return Err(ModuleInitializationPlanError::DependencyCycle { cycle });
        }
        let descriptor = self
            .catalog
            .get(module_id)
            .ok_or(ModuleInitializationPlanError::UnknownModule { module_id })?;

        visiting.push(module_id);
        for dependency in descriptor.dependencies() {
            self.collect_initialization_plan(dependency.module_id(), plan, visited, visiting)?;
        }
        visiting.pop();
        visited.insert(module_id);
        plan.push(module_id);
        Ok(())
    }
}

/// Eager producer that materializes immutable nodes retained by a bytecode graph.
pub(crate) struct GraphModuleProducer {
    graph: Arc<BytecodeGraph>,
    declared_catalog: Arc<ModuleCatalog>,
    graph_catalog: ModuleCatalog,
}

impl GraphModuleProducer {
    pub(crate) fn new(
        graph: Arc<BytecodeGraph>,
        declared_catalog: Arc<ModuleCatalog>,
    ) -> Result<Self, ModuleCatalogDerivationError> {
        let graph_catalog = derive_module_catalog(graph.as_ref())?;
        Ok(Self {
            graph,
            declared_catalog,
            graph_catalog,
        })
    }
}

impl ModuleProducer for GraphModuleProducer {
    fn produce(&self, module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        let descriptor = self.declared_catalog.get(module_id).ok_or_else(|| {
            ModuleResolveError::UnknownModule {
                context: ModuleResolveContext::new(module_id),
            }
        })?;
        let context = ModuleResolveContext::with_path(module_id, descriptor.module_path().clone());
        let node = self.graph.node_handle(module_id).ok_or_else(|| {
            ModuleResolveError::UnavailableInEager {
                context: context.clone(),
            }
        })?;
        let graph_descriptor = self.graph_catalog.get(module_id).ok_or_else(|| {
            ModuleResolveError::UnavailableInEager {
                context: context.clone(),
            }
        })?;

        if node.id() != descriptor.module_id()
            || node.path() != descriptor.module_path()
            || graph_descriptor.interface_hash() != descriptor.interface_hash()
        {
            return Err(ModuleResolveError::InterfaceMismatch {
                context,
                expected: descriptor.interface_hash(),
                actual: graph_descriptor.interface_hash(),
            });
        }

        Ok(node)
    }
}
