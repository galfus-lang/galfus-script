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
    Collision {
        attempted: ModulePath,
        existing: ModulePath,
        identity: IdentityKind,
        id: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentityKind {
    Module,
    Source,
}

impl IdentityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Module => "ModuleId",
            Self::Source => "SourceId",
        }
    }
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

    fn non_reserved_id(mut hash: u32, domain: &[u8], payload: &[u8], reserved: u32) -> u32 {
        let mut retry = 1u32;
        while hash == reserved {
            let mut retry_domain = Vec::with_capacity(domain.len() + 11);
            retry_domain.extend_from_slice(domain);
            retry_domain.extend_from_slice(b"rehash:");
            retry_domain.extend_from_slice(&retry.to_le_bytes());
            hash = Self::fnv1a_32(&retry_domain, payload);
            retry = retry.wrapping_add(1);
        }
        hash
    }

    fn module_id_for(logical_path: &str) -> ModuleId {
        let hash = Self::fnv1a_32(b"galfus:module:v1:", logical_path.as_bytes());
        ModuleId::new(Self::non_reserved_id(
            hash,
            b"galfus:module:v1:",
            logical_path.as_bytes(),
            0,
        ))
    }

    fn source_id_for(logical_path: &str) -> SourceId {
        let hash = Self::fnv1a_32(b"galfus:source:v1:", logical_path.as_bytes());
        SourceId::new(Self::non_reserved_id(
            hash,
            b"galfus:source:v1:",
            logical_path.as_bytes(),
            u32::MAX,
        ))
    }

    pub fn load_module(
        &mut self,
        path: ModulePath,
        bytes: Arc<[u8]>,
        origin: ModuleOrigin,
        current_revision: Revision,
    ) -> Result<(ModuleId, SourceId), LoadModuleError> {
        let logical_path = path.as_str();
        let module_id = Self::module_id_for(logical_path);
        let source_id = Self::source_id_for(logical_path);

        if let Some(entry) = self.entries_by_path.get_mut(&path) {
            entry.bytes = bytes;
            entry.revision = current_revision;
            entry.origin = origin;
            Ok((entry.module_id, entry.source_id))
        } else {
            let collision = self
                .entries_by_path
                .values()
                .filter_map(|existing| {
                    if existing.module_id == module_id {
                        Some((IdentityKind::Module, module_id.raw(), existing))
                    } else if existing.source_id == source_id {
                        Some((IdentityKind::Source, source_id.raw(), existing))
                    } else {
                        None
                    }
                })
                .min_by_key(|(identity, _, existing)| (existing.path.as_str(), *identity));

            if let Some((identity, id, existing)) = collision {
                return Err(LoadModuleError::Collision {
                    attempted: path.clone(),
                    existing: existing.path.clone(),
                    identity,
                    id,
                });
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
        let mut entries: Vec<_> = self.entries_by_path.values().collect();
        entries.sort_by_key(|e| e.module_id.raw());
        entries.into_iter()
    }
}
