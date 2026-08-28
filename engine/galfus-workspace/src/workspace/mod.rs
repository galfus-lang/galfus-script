pub mod compilation;
pub mod dependency;
pub mod execution;
pub mod module;
mod optimizer;

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
use galfus_contract::{
    AdapterFunctionSignature, AdapterModuleDescriptor, ExecutionTarget, Providers,
    RuntimeCapabilities,
};
use galfus_core::{DiagnosticBag, ModulePath, OpaqueTypeId, SourceFile};
use galfus_frontend::modules::{
    FrontendModuleKind, FrontendSession, FrontendSnapshot, FrontendSource, FrontendUpdate,
    SemanticRoot, SemanticRootKind,
};
use galfus_frontend::{ResolutionLayer, StringTable, TypeTable};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Workspace {
    pub root_path: Option<std::path::PathBuf>,
    pub config: Option<WorkspaceConfig>,
    pub source_state: SourceState,
    pub semantic_state: SemanticState,
    pub bytecode_state: BytecodeState,
    pub frontend: FrontendSession,
    frontend_snapshot: Option<FrontendSnapshot>,
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
            catalog: Arc::new(galfus_contract::CapabilityCatalog::default()),
            adapter_descriptors: HashMap::new(),
        }
    }

    pub fn set_catalog(&mut self, catalog: Arc<galfus_contract::CapabilityCatalog>) {
        if self.catalog.fingerprint() != catalog.fingerprint() {
            self.source_state.revision.next();
            let removed = self
                .source_state
                .store
                .remove_by_origin(ModuleOrigin::ProviderCatalog);
            for entry in removed {
                self.source_state.dirty_sources.remove(&entry.path);
                self.source_state.removed_modules.push(entry.module_id);
            }
            self.catalog = catalog;
            self.mark_dirty();
        }
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

    pub fn frontend_snapshot(&self) -> Option<&FrontendSnapshot> {
        self.frontend_snapshot.as_ref()
    }

    pub fn check_state(&self) -> &CheckState {
        &self.semantic_state.check_state
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
