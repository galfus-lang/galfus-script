mod module_graph;

use crate::source_store;

use galfus_bytecode::{
    ModuleCatalog, ModuleCatalogError, ModuleDependency, ModuleDescriptor, ModuleExportDescriptor,
    PackageImage,
};
use galfus_compiler::CompilerState;
use galfus_contract::ContentHash;
use galfus_core::{DiagnosticBag, ModuleId, ModulePath, Revision, SemanticRevision};
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

pub use module_graph::{
    IncrementalModuleStateGraph, ModuleLifecycle, ModuleStateError, ModuleStateRecord,
};

#[derive(Debug)]
pub enum CheckState {
    Dirty {
        current_revision: Revision,
        previous_checked_revision: Option<Revision>,
    },
    Passed {
        revision: Revision,
        semantic_revision: SemanticRevision,
        changed_modules: HashSet<ModuleId>,
        diagnostics: DiagnosticBag,
    },
    Failed {
        revision: Revision,
        diagnostics: DiagnosticBag,
    },
}

impl CheckState {
    pub fn is_dirty(&self) -> bool {
        matches!(self, Self::Dirty { .. })
    }

    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Passed { .. })
    }
}

/// Reason why `Workspace::compile()` cannot proceed.
#[derive(Debug)]
pub enum CompileBlocked {
    /// Sources changed but `check()` has not been called yet.
    Dirty {
        current_revision: Revision,
        checked_revision: Option<Revision>,
    },
    /// The last `check()` produced errors — compilation is gated behind a clean check.
    CheckFailed {
        revision: Revision,
        error_count: usize,
    },
    /// No workspace configuration has been loaded.
    MissingConfiguration,
    /// The compiler itself returned an error.
    CompilerError(String),
}

/// Reason why `Workspace::run()` cannot proceed.
#[derive(Debug)]
pub enum RunBlocked {
    /// `check()` has not produced an up-to-date source snapshot.
    CheckRequired,
    /// The configured entry module is not in the checked module catalog.
    EntryModuleMissing,
}

/// Reason why `Workspace::run()` failed.
#[derive(Debug)]
pub enum WorkspaceRunError {
    /// The execution could not be started due to an unchecked workspace state.
    Blocked(RunBlocked),
    /// The checked source catalog could not be turned into a runtime catalog.
    SourceSetup(String),
    /// The execution failed to initialize due to runtime rejection (e.g. panic or missing export).
    RuntimeStart(galfus_runtime::RuntimeError),
    /// The execution started but failed with an error.
    ExecutionFailed(galfus_contract::ExecutionFailure),
}

pub struct SourceState {
    pub store: source_store::SourceStore,
    pub revision: Revision,
    pub dirty_sources: HashSet<galfus_core::ModulePath>,
    pub removed_modules: Vec<ModuleId>,
    revision_guard: Arc<AtomicU64>,
    module_guard: Arc<RwLock<HashSet<ModuleId>>>,
}

impl Default for SourceState {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceState {
    pub fn new() -> Self {
        Self {
            store: source_store::SourceStore::new(),
            revision: Revision::new(1),
            dirty_sources: HashSet::new(),
            removed_modules: Vec::new(),
            revision_guard: Arc::new(AtomicU64::new(1)),
            module_guard: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn advance_revision(&mut self) {
        self.revision.next();
        self.revision_guard
            .store(self.revision.0, Ordering::Release);
    }

    pub fn revision_guard(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.revision_guard)
    }

    pub fn track_loaded_module(&self, module_id: ModuleId) {
        self.module_guard
            .write()
            .expect("workspace source module guard is available")
            .insert(module_id);
    }

    pub fn track_removed_module(&self, module_id: ModuleId) {
        self.module_guard
            .write()
            .expect("workspace source module guard is available")
            .remove(&module_id);
    }

    pub fn module_guard(&self) -> Arc<RwLock<HashSet<ModuleId>>> {
        Arc::clone(&self.module_guard)
    }
}

pub struct SemanticState {
    /// Canonical incremental lifecycle state for every known module identity.
    pub module_states: IncrementalModuleStateGraph,
    /// Transitional command gate. It does not own module lifecycle state.
    pub check_state: CheckState,
    pub module_catalog: Option<Arc<WorkspaceModuleCatalog>>,
}

impl Default for SemanticState {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticState {
    pub fn new() -> Self {
        Self {
            module_states: IncrementalModuleStateGraph::new(),
            check_state: CheckState::Dirty {
                current_revision: Revision::new(1),
                previous_checked_revision: None,
            },
            module_catalog: None,
        }
    }
}

/// Frozen workspace interface known before bytecode bodies are materialized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceModuleDescriptor {
    module_id: ModuleId,
    module_path: ModulePath,
    dependencies: Vec<ModuleId>,
    provider_modules: Vec<ModuleId>,
    adapter_proxy_modules: Vec<ModuleId>,
    exports: Vec<String>,
    runtime_exports: Vec<ModuleExportDescriptor>,
    has_initializer: bool,
    semantic_interface_hash: ContentHash,
    interface_hash: ContentHash,
}

impl WorkspaceModuleDescriptor {
    pub fn new(
        module_id: ModuleId,
        module_path: ModulePath,
        mut dependencies: Vec<ModuleId>,
        mut provider_modules: Vec<ModuleId>,
        mut adapter_proxy_modules: Vec<ModuleId>,
        mut exports: Vec<String>,
        mut runtime_exports: Vec<ModuleExportDescriptor>,
        has_initializer: bool,
    ) -> Self {
        dependencies.sort_unstable();
        dependencies.dedup();
        provider_modules.sort_unstable();
        provider_modules.dedup();
        adapter_proxy_modules.sort_unstable();
        adapter_proxy_modules.dedup();
        exports.sort_unstable();
        exports.dedup();
        runtime_exports.sort_unstable_by(|left, right| {
            left.runtime_export_id()
                .cmp(&right.runtime_export_id())
                .then_with(|| left.name().cmp(right.name()))
                .then_with(|| left.kind().cmp(&right.kind()))
        });
        runtime_exports
            .dedup_by(|left, right| left.runtime_export_id() == right.runtime_export_id());
        let mut identity = format!(
            "{}\n{}\n{}\n",
            module_id.raw(),
            module_path,
            has_initializer
        );
        for dependency in &dependencies {
            identity.push_str(&format!("{}\n", dependency.raw()));
        }
        for provider_module in &provider_modules {
            identity.push_str(&format!("provider:{}\n", provider_module.raw()));
        }
        for adapter_proxy_module in &adapter_proxy_modules {
            identity.push_str(&format!("adapter:{}\n", adapter_proxy_module.raw()));
        }
        for export in &exports {
            identity.push_str(export);
            identity.push('\n');
        }
        let runtime_dependencies = dependencies
            .iter()
            .copied()
            .map(ModuleDependency::new)
            .collect::<Vec<_>>();
        let interface_hash = ModuleDescriptor::new_with_capability_requirements(
            module_id,
            module_path.clone(),
            runtime_dependencies,
            provider_modules.clone(),
            adapter_proxy_modules.clone(),
            runtime_exports.clone(),
            has_initializer,
            ContentHash::of(&[]),
            ContentHash::of(&[]),
        )
        .expect("workspace runtime exports form a valid module descriptor")
        .computed_interface_hash()
        .expect("workspace module interface serializes");
        Self {
            module_id,
            module_path,
            dependencies,
            provider_modules,
            adapter_proxy_modules,
            exports,
            runtime_exports,
            has_initializer,
            semantic_interface_hash: ContentHash::of(identity.as_bytes()),
            interface_hash,
        }
    }

    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub fn module_path(&self) -> &ModulePath {
        &self.module_path
    }

    pub fn dependencies(&self) -> &[ModuleId] {
        self.dependencies.as_slice()
    }

    pub fn provider_modules(&self) -> &[ModuleId] {
        self.provider_modules.as_slice()
    }

    pub fn adapter_proxy_modules(&self) -> &[ModuleId] {
        self.adapter_proxy_modules.as_slice()
    }

    pub fn exports(&self) -> &[String] {
        self.exports.as_slice()
    }

    pub fn runtime_exports(&self) -> &[ModuleExportDescriptor] {
        self.runtime_exports.as_slice()
    }

    pub const fn semantic_interface_hash(&self) -> ContentHash {
        self.semantic_interface_hash
    }

    pub const fn has_initializer(&self) -> bool {
        self.has_initializer
    }

    pub const fn interface_hash(&self) -> ContentHash {
        self.interface_hash
    }
}

/// Immutable workspace interface catalog paired with one source and semantic snapshot.
#[derive(Clone, Debug)]
pub struct WorkspaceModuleCatalog {
    source_revision: Revision,
    semantic_revision: SemanticRevision,
    descriptors: Vec<WorkspaceModuleDescriptor>,
    indexes_by_id: HashMap<ModuleId, usize>,
}

impl WorkspaceModuleCatalog {
    pub fn new(
        source_revision: Revision,
        semantic_revision: SemanticRevision,
        mut descriptors: Vec<WorkspaceModuleDescriptor>,
    ) -> Self {
        descriptors.sort_unstable_by_key(WorkspaceModuleDescriptor::module_id);
        let indexes_by_id = descriptors
            .iter()
            .enumerate()
            .map(|(index, descriptor)| (descriptor.module_id(), index))
            .collect();
        Self {
            source_revision,
            semantic_revision,
            descriptors,
            indexes_by_id,
        }
    }

    pub const fn source_revision(&self) -> Revision {
        self.source_revision
    }

    pub const fn semantic_revision(&self) -> SemanticRevision {
        self.semantic_revision
    }

    pub fn get(&self, module_id: ModuleId) -> Option<&WorkspaceModuleDescriptor> {
        self.indexes_by_id
            .get(&module_id)
            .map(|index| &self.descriptors[*index])
    }

    pub fn iter(&self) -> impl Iterator<Item = &WorkspaceModuleDescriptor> {
        self.descriptors.iter()
    }

    pub const fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn runtime_catalog(&self) -> Result<ModuleCatalog, ModuleCatalogError> {
        let descriptors = self
            .descriptors
            .iter()
            .map(|descriptor| {
                ModuleDescriptor::new_with_capability_requirements(
                    descriptor.module_id(),
                    descriptor.module_path().clone(),
                    descriptor
                        .dependencies()
                        .iter()
                        .copied()
                        .map(ModuleDependency::new)
                        .collect(),
                    descriptor.provider_modules().to_vec(),
                    descriptor.adapter_proxy_modules().to_vec(),
                    descriptor.runtime_exports().to_vec(),
                    descriptor.has_initializer(),
                    descriptor.interface_hash(),
                    ContentHash::of(&[]),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        ModuleCatalog::new(descriptors)
    }
}

pub struct BytecodeState {
    pub compile_state: CompileState,
    pub compiler_state: CompilerState,
}

impl Default for BytecodeState {
    fn default() -> Self {
        Self::new()
    }
}

impl BytecodeState {
    pub fn new() -> Self {
        Self {
            compile_state: CompileState::Missing,
            compiler_state: CompilerState::default(),
        }
    }
}

#[derive(Debug)]
pub enum CompileState {
    /// No compilation has ever been attempted.
    Missing,
    /// A previous compiled graph exists but is stale (check result changed).
    Stale {
        semantic_revision: SemanticRevision,
        package: Arc<PackageImage>,
        unfinalized_package: Arc<PackageImage>,
    },
    /// A compiled package is available and up-to-date with the last check.
    Ready {
        semantic_revision: SemanticRevision,
        /// The immutable package produced by the last successful compile.
        package: Arc<PackageImage>,
        /// Pre-finalization graph retained for incremental invalidation.
        unfinalized_package: Arc<PackageImage>,
    },
    /// The last compilation attempt failed.
    Failed {
        semantic_revision: SemanticRevision,
        error: String,
    },
}

impl CompileState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn package(&self) -> Option<&Arc<PackageImage>> {
        match self {
            Self::Ready { package, .. } | Self::Stale { package, .. } => Some(package),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum WorkspaceError {
    InvalidPath,
    SourceLoad(crate::workspace::SourceLoadError),
    ReservedProviderModule(String),
    MissingConfiguration,
    Collision {
        attempted: String,
        existing: String,
        identity: source_store::IdentityKind,
        id: u32,
    },
}
