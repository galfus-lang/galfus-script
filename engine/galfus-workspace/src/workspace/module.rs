use super::*;

use crate::diagnostic::WorkspaceDiagnosticCode;
use crate::source_store::{LoadModuleError, ModuleOrigin, SourceStore};
use crate::state::*;
use galfus_contract::AdapterModuleDescriptor;
use galfus_core::ModulePath;
use galfus_core::{Diagnostic, DiagnosticBag, Span};
use std::sync::Arc;

impl Workspace {
    pub fn load_module(
        &mut self,
        path: &str,
        module_bytes: &[u8],
    ) -> Result<LoadResult, WorkspaceError> {
        let module_path = ModulePath::new(path).ok_or(WorkspaceError::InvalidPath)?;
        self.load_module_source(module_path, Arc::from(module_bytes), ModuleOrigin::User)
    }

    pub fn request_source(
        &mut self,
        module_path: &ModulePath,
    ) -> Result<SourceRequestResult, WorkspaceError> {
        if let Some(entry) = self.source_state.store.get(module_path) {
            return Ok(SourceRequestResult::AlreadyLoaded {
                module_id: entry.module_id,
            });
        }

        let module_id = SourceStore::module_id_for_path(module_path);
        let source_loader = self.source_loader.as_ref().ok_or_else(|| {
            WorkspaceError::SourceLoad(SourceLoadError::loader_unavailable(
                module_path.clone(),
                module_id,
            ))
        })?;
        let source = source_loader.load_source(module_path).map_err(|error| {
            WorkspaceError::SourceLoad(error.for_request(module_path.clone(), module_id))
        })?;
        let (source_bytes, origin) = source.into_parts();
        match self.load_module_source(module_path.clone(), source_bytes, origin)? {
            LoadResult::Success => Ok(SourceRequestResult::Loaded { module_id }),
            LoadResult::Diagnostics(_) => Err(WorkspaceError::SourceLoad(
                SourceLoadError::backend(module_path.clone(), "invalid adapter proxy")
                    .for_request(module_path.clone(), module_id),
            )),
        }
    }

    pub(crate) fn load_module_source(
        &mut self,
        module_path: ModulePath,
        source_bytes: Arc<[u8]>,
        requested_origin: ModuleOrigin,
    ) -> Result<LoadResult, WorkspaceError> {
        if requested_origin == ModuleOrigin::User
            && self.catalog.is_provider_module(
                module_path
                    .as_str()
                    .strip_suffix(".gfs")
                    .unwrap_or(module_path.as_str()),
            )
        {
            return Err(WorkspaceError::ReservedProviderModule(
                module_path.as_str().to_string(),
            ));
        }
        if !module_path.as_str().ends_with(".gfp")
            && self
                .source_state
                .store
                .get(&module_path)
                .is_some_and(|entry| entry.bytes.as_ref() == source_bytes.as_ref())
        {
            return Ok(LoadResult::Success);
        }

        let (source_bytes, origin, descriptor) = if module_path.as_str().ends_with(".gfp") {
            let source = match str::from_utf8(&source_bytes) {
                Ok(source) => source,
                Err(_) => return Ok(Self::invalid_adapter_proxy(".gfp source must be UTF-8")),
            };
            let (frontmatter, body) = match parse_gfp_frontmatter(source) {
                Ok(parsed) => parsed,
                Err(error) => return Ok(Self::invalid_adapter_proxy(error)),
            };
            (
                Arc::from(body.as_bytes()),
                ModuleOrigin::AdapterProxy,
                Some(AdapterModuleDescriptor {
                    adapter: frontmatter.adapter,
                    config: frontmatter.config,
                    targets: frontmatter.targets,
                    exports: Vec::new(),
                }),
            )
        } else {
            (source_bytes, requested_origin, None)
        };

        self.source_state.advance_revision();
        let (module_id, _) = self
            .source_state
            .store
            .load_module(
                module_path.clone(),
                Arc::clone(&source_bytes),
                origin,
                self.source_state.revision,
            )
            .map_err(|err| match err {
                LoadModuleError::Collision {
                    attempted,
                    existing,
                    identity,
                    id,
                } => WorkspaceError::Collision {
                    attempted: attempted.as_str().to_string(),
                    existing: existing.as_str().to_string(),
                    identity,
                    id,
                },
            })?;
        self.source_state.track_loaded_module(module_id);
        self.record_loaded_source(module_id, module_path.clone(), source_bytes.as_ref());
        if let Some(descriptor) = descriptor {
            self.adapter_descriptors
                .insert(module_path.clone(), descriptor);
        } else {
            self.adapter_descriptors.remove(&module_path);
        }
        self.source_state.dirty_sources.insert(module_path);
        self.mark_dirty();
        Ok(LoadResult::Success)
    }

    pub fn register_bridge_module(
        &mut self,
        bridge: galfus_contract::BridgeModule,
    ) -> Result<LoadResult, WorkspaceError> {
        let module_path = ModulePath::new(&bridge.name).ok_or(WorkspaceError::InvalidPath)?;
        self.load_module_source(
            module_path,
            Arc::from(bridge.source.as_bytes()),
            ModuleOrigin::Builtin,
        )
    }

    pub fn remove_module(&mut self, path: &str) -> Result<RemoveResult, WorkspaceError> {
        let module_path = ModulePath::new(path).ok_or(WorkspaceError::InvalidPath)?;

        if let Some(entry) = self.source_state.store.remove_module(&module_path) {
            self.adapter_descriptors.remove(&module_path);
            self.source_state.advance_revision();
            self.source_state.dirty_sources.remove(&module_path);
            self.source_state.removed_modules.push(entry.module_id);
            self.source_state.track_removed_module(entry.module_id);
            self.record_removed_source(entry.module_id, entry.path);
            self.mark_dirty();
            Ok(RemoveResult::Success)
        } else {
            Ok(RemoveResult::NotFound)
        }
    }

    pub(crate) fn invalid_adapter_proxy(message: impl Into<String>) -> LoadResult {
        let mut diagnostics = DiagnosticBag::new();
        diagnostics.push(Diagnostic::error_with_message(
            WorkspaceDiagnosticCode::InvalidAdapterProxy,
            message,
            Span::empty(WORKSPACE_SOURCE_ID, 0),
        ));
        LoadResult::Diagnostics(diagnostics)
    }
}
