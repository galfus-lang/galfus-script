#[cfg(test)]
mod tests;

use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath, Revision};
use std::fmt;

/// Stable diagnostic context for one module materialization attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleResolveContext {
    module_id: ModuleId,
    module_path: Option<ModulePath>,
}

impl ModuleResolveContext {
    pub const fn new(module_id: ModuleId) -> Self {
        Self {
            module_id,
            module_path: None,
        }
    }

    pub fn with_path(module_id: ModuleId, module_path: ModulePath) -> Self {
        Self {
            module_id,
            module_path: Some(module_path),
        }
    }

    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub fn module_path(&self) -> Option<&ModulePath> {
        self.module_path.as_ref()
    }
}

impl fmt::Display for ModuleResolveContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.module_path {
            Some(module_path) => write!(formatter, "module {:?} ({module_path})", self.module_id),
            None => write!(formatter, "module {:?}", self.module_id),
        }
    }
}

/// Deterministic failures raised while materializing one module.
///
/// Every variant retains the requested ModuleId through its
/// ModuleResolveContext. Callers must attach only stable package metadata to
/// this error; OS, pointer, and hash-map iteration details do not belong in
/// resolver diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModuleResolveError {
    #[error("{context} is not declared in the module catalog")]
    UnknownModule { context: ModuleResolveContext },
    #[error("{context} is unavailable for eager materialization")]
    UnavailableInEager { context: ModuleResolveContext },
    #[error("{context} has an invalid module chunk")]
    ChunkDecode { context: ModuleResolveContext },
    #[error("{context} has a content hash mismatch: expected {expected}, actual {actual}")]
    ChunkHashMismatch {
        context: ModuleResolveContext,
        expected: ContentHash,
        actual: ContentHash,
    },
    #[error("{context} has an interface hash mismatch: expected {expected}, actual {actual}")]
    InterfaceMismatch {
        context: ModuleResolveContext,
        expected: ContentHash,
        actual: ContentHash,
    },
    #[error("{context} belongs to source revision {actual:?}, not frozen revision {expected:?}")]
    SourceSnapshotChanged {
        context: ModuleResolveContext,
        expected: Revision,
        actual: Revision,
    },
    #[error("{context} is unavailable from the frozen source snapshot")]
    SourceModuleUnavailable { context: ModuleResolveContext },
    #[error("{context} does not match its frozen source interface")]
    SourceInterfaceMismatch { context: ModuleResolveContext },
    #[error("{context} requires unavailable provider module `{module_path}`")]
    ProviderRequirementUnsatisfied {
        context: ModuleResolveContext,
        module_path: String,
    },
    #[error("{context} requires unavailable adapter proxy `{proxy_module}`")]
    AdapterRequirementUnsatisfied {
        context: ModuleResolveContext,
        proxy_module: String,
    },
    #[error("dependency cycle detected while resolving {context}")]
    DependencyCycle { context: ModuleResolveContext },
    #[error("{context} could not be produced")]
    ProducerFailed { context: ModuleResolveContext },
}

impl ModuleResolveError {
    pub const fn context(&self) -> &ModuleResolveContext {
        match self {
            Self::UnknownModule { context }
            | Self::UnavailableInEager { context }
            | Self::ChunkDecode { context }
            | Self::ChunkHashMismatch { context, .. }
            | Self::InterfaceMismatch { context, .. }
            | Self::SourceSnapshotChanged { context, .. }
            | Self::SourceModuleUnavailable { context }
            | Self::SourceInterfaceMismatch { context }
            | Self::ProviderRequirementUnsatisfied { context, .. }
            | Self::AdapterRequirementUnsatisfied { context, .. }
            | Self::DependencyCycle { context }
            | Self::ProducerFailed { context } => context,
        }
    }

    pub const fn module_id(&self) -> ModuleId {
        self.context().module_id()
    }

    pub fn module_path(&self) -> Option<&ModulePath> {
        self.context().module_path()
    }
}
