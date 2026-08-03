#[cfg(test)]
mod tests;

use galfus_core::{ModuleId, ModulePath, Revision, SourceId};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleOrigin {
    User,
    Builtin,
    ExternalProxy,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum LoadModuleError {
    Collision(ModulePath),
}

pub struct SourceEntry {
    pub module_id: ModuleId,
    pub source_id: SourceId,
    pub path: ModulePath,
    pub bytes: Arc<[u8]>,
    pub revision: Revision,
    pub origin: ModuleOrigin,
}

pub struct SourceStore {
    entries_by_path: HashMap<ModulePath, SourceEntry>,
}

impl Default for SourceStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceStore {
    pub fn new() -> Self {
        Self {
            entries_by_path: HashMap::new(),
        }
    }

    pub fn load_module(
        &mut self,
        path: ModulePath,
        bytes: Arc<[u8]>,
        origin: ModuleOrigin,
        current_revision: Revision,
    ) -> Result<(ModuleId, SourceId), LoadModuleError> {
        let logical_path = path.as_str();

        let mut hasher = DefaultHasher::new();
        "galfus:module:v1:".hash(&mut hasher);
        logical_path.hash(&mut hasher);
        let hash = hasher.finish();
        let module_id_raw = (hash ^ (hash >> 32)) as u32;
        let module_id_raw = if module_id_raw == 0 { 1 } else { module_id_raw };
        let module_id = ModuleId::new(module_id_raw);

        let mut hasher = DefaultHasher::new();
        "galfus:source:v1:".hash(&mut hasher);
        logical_path.hash(&mut hasher);
        let hash = hasher.finish();
        let source_id_raw = (hash ^ (hash >> 32)) as u32;
        let source_id_raw = if source_id_raw == u32::MAX {
            u32::MAX - 1
        } else {
            source_id_raw
        };
        let source_id = SourceId::new(source_id_raw);

        if let Some(entry) = self.entries_by_path.get_mut(&path) {
            entry.bytes = bytes;
            entry.revision = current_revision;
            entry.origin = origin;
            Ok((entry.module_id, entry.source_id))
        } else {
            // Check for collision by iterating over existing values
            for existing in self.entries_by_path.values() {
                if existing.module_id == module_id || existing.source_id == source_id {
                    return Err(LoadModuleError::Collision(path.clone()));
                }
            }

            self.entries_by_path.insert(
                path.clone(),
                SourceEntry {
                    module_id,
                    source_id,
                    path: path.clone(),
                    bytes,
                    revision: current_revision,
                    origin,
                },
            );

            Ok((module_id, source_id))
        }
    }

    pub fn remove_module(&mut self, path: &ModulePath) -> Option<SourceEntry> {
        self.entries_by_path.remove(path)
    }

    pub fn get(&self, path: &ModulePath) -> Option<&SourceEntry> {
        self.entries_by_path.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SourceEntry> {
        self.entries_by_path.values()
    }
}
