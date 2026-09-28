#[cfg(test)]
mod tests;

use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath, Revision, SemanticRevision};
use std::collections::HashMap;

/// The most advanced artifact currently available for one module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleLifecycle {
    Indexed,
    SourceLoaded,
    InterfaceValid,
    InterfaceInvalid,
    BytecodeCached,
    BytecodeProduced,
}

/// Immutable identity plus the latest incremental artifacts for one module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleStateRecord {
    module_id: ModuleId,
    module_path: ModulePath,
    lifecycle: ModuleLifecycle,
    source_revision: Option<Revision>,
    semantic_revision: Option<SemanticRevision>,
    source_hash: Option<ContentHash>,
    interface_hash: Option<ContentHash>,
    bytecode_hash: Option<ContentHash>,
}

impl ModuleStateRecord {
    fn indexed(module_id: ModuleId, module_path: ModulePath) -> Self {
        Self {
            module_id,
            module_path,
            lifecycle: ModuleLifecycle::Indexed,
            source_revision: None,
            semantic_revision: None,
            source_hash: None,
            interface_hash: None,
            bytecode_hash: None,
        }
    }

    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub fn module_path(&self) -> &ModulePath {
        &self.module_path
    }

    pub const fn lifecycle(&self) -> ModuleLifecycle {
        self.lifecycle
    }

    pub const fn source_revision(&self) -> Option<Revision> {
        self.source_revision
    }

    pub const fn semantic_revision(&self) -> Option<SemanticRevision> {
        self.semantic_revision
    }

    pub const fn source_hash(&self) -> Option<ContentHash> {
        self.source_hash
    }

    pub const fn interface_hash(&self) -> Option<ContentHash> {
        self.interface_hash
    }

    pub const fn bytecode_hash(&self) -> Option<ContentHash> {
        self.bytecode_hash
    }
}

/// Identity conflict while updating the ModuleId-keyed incremental graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleStateError {
    ModulePathMismatch {
        module_id: ModuleId,
        existing: ModulePath,
        attempted: ModulePath,
    },
    SourceRequired {
        module_id: ModuleId,
    },
    ValidInterfaceRequired {
        module_id: ModuleId,
    },
}

/// In-memory, ModuleId-keyed state graph shared by workspace operations.
#[derive(Default)]
pub struct IncrementalModuleStateGraph {
    records: HashMap<ModuleId, ModuleStateRecord>,
}

impl IncrementalModuleStateGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, module_id: ModuleId) -> Option<&ModuleStateRecord> {
        self.records.get(&module_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModuleStateRecord> {
        let mut records = self.records.values().collect::<Vec<_>>();
        records.sort_unstable_by_key(|record| record.module_id().raw());
        records.into_iter()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn index(
        &mut self,
        module_id: ModuleId,
        module_path: ModulePath,
    ) -> Result<&ModuleStateRecord, ModuleStateError> {
        self.ensure_record(module_id, module_path)
            .map(|record| &*record)
    }

    pub fn source_loaded(
        &mut self,
        module_id: ModuleId,
        module_path: ModulePath,
        source_revision: Revision,
        source_hash: ContentHash,
    ) -> Result<&ModuleStateRecord, ModuleStateError> {
        let record = self.ensure_record(module_id, module_path)?;
        record.lifecycle = ModuleLifecycle::SourceLoaded;
        record.source_revision = Some(source_revision);
        record.semantic_revision = None;
        record.source_hash = Some(source_hash);
        record.interface_hash = None;
        record.bytecode_hash = None;
        Ok(record)
    }

    pub fn source_removed(
        &mut self,
        module_id: ModuleId,
        module_path: ModulePath,
    ) -> Result<&ModuleStateRecord, ModuleStateError> {
        let record = self.ensure_record(module_id, module_path)?;
        record.lifecycle = ModuleLifecycle::Indexed;
        record.source_revision = None;
        record.semantic_revision = None;
        record.source_hash = None;
        record.interface_hash = None;
        record.bytecode_hash = None;
        Ok(record)
    }

    pub fn interface_valid(
        &mut self,
        module_id: ModuleId,
        semantic_revision: SemanticRevision,
        interface_hash: ContentHash,
    ) -> Result<&ModuleStateRecord, ModuleStateError> {
        let record = self.source_record_mut(module_id)?;
        if record.semantic_revision != Some(semantic_revision)
            || record.interface_hash != Some(interface_hash)
        {
            record.bytecode_hash = None;
        }
        record.lifecycle = ModuleLifecycle::InterfaceValid;
        record.semantic_revision = Some(semantic_revision);
        record.interface_hash = Some(interface_hash);
        Ok(record)
    }

    pub fn interface_invalid(
        &mut self,
        module_id: ModuleId,
    ) -> Result<&ModuleStateRecord, ModuleStateError> {
        let record = self.source_record_mut(module_id)?;
        record.lifecycle = ModuleLifecycle::InterfaceInvalid;
        record.semantic_revision = None;
        record.interface_hash = None;
        record.bytecode_hash = None;
        Ok(record)
    }

    pub fn bytecode_cached(
        &mut self,
        module_id: ModuleId,
        bytecode_hash: ContentHash,
    ) -> Result<&ModuleStateRecord, ModuleStateError> {
        let record = self.valid_interface_record_mut(module_id)?;
        record.lifecycle = ModuleLifecycle::BytecodeCached;
        record.bytecode_hash = Some(bytecode_hash);
        Ok(record)
    }

    pub fn bytecode_produced(
        &mut self,
        module_id: ModuleId,
        bytecode_hash: ContentHash,
    ) -> Result<&ModuleStateRecord, ModuleStateError> {
        let record = self.valid_interface_record_mut(module_id)?;
        record.lifecycle = ModuleLifecycle::BytecodeProduced;
        record.bytecode_hash = Some(bytecode_hash);
        Ok(record)
    }

    fn ensure_record(
        &mut self,
        module_id: ModuleId,
        module_path: ModulePath,
    ) -> Result<&mut ModuleStateRecord, ModuleStateError> {
        if let Some(record) = self.records.get(&module_id) {
            if record.module_path() != &module_path {
                return Err(ModuleStateError::ModulePathMismatch {
                    module_id,
                    existing: record.module_path().clone(),
                    attempted: module_path,
                });
            }
        }
        Ok(self
            .records
            .entry(module_id)
            .or_insert_with(|| ModuleStateRecord::indexed(module_id, module_path)))
    }

    fn source_record_mut(
        &mut self,
        module_id: ModuleId,
    ) -> Result<&mut ModuleStateRecord, ModuleStateError> {
        let record = self
            .records
            .get_mut(&module_id)
            .ok_or(ModuleStateError::SourceRequired { module_id })?;
        if record.source_hash().is_none() {
            return Err(ModuleStateError::SourceRequired { module_id });
        }
        Ok(record)
    }

    fn valid_interface_record_mut(
        &mut self,
        module_id: ModuleId,
    ) -> Result<&mut ModuleStateRecord, ModuleStateError> {
        let record = self.source_record_mut(module_id)?;
        if record.lifecycle() != ModuleLifecycle::InterfaceValid
            && record.lifecycle() != ModuleLifecycle::BytecodeCached
            && record.lifecycle() != ModuleLifecycle::BytecodeProduced
        {
            return Err(ModuleStateError::ValidInterfaceRequired { module_id });
        }
        Ok(record)
    }
}
