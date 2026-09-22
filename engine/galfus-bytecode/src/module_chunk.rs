#[cfg(test)]
mod tests;

use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath};

use crate::{
    BytecodeFormatError, BytecodeFormatVersion, BytecodeNode, BytecodeValidationError,
    ModuleDescriptor, validate_bytecode_format, validate_bytecode_module,
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
