#[cfg(test)]
mod tests;

use crate::source_store::ModuleOrigin;
use galfus_core::{ModuleId, ModulePath};
use std::sync::Arc;

/// Source bytes and provenance returned by a workspace source backend.
#[derive(Clone, Debug)]
pub struct LoadedWorkspaceSource {
    bytes: Arc<[u8]>,
    origin: ModuleOrigin,
}

impl LoadedWorkspaceSource {
    pub fn new(bytes: Arc<[u8]>, origin: ModuleOrigin) -> Self {
        Self { bytes, origin }
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }

    pub const fn origin(&self) -> ModuleOrigin {
        self.origin
    }

    pub(crate) fn into_parts(self) -> (Arc<[u8]>, ModuleOrigin) {
        (self.bytes, self.origin)
    }
}

/// Category for a failed source backend request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceLoadErrorKind {
    NotFound,
    PathEscapesWorkspace,
    Backend(String),
    LoaderUnavailable,
}

/// Structured failure from a source backend request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLoadError {
    requested_path: ModulePath,
    module_id: Option<ModuleId>,
    kind: SourceLoadErrorKind,
}

impl SourceLoadError {
    pub fn not_found(requested_path: ModulePath) -> Self {
        Self {
            requested_path,
            module_id: None,
            kind: SourceLoadErrorKind::NotFound,
        }
    }

    pub fn backend(requested_path: ModulePath, message: impl Into<String>) -> Self {
        Self {
            requested_path,
            module_id: None,
            kind: SourceLoadErrorKind::Backend(message.into()),
        }
    }

    pub fn path_escapes_workspace(requested_path: ModulePath) -> Self {
        Self {
            requested_path,
            module_id: None,
            kind: SourceLoadErrorKind::PathEscapesWorkspace,
        }
    }

    pub(crate) fn loader_unavailable(requested_path: ModulePath, module_id: ModuleId) -> Self {
        Self {
            requested_path,
            module_id: Some(module_id),
            kind: SourceLoadErrorKind::LoaderUnavailable,
        }
    }

    pub const fn module_id(&self) -> Option<ModuleId> {
        self.module_id
    }

    pub fn requested_path(&self) -> &ModulePath {
        &self.requested_path
    }

    pub const fn kind(&self) -> &SourceLoadErrorKind {
        &self.kind
    }

    pub(crate) fn for_request(mut self, requested_path: ModulePath, module_id: ModuleId) -> Self {
        self.requested_path = requested_path;
        if self.module_id.is_none() {
            self.module_id = Some(module_id);
        }
        self
    }
}

/// Filesystem-independent source backend used by lazy workspace loading.
pub trait WorkspaceSourceLoader: Send + Sync {
    fn load_source(
        &self,
        module_path: &ModulePath,
    ) -> Result<LoadedWorkspaceSource, SourceLoadError>;
}

/// Result of a workspace source request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRequestResult {
    AlreadyLoaded { module_id: ModuleId },
    Loaded { module_id: ModuleId },
}

impl SourceRequestResult {
    pub const fn module_id(self) -> ModuleId {
        match self {
            Self::AlreadyLoaded { module_id } | Self::Loaded { module_id } => module_id,
        }
    }
}
