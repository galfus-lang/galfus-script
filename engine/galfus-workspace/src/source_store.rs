#[cfg(test)]
mod tests;

use galfus_core::{ModuleId, ModulePath, Revision, SourceId};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleOrigin {
    User,
    Builtin,
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
    next_module_id: u32,
    next_source_id: u32,
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
            next_module_id: 0,
            // Reserve WORKSPACE_SOURCE_ID (which is u32::MAX)
            next_source_id: 0,
        }
    }

    pub fn load_module(
        &mut self,
        path: ModulePath,
        bytes: Arc<[u8]>,
        origin: ModuleOrigin,
        current_revision: Revision,
    ) -> Option<(ModuleId, SourceId)> {
        if let Some(entry) = self.entries_by_path.get_mut(&path) {
            // Already exists, update contents and revision
            entry.bytes = bytes;
            entry.revision = current_revision;
            entry.origin = origin;
            Some((entry.module_id, entry.source_id))
        } else {
            // New module
            let module_id = ModuleId::new(self.next_module_id);
            self.next_module_id += 1;

            let source_id = SourceId::new(self.next_source_id);
            self.next_source_id += 1;

            self.entries_by_path.insert(
                path.clone(),
                SourceEntry {
                    module_id,
                    source_id,
                    path,
                    bytes,
                    revision: current_revision,
                    origin,
                },
            );

            Some((module_id, source_id))
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
