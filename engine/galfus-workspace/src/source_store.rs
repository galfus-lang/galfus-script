#[cfg(test)]
mod tests;

use galfus_core::{ModuleId, ModulePath, Revision, SourceId};
use std::collections::HashMap;
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

    fn fnv1a_32(domain: &[u8], payload: &[u8]) -> u32 {
        let mut hash: u32 = 2166136261;
        for &b in domain {
            hash ^= b as u32;
            hash = hash.wrapping_mul(16777619);
        }
        for &b in payload {
            hash ^= b as u32;
            hash = hash.wrapping_mul(16777619);
        }
        hash
    }

    pub fn load_module(
        &mut self,
        path: ModulePath,
        bytes: Arc<[u8]>,
        origin: ModuleOrigin,
        current_revision: Revision,
    ) -> Result<(ModuleId, SourceId), LoadModuleError> {
        let logical_path = path.as_str();

        let hash = Self::fnv1a_32(b"galfus:module:v1:", logical_path.as_bytes());
        let module_id_raw = if hash == 0 { 1 } else { hash };
        let module_id = ModuleId::new(module_id_raw);

        let hash = Self::fnv1a_32(b"galfus:source:v1:", logical_path.as_bytes());
        let source_id_raw = if hash == u32::MAX {
            u32::MAX - 1
        } else {
            hash
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
