use super::*;

use crate::diagnostic::WorkspaceDiagnosticCode;
use crate::source_store::{LoadModuleError, ModuleOrigin};
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
        if self.catalog.is_provider_module(
            module_path
                .as_str()
                .strip_suffix(".gfs")
                .unwrap_or(module_path.as_str()),
        ) {
            return Err(WorkspaceError::ReservedProviderModule(path.to_string()));
        }
        if !path.ends_with(".gfp")
            && self
                .source_state
                .store
                .get(&module_path)
                .is_some_and(|entry| entry.bytes.as_ref() == module_bytes)
        {
            return Ok(LoadResult::Success);
        }

        let (source_bytes, origin, descriptor) = if path.ends_with(".gfp") {
            let source = match str::from_utf8(module_bytes) {
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
            (Arc::from(module_bytes), ModuleOrigin::User, None)
        };

        self.source_state.revision.next();
        self.source_state
            .store
            .load_module(
                module_path.clone(),
                source_bytes,
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
        self.source_state.revision.next();
        self.source_state
            .store
            .load_module(
                module_path.clone(),
                Arc::from(bridge.source.as_bytes()),
                ModuleOrigin::Builtin,
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
        self.source_state.dirty_sources.insert(module_path);
        self.mark_dirty();
        Ok(LoadResult::Success)
    }

    pub fn remove_module(&mut self, path: &str) -> Result<RemoveResult, WorkspaceError> {
        let module_path = ModulePath::new(path).ok_or(WorkspaceError::InvalidPath)?;

        if let Some(entry) = self.source_state.store.remove_module(&module_path) {
            self.adapter_descriptors.remove(&module_path);
            self.source_state.revision.next();
            self.source_state.dirty_sources.remove(&module_path);
            self.source_state.removed_modules.push(entry.module_id);
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
