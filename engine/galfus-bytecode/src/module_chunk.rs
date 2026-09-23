#[cfg(test)]
mod tests;

use std::collections::HashMap;

use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    BytecodeFormatError, BytecodeFormatVersion, BytecodeNode, BytecodeValidationError,
    ModuleCatalog, ModuleDescriptor, validate_bytecode_format, validate_bytecode_module,
};

/// Immutable, independently verifiable executable payload for one module.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleChunk {
    format_version: BytecodeFormatVersion,
    module_id: ModuleId,
    node: BytecodeNode,
    content_hash: ContentHash,
    interface_hash: ContentHash,
}

/// Immutable ModuleId-indexed collection of canonical module chunks.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleChunkStore {
    chunks: Vec<ModuleChunk>,
    indexes_by_id: HashMap<ModuleId, usize>,
}

impl ModuleChunkStore {
    pub fn new(mut chunks: Vec<ModuleChunk>) -> Result<Self, ModuleChunkStoreError> {
        chunks.sort_unstable_by_key(ModuleChunk::module_id);
        if let Some(module_id) = chunks
            .windows(2)
            .find(|pair| pair[0].module_id() == pair[1].module_id())
            .map(|pair| pair[0].module_id())
        {
            return Err(ModuleChunkStoreError::DuplicateModuleId { module_id });
        }

        let indexes_by_id = chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| (chunk.module_id(), index))
            .collect();
        Ok(Self {
            chunks,
            indexes_by_id,
        })
    }

    pub fn from_nodes(
        format_version: BytecodeFormatVersion,
        nodes: impl IntoIterator<Item = BytecodeNode>,
        catalog: &ModuleCatalog,
    ) -> Result<Self, ModuleChunkStoreBuildError> {
        let mut chunks = Vec::new();
        for node in nodes {
            let module_id = node.id();
            let descriptor = catalog
                .get(module_id)
                .ok_or(ModuleChunkStoreBuildError::MissingDescriptor { module_id })?;
            chunks.push(ModuleChunk::from_node(format_version, node, descriptor)?);
        }
        let store = Self::new(chunks)?;
        store.verify_catalog(catalog, format_version)?;
        Ok(store)
    }

    pub fn get(&self, module_id: ModuleId) -> Option<&ModuleChunk> {
        self.indexes_by_id
            .get(&module_id)
            .map(|index| &self.chunks[*index])
    }

    /// Iterates canonical chunks in ascending ModuleId order.
    pub fn iter(&self) -> impl Iterator<Item = &ModuleChunk> {
        self.chunks.iter()
    }

    pub const fn len(&self) -> usize {
        self.chunks.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub(crate) fn verify_catalog(
        &self,
        catalog: &ModuleCatalog,
        format_version: BytecodeFormatVersion,
    ) -> Result<(), ModuleChunkStoreValidationError> {
        for descriptor in catalog.iter() {
            let chunk = self.get(descriptor.module_id()).ok_or(
                ModuleChunkStoreValidationError::MissingChunk {
                    module_id: descriptor.module_id(),
                },
            )?;
            chunk.verify(descriptor)?;
            if chunk.format_version() != format_version {
                return Err(ModuleChunkStoreValidationError::BytecodeFormatMismatch {
                    module_id: descriptor.module_id(),
                    expected: format_version,
                    actual: chunk.format_version(),
                });
            }
        }
        for chunk in &self.chunks {
            if catalog.get(chunk.module_id()).is_none() {
                return Err(ModuleChunkStoreValidationError::UnexpectedChunk {
                    module_id: chunk.module_id(),
                });
            }
        }
        Ok(())
    }
}

impl Serialize for ModuleChunkStore {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.chunks.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModuleChunkStore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let chunks = Vec::<ModuleChunk>::deserialize(deserializer)?;
        Self::new(chunks).map_err(<D::Error as serde::de::Error>::custom)
    }
}

impl ModuleChunk {
    /// Creates a canonical chunk whose bytecode identity matches `descriptor`.
    pub fn from_node(
        format_version: BytecodeFormatVersion,
        mut node: BytecodeNode,
        descriptor: &ModuleDescriptor,
    ) -> Result<Self, ModuleChunkValidationError> {
        node.semantic_revision = Default::default();
        node.metadata = None;
        let chunk = Self {
            format_version,
            module_id: node.id(),
            content_hash: canonical_node_content_hash(&node)?,
            interface_hash: descriptor.interface_hash(),
            node,
        };
        chunk.verify(descriptor)?;
        Ok(chunk)
    }

    pub const fn format_version(&self) -> BytecodeFormatVersion {
        self.format_version
    }

    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub fn node(&self) -> &BytecodeNode {
        &self.node
    }

    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    pub const fn interface_hash(&self) -> ContentHash {
        self.interface_hash
    }

    /// Encodes the chunk with the stable postcard representation used by package storage.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ModuleChunkEncodingError> {
        self.validate_internal()?;
        postcard::to_stdvec(self).map_err(ModuleChunkEncodingError::from)
    }

    /// Decodes a standalone chunk and validates every property that needs no catalog.
    pub fn from_bytecode(bytes: &[u8]) -> Result<Self, ModuleChunkDecodingError> {
        let (chunk, remaining) = postcard::take_from_bytes::<Self>(bytes)?;
        if !remaining.is_empty() {
            return Err(ModuleChunkDecodingError::UnexpectedTrailingBytes);
        }
        chunk.validate_internal()?;
        Ok(chunk)
    }

    /// Verifies this chunk against exactly one descriptor from the module catalog.
    pub fn verify(&self, descriptor: &ModuleDescriptor) -> Result<(), ModuleChunkValidationError> {
        self.validate_internal()?;

        if self.module_id != descriptor.module_id() {
            return Err(ModuleChunkValidationError::DescriptorModuleIdMismatch {
                expected: descriptor.module_id(),
                actual: self.module_id,
            });
        }
        if self.node.path() != descriptor.module_path() {
            return Err(ModuleChunkValidationError::ModulePathMismatch {
                module_id: self.module_id,
                expected: descriptor.module_path().clone(),
                actual: self.node.path().clone(),
            });
        }
        if self.content_hash != descriptor.chunk_hash() {
            return Err(ModuleChunkValidationError::ContentHashMismatch {
                module_id: self.module_id,
                expected: descriptor.chunk_hash(),
                actual: self.content_hash,
            });
        }
        if self.interface_hash != descriptor.interface_hash() {
            return Err(ModuleChunkValidationError::InterfaceHashMismatch {
                module_id: self.module_id,
                expected: descriptor.interface_hash(),
                actual: self.interface_hash,
            });
        }

        Ok(())
    }

    fn validate_internal(&self) -> Result<(), ModuleChunkValidationError> {
        if self.module_id != self.node.id() {
            return Err(ModuleChunkValidationError::NodeModuleIdMismatch {
                owner: self.module_id,
                node: self.node.id(),
            });
        }
        validate_bytecode_format(self.format_version)?;
        validate_bytecode_module(self.node.module()).map_err(|errors| {
            ModuleChunkValidationError::InvalidBytecode {
                module_id: self.module_id,
                errors,
            }
        })?;

        let actual = canonical_node_content_hash(&self.node)?;
        if actual != self.content_hash {
            return Err(ModuleChunkValidationError::ContentHashMismatch {
                module_id: self.module_id,
                expected: self.content_hash,
                actual,
            });
        }
        Ok(())
    }
}

pub(crate) fn canonical_node_content_hash(
    node: &BytecodeNode,
) -> Result<ContentHash, postcard::Error> {
    postcard::to_stdvec(node).map(|bytes| ContentHash::of(bytes.as_slice()))
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleChunkValidationError {
    #[error("chunk owner module ID {owner:?} does not match node module ID {node:?}")]
    NodeModuleIdMismatch { owner: ModuleId, node: ModuleId },
    #[error("chunk module ID {actual:?} does not match descriptor module ID {expected:?}")]
    DescriptorModuleIdMismatch {
        expected: ModuleId,
        actual: ModuleId,
    },
    #[error(
        "chunk module {module_id:?} path `{actual}` does not match descriptor path `{expected}`"
    )]
    ModulePathMismatch {
        module_id: ModuleId,
        expected: ModulePath,
        actual: ModulePath,
    },
    #[error(transparent)]
    BytecodeFormat(#[from] BytecodeFormatError),
    #[error("chunk module {module_id:?} contains invalid bytecode: {errors:?}")]
    InvalidBytecode {
        module_id: ModuleId,
        errors: Vec<BytecodeValidationError>,
    },
    #[error(
        "chunk module {module_id:?} content hash does not match: expected {expected}, got {actual}"
    )]
    ContentHashMismatch {
        module_id: ModuleId,
        expected: ContentHash,
        actual: ContentHash,
    },
    #[error(
        "chunk module {module_id:?} interface hash does not match: expected {expected}, got {actual}"
    )]
    InterfaceHashMismatch {
        module_id: ModuleId,
        expected: ContentHash,
        actual: ContentHash,
    },
    #[error("could not encode canonical module bytecode: {0}")]
    Postcard(#[from] postcard::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleChunkEncodingError {
    #[error(transparent)]
    Validation(#[from] ModuleChunkValidationError),
    #[error("could not encode the module chunk: {0}")]
    Postcard(#[from] postcard::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleChunkDecodingError {
    #[error("could not decode the module chunk: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("module chunk has trailing bytes")]
    UnexpectedTrailingBytes,
    #[error(transparent)]
    Validation(#[from] ModuleChunkValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModuleChunkStoreError {
    #[error("module chunk store contains module ID {module_id:?} more than once")]
    DuplicateModuleId { module_id: ModuleId },
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleChunkStoreBuildError {
    #[error("module {module_id:?} has no descriptor in the module catalog")]
    MissingDescriptor { module_id: ModuleId },
    #[error(transparent)]
    Chunk(#[from] ModuleChunkValidationError),
    #[error(transparent)]
    Store(#[from] ModuleChunkStoreError),
    #[error(transparent)]
    Validation(#[from] ModuleChunkStoreValidationError),
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleChunkStoreValidationError {
    #[error("module catalog declares {module_id:?}, but the chunk store does not")]
    MissingChunk { module_id: ModuleId },
    #[error("chunk store declares {module_id:?}, but the module catalog does not")]
    UnexpectedChunk { module_id: ModuleId },
    #[error("chunk module {module_id:?} has bytecode format {actual:?}, expected {expected:?}")]
    BytecodeFormatMismatch {
        module_id: ModuleId,
        expected: BytecodeFormatVersion,
        actual: BytecodeFormatVersion,
    },
    #[error(transparent)]
    Chunk(#[from] ModuleChunkValidationError),
}
