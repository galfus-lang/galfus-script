#[cfg(test)]
mod tests;

use galfus_core::ModulePath;
use galfus_workspace::{
    LoadedWorkspaceSource, ModuleOrigin, SourceLoadError, WorkspaceSourceLoader,
};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Filesystem-backed source loader constrained to one canonical workspace root.
pub(super) struct FilesystemSourceLoader {
    root: PathBuf,
}

impl FilesystemSourceLoader {
    pub(super) fn new(root: &Path) -> io::Result<Self> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workspace source root must be a directory",
            ));
        }
        Ok(Self { root })
    }

    fn source_path(&self, module_path: &ModulePath) -> Result<PathBuf, SourceLoadError> {
        if !module_path.as_str().ends_with(".gfs") {
            return Err(SourceLoadError::backend(
                module_path.clone(),
                "filesystem source loader supports only .gfs modules",
            ));
        }
        let candidate = self.root.join(module_path.as_str());
        let candidate = candidate.canonicalize().map_err(|error| {
            source_load_error(module_path.clone(), error, "canonicalize source module")
        })?;
        if !candidate.starts_with(&self.root) {
            return Err(SourceLoadError::path_escapes_workspace(module_path.clone()));
        }
        Ok(candidate)
    }
}

impl WorkspaceSourceLoader for FilesystemSourceLoader {
    fn load_source(
        &self,
        module_path: &ModulePath,
    ) -> Result<LoadedWorkspaceSource, SourceLoadError> {
        let source_path = self.source_path(module_path)?;
        let bytes = fs::read(source_path)
            .map_err(|error| source_load_error(module_path.clone(), error, "read source module"))?;
        Ok(LoadedWorkspaceSource::new(
            Arc::from(bytes),
            ModuleOrigin::User,
        ))
    }
}

fn source_load_error(
    module_path: ModulePath,
    error: io::Error,
    operation: &str,
) -> SourceLoadError {
    if error.kind() == io::ErrorKind::NotFound {
        SourceLoadError::not_found(module_path)
    } else {
        SourceLoadError::backend(module_path, format!("{operation}: {error}"))
    }
}
