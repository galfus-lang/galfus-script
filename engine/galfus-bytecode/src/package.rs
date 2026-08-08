#[cfg(test)]
mod tests;

use bincode::Options;
use galfus_contract::{
    AdapterModuleRequirement, BoundaryAbiVersion, CURRENT_BOUNDARY_ABI_VERSION,
    CURRENT_PRODUCER_VERSION, ContentHash, ExecutionTarget, ProducerVersion,
    ProviderModuleRequirement,
};
use galfus_core::{ModuleId, ModulePath};
use std::collections::BTreeSet;

use crate::{
    BytecodeFormatVersion, BytecodeGraph, CURRENT_PACKAGE_FORMAT_VERSION, PackageFormatVersion,
};

/// The exported entry point of a package image.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
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

/// Version contracts recorded with a package image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PackageVersions {
    producer: ProducerVersion,
    package_format: PackageFormatVersion,
    bytecode_format: BytecodeFormatVersion,
    boundary_abi: BoundaryAbiVersion,
}

impl PackageVersions {
    pub const fn for_bytecode(bytecode_format: BytecodeFormatVersion) -> Self {
        Self {
            producer: CURRENT_PRODUCER_VERSION,
            package_format: CURRENT_PACKAGE_FORMAT_VERSION,
            bytecode_format,
            boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
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
}

/// Immutable compiled output delivered to a host.
///
/// The graph and its declarative external requirements are created together
/// and cannot be replaced independently after publication.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PackageImage {
    graph: BytecodeGraph,
    target: ExecutionTarget,
    entry_point: Option<PackageEntryPoint>,
    adapter_requirements: Vec<AdapterModuleRequirement>,
    provider_requirements: Vec<ProviderModuleRequirement>,
    versions: PackageVersions,
}

/// Errors that prevent a package image from having an exact adapter manifest.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PackageValidationError {
    #[error("adapter requirement for `{proxy_module}` is duplicated")]
    DuplicateAdapterRequirement { proxy_module: String },
    #[error("reachable adapter proxy `{proxy_module}` is missing from the package manifest")]
    MissingAdapterRequirement { proxy_module: String },
    #[error("adapter requirement `{proxy_module}` does not match a reachable adapter proxy")]
    UnexpectedAdapterRequirement { proxy_module: String },
    #[error("provider requirement for `{module_path}` is duplicated")]
    DuplicateProviderRequirement { module_path: String },
}

#[derive(Debug, thiserror::Error)]
#[error("could not encode the canonical package image: {0}")]
pub struct PackageEncodingError(#[from] bincode::Error);

impl PackageImage {
    pub fn try_new(
        graph: BytecodeGraph,
        target: ExecutionTarget,
        entry_point: Option<PackageEntryPoint>,
        mut adapter_requirements: Vec<AdapterModuleRequirement>,
        mut provider_requirements: Vec<ProviderModuleRequirement>,
    ) -> Result<Self, PackageValidationError> {
        for requirement in &mut adapter_requirements {
            requirement.descriptor.canonicalize();
        }
        adapter_requirements.sort_by(|left, right| left.proxy_module.cmp(&right.proxy_module));
        provider_requirements.sort_by(|left, right| left.module_path.cmp(&right.module_path));
        Self::validate_adapter_requirements(&graph, entry_point.as_ref(), &adapter_requirements)?;
        Self::validate_provider_requirements(&provider_requirements)?;

        Ok(Self {
            versions: PackageVersions::for_bytecode(graph.format_version()),
            graph,
            target,
            entry_point,
            adapter_requirements,
            provider_requirements,
        })
    }

    fn validate_adapter_requirements(
        graph: &BytecodeGraph,
        entry_point: Option<&PackageEntryPoint>,
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

        let reachable_modules = match entry_point {
            Some(entry_point) => graph
                .modules()
                .find(|module| module.path() == entry_point.module_path())
                .map(|entry| Self::reachable_modules(graph, entry.id()))
                .unwrap_or_default(),
            None => graph.modules().map(|module| module.id()).collect(),
        };
        let reachable_proxies = reachable_modules
            .into_iter()
            .filter_map(|module_id| graph.get(module_id))
            .map(|module| module.path().as_str())
            .filter(|path| path.ends_with(".gfp"))
            .collect::<BTreeSet<_>>();

        if let Some(proxy_module) = reachable_proxies
            .iter()
            .find(|proxy_module| !declared_proxies.contains(**proxy_module))
        {
            return Err(PackageValidationError::MissingAdapterRequirement {
                proxy_module: (*proxy_module).to_string(),
            });
        }

        if let Some(proxy_module) = declared_proxies
            .iter()
            .find(|proxy_module| !reachable_proxies.contains(**proxy_module))
        {
            return Err(PackageValidationError::UnexpectedAdapterRequirement {
                proxy_module: (*proxy_module).to_string(),
            });
        }

        Ok(())
    }

    fn reachable_modules(
        graph: &BytecodeGraph,
        entry: galfus_core::ModuleId,
    ) -> BTreeSet<ModuleId> {
        let mut reachable = BTreeSet::from([entry]);
        let mut pending = vec![entry];

        while let Some(module_id) = pending.pop() {
            for dependency in graph.deps_of(module_id) {
                if reachable.insert(dependency) {
                    pending.push(dependency);
                }
            }
        }

        reachable
    }

    fn validate_provider_requirements(
        provider_requirements: &[ProviderModuleRequirement],
    ) -> Result<(), PackageValidationError> {
        let mut paths = BTreeSet::new();
        for requirement in provider_requirements {
            if !paths.insert(requirement.module_path.as_str()) {
                return Err(PackageValidationError::DuplicateProviderRequirement {
                    module_path: requirement.module_path.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn graph(&self) -> &BytecodeGraph {
        &self.graph
    }

    pub fn target(&self) -> &ExecutionTarget {
        &self.target
    }

    pub fn entry_point(&self) -> Option<&PackageEntryPoint> {
        self.entry_point.as_ref()
    }

    pub fn adapter_requirements(&self) -> &[AdapterModuleRequirement] {
        self.adapter_requirements.as_slice()
    }

    pub fn provider_requirements(&self) -> &[ProviderModuleRequirement] {
        self.provider_requirements.as_slice()
    }

    pub const fn versions(&self) -> PackageVersions {
        self.versions
    }

    /// Encodes this immutable package with a fixed-width, deterministic layout.
    ///
    /// Graph snapshot revisions and debug locations are not part of a package image,
    /// because they do not affect executable behavior.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, PackageEncodingError> {
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_little_endian()
            .serialize(self)
            .map_err(PackageEncodingError::from)
    }

    pub fn content_hash(&self) -> Result<ContentHash, PackageEncodingError> {
        self.canonical_bytes().map(|bytes| ContentHash::of(&bytes))
    }
}
