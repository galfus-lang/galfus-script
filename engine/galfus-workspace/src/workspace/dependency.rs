use super::*;

use crate::source_store::{LoadModuleError, ModuleOrigin};
use crate::state::*;
use galfus_core::ModulePath;
use std::sync::Arc;

impl Workspace {
    pub(crate) fn load_required_dependencies(
        &mut self,
        paths: &[ModulePath],
    ) -> Result<bool, WorkspaceError> {
        let mut loaded = false;
        for path in paths {
            if self.source_state.store.get(path).is_some() {
                continue;
            }
            let builtin_name = path.as_str().strip_suffix(".gfs").unwrap_or(path.as_str());
            let (source, origin) = if let Some((_, source)) = galfus_contract::BUILTIN_MODULES
                .iter()
                .find(|(name, _)| *name == builtin_name)
            {
                (*source, ModuleOrigin::Builtin)
            } else if let Some(source) = self.catalog.provider_source(builtin_name) {
                (source, ModuleOrigin::ProviderCatalog)
            } else {
                continue;
            };
            self.source_state.revision.next();
            self.source_state
                .store
                .load_module(
                    path.clone(),
                    Arc::from(source.as_bytes()),
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
            self.source_state.dirty_sources.insert(path.clone());
            loaded = true;
        }
        Ok(loaded)
    }
}
