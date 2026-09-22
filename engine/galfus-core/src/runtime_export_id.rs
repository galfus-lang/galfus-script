#[cfg(test)]
mod tests;

use crate::ModuleId;
use std::collections::BTreeMap;
use std::fmt;

/// The stable kind of a symbol exported across a module boundary.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum RuntimeExportKind {
    Function,
    Global,
}

impl RuntimeExportKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Function => 1,
            Self::Global => 2,
        }
    }
}

/// Opaque, stable identity of an exported runtime symbol.
///
/// This ID is derived from its owning module, export kind, and export name. It
/// is not a local function or global slot and must not be constructed from an
/// integer by consumers.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct RuntimeExportId(u64);

impl RuntimeExportId {
    pub fn new(module_id: ModuleId, kind: RuntimeExportKind, export_name: &str) -> Self {
        let mut hash = FNV1A_64_OFFSET_BASIS;
        fnv1a_64_extend(&mut hash, b"galfus:runtime-export:v1:");
        fnv1a_64_extend(&mut hash, module_id.raw().to_le_bytes().as_slice());
        fnv1a_64_extend(&mut hash, [kind.tag()].as_slice());
        fnv1a_64_extend(
            &mut hash,
            (export_name.len() as u64).to_le_bytes().as_slice(),
        );
        fnv1a_64_extend(&mut hash, export_name.as_bytes());
        Self(hash)
    }
}

/// The source identity that must own one RuntimeExportId.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeExportIdentity {
    module_id: ModuleId,
    kind: RuntimeExportKind,
    export_name: String,
}

impl RuntimeExportIdentity {
    pub fn new(
        module_id: ModuleId,
        kind: RuntimeExportKind,
        export_name: impl Into<String>,
    ) -> Self {
        Self {
            module_id,
            kind,
            export_name: export_name.into(),
        }
    }

    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub const fn kind(&self) -> RuntimeExportKind {
        self.kind
    }

    pub fn export_name(&self) -> &str {
        self.export_name.as_str()
    }

    pub fn runtime_export_id(&self) -> RuntimeExportId {
        RuntimeExportId::new(self.module_id, self.kind, self.export_name.as_str())
    }
}

impl fmt::Display for RuntimeExportIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "module {:?} {:?} {}",
            self.module_id, self.kind, self.export_name
        )
    }
}

/// A detected collision between two distinct runtime export identities.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("runtime export ID {id:?} is assigned to both {first} and {second}")]
pub struct RuntimeExportIdCollision {
    id: RuntimeExportId,
    first: RuntimeExportIdentity,
    second: RuntimeExportIdentity,
}

impl RuntimeExportIdCollision {
    pub const fn id(&self) -> RuntimeExportId {
        self.id
    }

    pub const fn first(&self) -> &RuntimeExportIdentity {
        &self.first
    }

    pub const fn second(&self) -> &RuntimeExportIdentity {
        &self.second
    }
}

/// Deterministic collision detector for package export construction.
#[derive(Default)]
pub struct RuntimeExportIdRegistry {
    identities: BTreeMap<RuntimeExportId, RuntimeExportIdentity>,
}

impl RuntimeExportIdRegistry {
    pub fn register(
        &mut self,
        identity: RuntimeExportIdentity,
    ) -> Result<RuntimeExportId, RuntimeExportIdCollision> {
        let id = identity.runtime_export_id();
        self.register_id(id, identity)
    }

    fn register_id(
        &mut self,
        id: RuntimeExportId,
        identity: RuntimeExportIdentity,
    ) -> Result<RuntimeExportId, RuntimeExportIdCollision> {
        match self.identities.get(&id) {
            Some(existing) if existing != &identity => Err(RuntimeExportIdCollision {
                id,
                first: existing.clone(),
                second: identity,
            }),
            Some(_) => Ok(id),
            None => {
                self.identities.insert(id, identity);
                Ok(id)
            }
        }
    }
}

const FNV1A_64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A_64_PRIME: u64 = 0x00000100000001b3;

fn fnv1a_64_extend(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= *byte as u64;
        *hash = hash.wrapping_mul(FNV1A_64_PRIME);
    }
}
