#[cfg(test)]
mod tests;

use galfus_contract::{
    AdapterModuleRequirement, BoundaryAbiVersion, CURRENT_BOUNDARY_ABI_VERSION,
    CURRENT_PRODUCER_VERSION, ProducerVersion,
};
use galfus_core::ModulePath;

use crate::{
    BytecodeFormatVersion, BytecodeGraph, CURRENT_PACKAGE_FORMAT_VERSION, PackageFormatVersion,
};

/// The exported entry point of a package image.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug)]
pub struct PackageImage {
    graph: BytecodeGraph,
    entry_point: Option<PackageEntryPoint>,
    adapter_requirements: Vec<AdapterModuleRequirement>,
    versions: PackageVersions,
}

impl PackageImage {
    pub fn new(
        graph: BytecodeGraph,
        entry_point: Option<PackageEntryPoint>,
        adapter_requirements: Vec<AdapterModuleRequirement>,
    ) -> Self {
        Self {
            versions: PackageVersions::for_bytecode(graph.format_version()),
            graph,
            entry_point,
            adapter_requirements,
        }
    }

    pub fn graph(&self) -> &BytecodeGraph {
        &self.graph
    }

    pub fn entry_point(&self) -> Option<&PackageEntryPoint> {
        self.entry_point.as_ref()
    }

    pub fn adapter_requirements(&self) -> &[AdapterModuleRequirement] {
        self.adapter_requirements.as_slice()
    }

    pub const fn versions(&self) -> PackageVersions {
        self.versions
    }
}
