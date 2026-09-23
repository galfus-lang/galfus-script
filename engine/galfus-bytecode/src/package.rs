#[cfg(test)]
mod tests;

use galfus_contract::{
    AdapterModuleRequirement, BoundaryAbiVersion, CURRENT_BOUNDARY_ABI_VERSION,
    CURRENT_NUMERIC_SEMANTICS_VERSION, CURRENT_PRODUCER_VERSION, ContentHash, ExecutionTarget,
    LimitsMetadata, NumericSemanticsVersion, ProducerVersion, ProviderModuleRequirement,
};
use galfus_core::{ModuleId, ModulePath};
use std::collections::BTreeSet;

use crate::{
    BytecodeFormatError, BytecodeFormatVersion, BytecodeGraph, BytecodeGraphValidationErrors,
    CURRENT_PACKAGE_FORMAT_VERSION, ModuleCatalog, ModuleChunkStore, PackageFormatError,
    PackageFormatVersion, attach_capability_requirements, derive_module_catalog,
};

/// The exported entry point of a package image.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageEntryPoint {
    module_path: ModulePath,
    function_name: String,
}

impl PackageEntryPoint {
    pub fn new(module_path: ModulePath, function_name: impl Into<String>) -> Self {
        Self {
            module_path,
            function_name: function_name.into(),
        }
    }

    pub fn module_path(&self) -> &ModulePath {
        &self.module_path
    }

    pub fn function_name(&self) -> &str {
        self.function_name.as_str()
    }
}

/// Metadata describing the published package.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub email: Option<String>,
    pub description: Option<String>,
}

/// Version contracts recorded with a package image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageVersions {
    producer: ProducerVersion,
    package_format: PackageFormatVersion,
    bytecode_format: BytecodeFormatVersion,
    boundary_abi: BoundaryAbiVersion,
    numeric_semantics: NumericSemanticsVersion,
}

impl PackageVersions {
    pub const fn for_bytecode(bytecode_format: BytecodeFormatVersion) -> Self {
        Self {
            producer: CURRENT_PRODUCER_VERSION,
            package_format: CURRENT_PACKAGE_FORMAT_VERSION,
            bytecode_format,
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            numeric_semantics: CURRENT_NUMERIC_SEMANTICS_VERSION,
        }
    }

    pub const fn producer(self) -> ProducerVersion {
        self.producer
    }

    pub const fn package_format(self) -> PackageFormatVersion {
        self.package_format
    }

    pub const fn bytecode_format(self) -> BytecodeFormatVersion {
        self.bytecode_format
    }

    pub const fn boundary_abi(self) -> BoundaryAbiVersion {
        self.boundary_abi
    }

    pub const fn numeric_semantics(self) -> NumericSemanticsVersion {
        self.numeric_semantics
    }
}

/// Immutable compiled output delivered to a host.
///
/// The catalog, canonical chunks, and declarative external requirements are
/// created together and cannot be replaced independently after publication.
///
/// The field order is the package-format v7 wire contract. Postcard does not
/// encode field names or a self-describing schema, so adding, removing, or
/// reordering serialized fields requires a new package format and a dedicated
/// decoding branch. Never change this layout in place.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PackageImage {
    #[serde(skip, default)]
    graph: Option<std::sync::Arc<BytecodeGraph>>,
    target: ExecutionTarget,
    entry_point: Option<PackageEntryPoint>,
    metadata: PackageMetadata,
    limits: LimitsMetadata,
    adapter_requirements: Vec<AdapterModuleRequirement>,
    provider_requirements: Vec<ProviderModuleRequirement>,
    versions: PackageVersions,
    catalog: ModuleCatalog,
    chunks: ModuleChunkStore,
}

/// Errors that prevent a package image from having an exact adapter manifest.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PackageValidationError {
    #[error("adapter requirement for `{proxy_module}` is duplicated")]
    DuplicateAdapterRequirement { proxy_module: String },
    #[error("adapter proxy `{proxy_module}` is missing from the package manifest")]
    MissingAdapterRequirement { proxy_module: String },
    #[error("adapter requirement `{proxy_module}` does not match a package adapter proxy")]
    UnexpectedAdapterRequirement { proxy_module: String },
    #[error("provider requirement for `{module_path}` is duplicated")]
    DuplicateProviderRequirement { module_path: String },
    #[error("provider alias `{alias}` is duplicated")]
    DuplicateProviderAlias { alias: String },
    #[error(
        "module {module_id:?} refers to provider module {provider_module:?}, absent from the package manifest"
    )]
    UndeclaredProviderModuleRequirement {
        module_id: ModuleId,
        provider_module: ModuleId,
    },
    #[error(
        "module {module_id:?} refers to adapter proxy module {adapter_proxy_module:?}, absent from the package manifest"
    )]
    UndeclaredAdapterProxyRequirement {
        module_id: ModuleId,
        adapter_proxy_module: ModuleId,
    },
    #[error("the supplied module catalog does not exactly describe the package graph")]
    CatalogGraphMismatch,
    #[error("could not derive a module catalog from the package graph: {reason}")]
    CatalogDerivation { reason: String },
    #[error("module chunks do not exactly describe the module catalog: {reason}")]
    ChunkCatalogMismatch { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum PackageEncodingError {
    #[error("could not encode the package image: {0}")]
    Postcard(#[from] postcard::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum PackageDecodingError {
    #[error("could not decode the package image: {0}")]
    Postcard(#[from] postcard::Error),
    #[error(
        "package format {actual} is not supported by this decoder; supported package format is {supported}"
    )]
    UnsupportedPackageFormat {
        supported: PackageFormatVersion,
        actual: PackageFormatVersion,
    },
    #[error(transparent)]
    PackageFormat(PackageFormatError),
    #[error(transparent)]
    BytecodeFormat(BytecodeFormatError),
    #[error(transparent)]
    Graph(#[from] BytecodeGraphValidationErrors),
    #[error(transparent)]
    Validation(#[from] PackageValidationError),
    #[error("package declares bytecode format {declared:?}, but graph contains {actual:?}")]
    BytecodeFormatMismatch {
        declared: BytecodeFormatVersion,
        actual: BytecodeFormatVersion,
    },
}

impl PackageImage {
    pub fn try_new(
        graph: BytecodeGraph,
        catalog: ModuleCatalog,
        target: ExecutionTarget,
        entry_point: Option<PackageEntryPoint>,
        metadata: PackageMetadata,
        limits: LimitsMetadata,
        adapter_requirements: Vec<AdapterModuleRequirement>,
        provider_requirements: Vec<ProviderModuleRequirement>,
    ) -> Result<Self, PackageValidationError> {
        let base_catalog = derive_module_catalog(&graph).map_err(|error| {
            PackageValidationError::CatalogDerivation {
                reason: error.to_string(),
            }
        })?;
        let catalog = if catalog == base_catalog {
            attach_capability_requirements(&catalog, &adapter_requirements, &provider_requirements)
                .map_err(|error| PackageValidationError::CatalogDerivation {
                    reason: error.to_string(),
                })?
        } else {
            catalog
        };
        Self::validate_catalog(
            &graph,
            &catalog,
            &adapter_requirements,
            &provider_requirements,
        )?;
        let chunks = ModuleChunkStore::from_nodes(
            graph.format_version(),
            graph.modules().cloned(),
            &catalog,
        )
        .map_err(|error| PackageValidationError::ChunkCatalogMismatch {
            reason: error.to_string(),
        })?;
        Self::try_new_with_chunks(
            graph,
            catalog,
            chunks,
            target,
            entry_point,
            metadata,
            limits,
            adapter_requirements,
            provider_requirements,
        )
    }

    pub fn try_new_with_chunks(
        graph: BytecodeGraph,
        catalog: ModuleCatalog,
        chunks: ModuleChunkStore,
        target: ExecutionTarget,
        entry_point: Option<PackageEntryPoint>,
        metadata: PackageMetadata,
        limits: LimitsMetadata,
        mut adapter_requirements: Vec<AdapterModuleRequirement>,
        mut provider_requirements: Vec<ProviderModuleRequirement>,
    ) -> Result<Self, PackageValidationError> {
        for requirement in &mut adapter_requirements {
            requirement.descriptor.canonicalize();
        }
        for requirement in &mut provider_requirements {
            requirement.canonicalize();
        }
        adapter_requirements.sort_by(|left, right| left.proxy_module.cmp(&right.proxy_module));
        provider_requirements.sort_by(|left, right| {
            left.module_path
                .cmp(&right.module_path)
                .then_with(|| left.alias.cmp(&right.alias))
        });
        Self::validate_adapter_requirements(&catalog, &adapter_requirements)?;
        Self::validate_provider_requirements(&catalog, &provider_requirements)?;
        Self::validate_catalog(
            &graph,
            &catalog,
            &adapter_requirements,
            &provider_requirements,
        )?;
        Self::validate_chunks(&catalog, &chunks, graph.format_version())?;

        Ok(Self {
            versions: PackageVersions::for_bytecode(graph.format_version()),
            graph: Some(std::sync::Arc::new(graph)),
            target,
            entry_point,
            metadata,
            limits,
            adapter_requirements,
            provider_requirements,
            catalog,
            chunks,
        })
    }

    fn validate_catalog(
        graph: &BytecodeGraph,
        catalog: &ModuleCatalog,
        adapter_requirements: &[AdapterModuleRequirement],
        provider_requirements: &[ProviderModuleRequirement],
    ) -> Result<(), PackageValidationError> {
        let expected = derive_module_catalog(graph).map_err(|error| {
            PackageValidationError::CatalogDerivation {
                reason: error.to_string(),
            }
        })?;
        Self::validate_descriptor_requirements(
            catalog,
            adapter_requirements,
            provider_requirements,
        )?;
        let expected =
            attach_capability_requirements(&expected, adapter_requirements, provider_requirements)
                .map_err(|error| PackageValidationError::CatalogDerivation {
                    reason: error.to_string(),
                })?;
        if catalog != &expected {
            return Err(PackageValidationError::CatalogGraphMismatch);
        }
        Ok(())
    }

    fn validate_chunks(
        catalog: &ModuleCatalog,
        chunks: &ModuleChunkStore,
        format_version: BytecodeFormatVersion,
    ) -> Result<(), PackageValidationError> {
        chunks
            .verify_catalog(catalog, format_version)
            .map_err(|error| PackageValidationError::ChunkCatalogMismatch {
                reason: error.to_string(),
            })
    }

    fn validate_adapter_requirements(
        catalog: &ModuleCatalog,
        adapter_requirements: &[AdapterModuleRequirement],
    ) -> Result<(), PackageValidationError> {
        let mut declared_proxies = BTreeSet::new();
        for requirement in adapter_requirements {
            if !declared_proxies.insert(requirement.proxy_module.as_str()) {
                return Err(PackageValidationError::DuplicateAdapterRequirement {
                    proxy_module: requirement.proxy_module.clone(),
                });
            }
        }

        let declared_proxy_modules = catalog
            .iter()
            .map(|descriptor| descriptor.module_path().as_str())
            .filter(|path| path.ends_with(".gfp"))
            .collect::<BTreeSet<_>>();

        if let Some(proxy_module) = declared_proxy_modules
            .iter()
            .find(|proxy_module| !declared_proxies.contains(**proxy_module))
        {
            return Err(PackageValidationError::MissingAdapterRequirement {
                proxy_module: (*proxy_module).to_string(),
            });
        }

        if let Some(proxy_module) = declared_proxies
            .iter()
            .find(|proxy_module| !declared_proxy_modules.contains(**proxy_module))
        {
            return Err(PackageValidationError::UnexpectedAdapterRequirement {
                proxy_module: (*proxy_module).to_string(),
            });
        }

        Ok(())
    }

    fn validate_provider_requirements(
        _catalog: &ModuleCatalog,
        provider_requirements: &[ProviderModuleRequirement],
    ) -> Result<(), PackageValidationError> {
        let mut paths = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        for requirement in provider_requirements {
            if !paths.insert(requirement.module_path.as_str()) {
                return Err(PackageValidationError::DuplicateProviderRequirement {
                    module_path: requirement.module_path.clone(),
                });
            }
            if !aliases.insert(requirement.alias.as_str()) {
                return Err(PackageValidationError::DuplicateProviderAlias {
                    alias: requirement.alias.clone(),
                });
            }
        }
        Ok(())
    }

    fn validate_descriptor_requirements(
        catalog: &ModuleCatalog,
        adapter_requirements: &[AdapterModuleRequirement],
        provider_requirements: &[ProviderModuleRequirement],
    ) -> Result<(), PackageValidationError> {
        let adapter_paths = adapter_requirements
            .iter()
            .map(|requirement| requirement.proxy_module.as_str())
            .collect::<BTreeSet<_>>();
        let provider_paths = provider_requirements
            .iter()
            .map(|requirement| requirement.module_path.as_str())
            .collect::<BTreeSet<_>>();

        for descriptor in catalog.iter() {
            for provider_module in descriptor.provider_modules() {
                let target = catalog.get(*provider_module).expect(
                    "module catalog validates provider requirement targets before package validation",
                );
                let path = target
                    .module_path()
                    .as_str()
                    .strip_suffix(".gfs")
                    .unwrap_or(target.module_path().as_str());
                if !provider_paths.contains(path) {
                    return Err(
                        PackageValidationError::UndeclaredProviderModuleRequirement {
                            module_id: descriptor.module_id(),
                            provider_module: *provider_module,
                        },
                    );
                }
            }
            for adapter_proxy_module in descriptor.adapter_proxy_modules() {
                let target = catalog.get(*adapter_proxy_module).expect(
                    "module catalog validates adapter proxy requirement targets before package validation",
                );
                if !adapter_paths.contains(target.module_path().as_str()) {
                    return Err(PackageValidationError::UndeclaredAdapterProxyRequirement {
                        module_id: descriptor.module_id(),
                        adapter_proxy_module: *adapter_proxy_module,
                    });
                }
            }
        }
        Ok(())
    }

    /// Returns the transient compilation graph when this image was constructed in memory.
    ///
    /// Encoded package images intentionally do not retain this graph; hosts must use the
    /// catalog and chunks instead of this inspection-only view.
    pub fn graph(&self) -> &BytecodeGraph {
        self.graph
            .as_deref()
            .expect("encoded package images do not retain a bytecode graph")
    }

    pub fn target(&self) -> &ExecutionTarget {
        &self.target
    }

    pub fn entry_point(&self) -> Option<&PackageEntryPoint> {
        self.entry_point.as_ref()
    }

    pub fn metadata(&self) -> &PackageMetadata {
        &self.metadata
    }

    pub fn limits(&self) -> &LimitsMetadata {
        &self.limits
    }

    pub fn adapter_requirements(&self) -> &[AdapterModuleRequirement] {
        self.adapter_requirements.as_slice()
    }

    pub fn provider_requirements(&self) -> &[ProviderModuleRequirement] {
        self.provider_requirements.as_slice()
    }

    /// Immutable catalog describing every module available to this package.
    pub fn catalog(&self) -> &ModuleCatalog {
        &self.catalog
    }

    /// Immutable canonical chunks for every catalog-declared module.
    pub fn chunks(&self) -> &ModuleChunkStore {
        &self.chunks
    }

    pub const fn versions(&self) -> PackageVersions {
        self.versions
    }

    /// Encodes this immutable package with a fixed-width, deterministic layout.
    ///
    /// Graph snapshot revisions and debug locations are not part of a package image,
    /// because they do not affect executable behavior.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, PackageEncodingError> {
        postcard::to_stdvec(self).map_err(PackageEncodingError::from)
    }

    /// Serializes this package into the compact transport representation used by loaders.
    pub fn to_bytecode(&self) -> Result<Vec<u8>, PackageEncodingError> {
        self.canonical_bytes()
    }

    /// Decodes and validates a package received from a loader before it reaches the runtime.
    pub fn from_bytecode(bytes: &[u8]) -> Result<Self, PackageDecodingError> {
        let package = postcard::from_bytes::<Self>(bytes)?;
        Self::decode_v7(package)
    }

    fn decode_v7(package: Self) -> Result<Self, PackageDecodingError> {
        if package.versions.package_format() != CURRENT_PACKAGE_FORMAT_VERSION {
            return Err(PackageDecodingError::UnsupportedPackageFormat {
                supported: CURRENT_PACKAGE_FORMAT_VERSION,
                actual: package.versions.package_format(),
            });
        }
        Self::validate_adapter_requirements(&package.catalog, &package.adapter_requirements)?;
        Self::validate_provider_requirements(&package.catalog, &package.provider_requirements)?;
        Self::validate_chunks(
            &package.catalog,
            &package.chunks,
            package.versions.bytecode_format(),
        )?;
        Ok(package)
    }

    pub fn content_hash(&self) -> Result<ContentHash, PackageEncodingError> {
        self.canonical_bytes().map(|bytes| ContentHash::of(&bytes))
    }
}
