#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};

#[cfg(test)]
use galfus_bytecode::{BytecodeGraph, ModuleCatalogDerivationError, derive_module_catalog};
use galfus_bytecode::{
    BytecodeNode, ModuleCatalog, ModuleChunk, ModuleChunkDecodingError, ModuleChunkEncodingError,
    ModuleChunkStore, ModuleChunkValidationError, ModuleResolveContext, ModuleResolveError,
};
use galfus_core::ModuleId;

/// Produces one immutable bytecode node for a catalog-declared module.
pub trait ModuleProducer: Send + Sync {
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

/// The next action for a non-blocking module-load request.
pub(crate) enum ModuleLoadRequest {
    Ready(Arc<BytecodeNode>),
    Start,
    Loading,
    Failed(ModuleResolveError),
}

/// Resolves catalog-declared modules with synchronous single-flight production.
pub struct ModuleResolver {
    catalog: ModuleCatalog,
    cells: HashMap<ModuleId, Arc<ModuleCell>>,
    module_ids: Vec<ModuleId>,
    producer: Arc<dyn ModuleProducer>,
}

/// Deterministic failures while deriving an initializer plan from catalog IDs.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModuleInitializationPlanError {
    #[error("module {module_id:?} is not declared in the module catalog")]
    UnknownModule { module_id: ModuleId },
    #[error("module initialization dependency cycle: {cycle:?}")]
    DependencyCycle { cycle: Vec<ModuleId> },
}

impl ModuleResolver {
    pub fn new(catalog: &ModuleCatalog, producer: Arc<dyn ModuleProducer>) -> Self {
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

    pub fn ensure_module(
        &self,
        module_id: ModuleId,
    ) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        loop {
            match self.request_module_load(module_id)? {
                ModuleLoadRequest::Ready(node) => return Ok(node),
                ModuleLoadRequest::Failed(error) => return Err(error),
                ModuleLoadRequest::Start => return self.produce_requested_module(module_id),
                ModuleLoadRequest::Loading => {
                    let cell = self.cell(module_id)?;
                    let state = cell
                        .state
                        .lock()
                        .expect("module resolver cell state is available");
                    if matches!(&*state, ModuleCellState::Loading) {
                        drop(
                            cell.state_changed
                                .wait(state)
                                .expect("module resolver cell state is available"),
                        );
                    }
                }
            }
        }
    }

    /// Starts one materialization attempt without waiting for an in-flight cell.
    pub(crate) fn request_module_load(
        &self,
        module_id: ModuleId,
    ) -> Result<ModuleLoadRequest, ModuleResolveError> {
        let cell = self.cell(module_id)?;
        let mut state = cell
            .state
            .lock()
            .expect("module resolver cell state is available");
        match &*state {
            ModuleCellState::Ready(node) => Ok(ModuleLoadRequest::Ready(node.clone())),
            ModuleCellState::Failed(error) => Ok(ModuleLoadRequest::Failed(error.clone())),
            ModuleCellState::Loading => Ok(ModuleLoadRequest::Loading),
            ModuleCellState::Unloaded => {
                *state = ModuleCellState::Loading;
                Ok(ModuleLoadRequest::Start)
            }
        }
    }

    /// Produces a module previously claimed with [`Self::request_module_load`].
    pub(crate) fn produce_requested_module(
        &self,
        module_id: ModuleId,
    ) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        let result = self.producer.produce(module_id);
        let cell = self.cell(module_id)?;
        let mut state = cell
            .state
            .lock()
            .expect("module resolver cell state is available");
        match &result {
            Ok(node) => *state = ModuleCellState::Ready(node.clone()),
            Err(error) => *state = ModuleCellState::Failed(error.clone()),
        }
        cell.state_changed.notify_all();
        result
    }

    fn cell(&self, module_id: ModuleId) -> Result<&Arc<ModuleCell>, ModuleResolveError> {
        self.cells
            .get(&module_id)
            .ok_or_else(|| ModuleResolveError::UnknownModule {
                context: ModuleResolveContext::new(module_id),
            })
    }

    /// Materializes every catalog module in canonical ModuleId order.
    pub fn preload_all(&self) -> Result<(), ModuleResolveError> {
        for module_id in &self.module_ids {
            self.ensure_module(*module_id)?;
        }
        Ok(())
    }

    /// Returns the immutable nodes previously materialized for this execution.
    pub fn ready_modules(&self) -> Result<Vec<Arc<BytecodeNode>>, ModuleResolveError> {
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

    /// Returns modules materialized so far without requiring unloaded cells.
    pub fn loaded_modules(&self) -> Vec<Arc<BytecodeNode>> {
        self.module_ids
            .iter()
            .filter_map(|module_id| {
                let cell = self
                    .cells
                    .get(module_id)
                    .expect("module resolver cells are created from module IDs");
                let state = cell
                    .state
                    .lock()
                    .expect("module resolver cell state is available");
                match &*state {
                    ModuleCellState::Ready(node) => Some(node.clone()),
                    ModuleCellState::Unloaded
                    | ModuleCellState::Loading
                    | ModuleCellState::Failed(_) => None,
                }
            })
            .collect()
    }

    /// Returns the IDs currently materialized for this execution.
    pub fn loaded_module_ids(&self) -> Vec<ModuleId> {
        self.loaded_modules()
            .into_iter()
            .map(|node| node.id())
            .collect()
    }

    /// Returns every module in dependency-first initializer order for one entry module.
    pub fn initialization_plan(
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

/// Eager producer that validates and decodes immutable package chunks.
pub(crate) struct ChunkModuleProducer {
    chunks: Arc<ModuleChunkStore>,
    catalog: Arc<ModuleCatalog>,
}

impl ChunkModuleProducer {
    pub(crate) fn new(chunks: Arc<ModuleChunkStore>, catalog: Arc<ModuleCatalog>) -> Self {
        Self { chunks, catalog }
    }
}

impl ModuleProducer for ChunkModuleProducer {
    fn produce(&self, module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        let descriptor =
            self.catalog
                .get(module_id)
                .ok_or_else(|| ModuleResolveError::UnknownModule {
                    context: ModuleResolveContext::new(module_id),
                })?;
        let context = ModuleResolveContext::with_path(module_id, descriptor.module_path().clone());
        let chunk =
            self.chunks
                .get(module_id)
                .ok_or_else(|| ModuleResolveError::UnavailableInEager {
                    context: context.clone(),
                })?;
        let bytes = chunk
            .canonical_bytes()
            .map_err(|error| chunk_encoding_error(context.clone(), error))?;
        let decoded = ModuleChunk::from_bytecode(bytes.as_slice())
            .map_err(|error| chunk_decoding_error(context.clone(), error))?;
        decoded
            .verify(descriptor)
            .map_err(|error| chunk_validation_error(context.clone(), error))?;

        Ok(Arc::new(decoded.node().clone()))
    }
}

fn chunk_encoding_error(
    context: ModuleResolveContext,
    error: ModuleChunkEncodingError,
) -> ModuleResolveError {
    match error {
        ModuleChunkEncodingError::Validation(error) => chunk_validation_error(context, error),
        ModuleChunkEncodingError::Postcard(_) => ModuleResolveError::ChunkDecode { context },
    }
}

fn chunk_decoding_error(
    context: ModuleResolveContext,
    error: ModuleChunkDecodingError,
) -> ModuleResolveError {
    match error {
        ModuleChunkDecodingError::Validation(error) => chunk_validation_error(context, error),
        ModuleChunkDecodingError::Postcard(_)
        | ModuleChunkDecodingError::UnexpectedTrailingBytes => {
            ModuleResolveError::ChunkDecode { context }
        }
    }
}

fn chunk_validation_error(
    context: ModuleResolveContext,
    error: ModuleChunkValidationError,
) -> ModuleResolveError {
    match error {
        ModuleChunkValidationError::ContentHashMismatch {
            expected, actual, ..
        } => ModuleResolveError::ChunkHashMismatch {
            context,
            expected,
            actual,
        },
        ModuleChunkValidationError::InterfaceHashMismatch {
            expected, actual, ..
        } => ModuleResolveError::InterfaceMismatch {
            context,
            expected,
            actual,
        },
        ModuleChunkValidationError::NodeModuleIdMismatch { .. }
        | ModuleChunkValidationError::DescriptorModuleIdMismatch { .. }
        | ModuleChunkValidationError::ModulePathMismatch { .. }
        | ModuleChunkValidationError::BytecodeFormat(_)
        | ModuleChunkValidationError::InvalidBytecode { .. }
        | ModuleChunkValidationError::Postcard(_) => ModuleResolveError::ChunkDecode { context },
    }
}

/// Eager producer retained only for migration-equivalence tests and diagnostics.
#[cfg(test)]
pub(crate) struct GraphModuleProducer {
    graph: Arc<BytecodeGraph>,
    declared_catalog: Arc<ModuleCatalog>,
    graph_catalog: ModuleCatalog,
}

#[cfg(test)]
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

#[cfg(test)]
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
