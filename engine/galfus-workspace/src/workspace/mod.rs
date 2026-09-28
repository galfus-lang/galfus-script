pub mod compilation;
pub mod dependency;
pub mod execution;
pub mod module;
mod optimizer;
mod source_loader;
mod source_producer;
mod timing;

#[cfg(test)]
mod tests;

use std::str;

use crate::config::{WORKSPACE_SOURCE_ID, WorkspaceConfig, parse_workspace_config};
use crate::source_store::ModuleOrigin;
use crate::state::{
    BytecodeState, CheckState, CompileState, SemanticState, SourceState, WorkspaceError,
};
use galfus_bytecode::{PackageEntryPoint, PackageImage};
use galfus_compiler::{CompiledModule, gfp::parse_gfp_frontmatter};
use galfus_contract::ContentHash;
use galfus_contract::{
    AdapterFunctionSignature, AdapterModuleDescriptor, ExecutionTarget, Providers,
    RuntimeCapabilities,
};
use galfus_core::{DiagnosticBag, ModuleId, ModulePath, SourceFile};
use galfus_frontend::modules::{
    FrontendModuleKind, FrontendSession, FrontendSnapshot, FrontendSource, FrontendUpdate,
    SemanticRoot, SemanticRootKind,
};
use std::collections::HashMap;
use std::sync::Arc;

pub use source_loader::{
    LoadedWorkspaceSource, SourceLoadError, SourceLoadErrorKind, SourceRequestResult,
    WorkspaceSourceLoader,
};
pub use timing::{WorkspaceRuntimeTiming, WorkspaceTimingCollector};

pub struct Workspace {
    pub root_path: Option<std::path::PathBuf>,
    pub config: Option<WorkspaceConfig>,
    pub source_state: SourceState,
    pub semantic_state: SemanticState,
    pub bytecode_state: BytecodeState,
    pub frontend: FrontendSession,
    frontend_snapshot: Option<FrontendSnapshot>,
    source_loader: Option<Arc<dyn WorkspaceSourceLoader>>,
    timing_collector: Option<Arc<WorkspaceTimingCollector>>,
    pub catalog: Arc<galfus_contract::CapabilityCatalog>,
    pub adapter_descriptors: HashMap<ModulePath, AdapterModuleDescriptor>,
}

pub enum LoadResult {
    Success,
    Diagnostics(DiagnosticBag),
}

pub enum RemoveResult {
    Success,
    NotFound,
}

pub struct CheckReport<'a> {
    pub is_valid: bool,
    pub diagnostics: &'a DiagnosticBag,
}

/// Result of a successful `compile()` call.
pub struct CompileReport {
    /// The immutable compiled package, ready to be delivered to a host.
    pub package: Arc<PackageImage>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            root_path: None,
            config: None,
            source_state: SourceState::new(),
            semantic_state: SemanticState::new(),
            bytecode_state: BytecodeState::new(),
            frontend: FrontendSession::new(),
            frontend_snapshot: None,
            source_loader: None,
            timing_collector: None,
            catalog: Arc::new(galfus_contract::CapabilityCatalog::default()),
            adapter_descriptors: HashMap::new(),
        }
    }

    pub fn set_catalog(&mut self, catalog: Arc<galfus_contract::CapabilityCatalog>) {
        if self.catalog.fingerprint() != catalog.fingerprint() {
            self.source_state.advance_revision();
            let removed = self
                .source_state
                .store
                .remove_by_origin(ModuleOrigin::ProviderCatalog);
            for entry in removed {
                self.source_state.dirty_sources.remove(&entry.path);
                self.source_state.removed_modules.push(entry.module_id);
                self.source_state.track_removed_module(entry.module_id);
                self.record_removed_source(entry.module_id, entry.path);
            }
            self.catalog = catalog;
            self.mark_dirty();
        }
    }

    /// Installs optional timing collection for one workspace operation.
    pub fn set_timing_collector(&mut self, collector: Arc<WorkspaceTimingCollector>) {
        self.timing_collector = Some(collector);
    }

    /// Installs the backend used to load source modules requested on demand.
    pub fn set_source_loader(&mut self, source_loader: Arc<dyn WorkspaceSourceLoader>) {
        self.source_loader = Some(source_loader);
    }

    pub fn load_manifest(
        &mut self,
        manifest: crate::config::WorkspaceManifest,
    ) -> Result<LoadResult, WorkspaceError> {
        let mut diagnostics = DiagnosticBag::new();
        if let Some(config) = parse_workspace_config(manifest, &mut diagnostics) {
            self.config = Some(config);
            self.mark_dirty();
            Ok(LoadResult::Success)
        } else {
            Ok(LoadResult::Diagnostics(diagnostics))
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.semantic_state.check_state.is_dirty()
    }

    pub(crate) fn mark_dirty(&mut self) {
        let previous = match &self.semantic_state.check_state {
            CheckState::Passed { revision, .. } | CheckState::Failed { revision, .. } => {
                Some(*revision)
            }
            CheckState::Dirty {
                previous_checked_revision,
                ..
            } => *previous_checked_revision,
        };

        self.semantic_state.check_state = CheckState::Dirty {
            current_revision: self.source_state.revision,
            previous_checked_revision: previous,
        };
        self.frontend_snapshot = None;

        if let CompileState::Ready {
            semantic_revision,
            package,
            unfinalized_package,
        } = &self.bytecode_state.compile_state
        {
            self.bytecode_state.compile_state = CompileState::Stale {
                semantic_revision: *semantic_revision,
                package: Arc::clone(package),
                unfinalized_package: Arc::clone(unfinalized_package),
            };
        }
    }

    pub(crate) fn record_loaded_source(
        &mut self,
        module_id: ModuleId,
        module_path: ModulePath,
        source_bytes: &[u8],
    ) {
        self.semantic_state
            .module_states
            .source_loaded(
                module_id,
                module_path,
                self.source_state.revision,
                ContentHash::of(source_bytes),
            )
            .expect("source store preserves ModuleId to ModulePath identity");
    }

    pub(crate) fn record_removed_source(&mut self, module_id: ModuleId, module_path: ModulePath) {
        self.semantic_state
            .module_states
            .source_removed(module_id, module_path)
            .expect("source store preserves ModuleId to ModulePath identity");
    }

    pub fn frontend_snapshot(&self) -> Option<&FrontendSnapshot> {
        self.frontend_snapshot.as_ref()
    }

    pub fn check_state(&self) -> &CheckState {
        &self.semantic_state.check_state
    }

    /// Returns the frozen interface catalog only while it matches a successful check.
    pub fn module_catalog(&self) -> Option<&crate::state::WorkspaceModuleCatalog> {
        match self.semantic_state.check_state {
            CheckState::Passed { revision, .. }
                if self
                    .semantic_state
                    .module_catalog
                    .as_ref()
                    .is_some_and(|catalog| catalog.source_revision() == revision) =>
            {
                self.semantic_state.module_catalog.as_deref()
            }
            _ => None,
        }
    }

    pub fn source_file(&self, path: &ModulePath) -> Option<SourceFile> {
        let entry = self.source_state.store.get(path)?;
        let text = String::from_utf8_lossy(&entry.bytes).to_string();
        Some(SourceFile::new(
            entry.source_id,
            entry.path.as_str().to_string(),
            text,
        ))
    }
}
