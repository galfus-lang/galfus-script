#[cfg(test)]
mod tests;

mod derive;

pub use derive::{
    ModuleCatalogDerivationError, attach_capability_requirements, derive_module_catalog,
    derive_module_catalog_with_capability_requirements,
};

use std::collections::{BTreeSet, HashMap};

use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath, RuntimeExportId, RuntimeExportKind};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize)]
struct ModuleInterface<'a> {
    module_id: ModuleId,
    module_path: &'a ModulePath,
    dependencies: &'a [ModuleDependency],
    provider_modules: &'a [ModuleId],
    adapter_proxy_modules: &'a [ModuleId],
    exports: &'a [ModuleExportDescriptor],
    has_initializer: bool,
}

/// One direct dependency declared by a module interface.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ModuleDependency {
    module_id: ModuleId,
}

impl ModuleDependency {
    pub const fn new(module_id: ModuleId) -> Self {
        Self { module_id }
    }

    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }
}

/// A stable exported symbol in one module interface.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ModuleExportDescriptor {
    runtime_export_id: RuntimeExportId,
    name: String,
    kind: RuntimeExportKind,
}

impl ModuleExportDescriptor {
    pub fn new(
        runtime_export_id: RuntimeExportId,
        name: impl Into<String>,
        kind: RuntimeExportKind,
    ) -> Self {
        Self {
            runtime_export_id,
            name: name.into(),
            kind,
        }
    }

    pub const fn runtime_export_id(&self) -> RuntimeExportId {
        self.runtime_export_id
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub const fn kind(&self) -> RuntimeExportKind {
        self.kind
    }
}

/// Immutable interface and materialization metadata for one possible module.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModuleDescriptor {
    module_id: ModuleId,
    module_path: ModulePath,
    dependencies: Vec<ModuleDependency>,
    provider_modules: Vec<ModuleId>,
    adapter_proxy_modules: Vec<ModuleId>,
    exports: Vec<ModuleExportDescriptor>,
    has_initializer: bool,
    interface_hash: ContentHash,
    chunk_hash: ContentHash,
}

impl ModuleDescriptor {
    pub fn new(
        module_id: ModuleId,
        module_path: ModulePath,
        dependencies: Vec<ModuleDependency>,
        exports: Vec<ModuleExportDescriptor>,
        has_initializer: bool,
        interface_hash: ContentHash,
        chunk_hash: ContentHash,
    ) -> Result<Self, ModuleDescriptorError> {
        Self::new_with_capability_requirements(
            module_id,
            module_path,
            dependencies,
            Vec::new(),
            Vec::new(),
            exports,
            has_initializer,
            interface_hash,
            chunk_hash,
        )
    }

    pub fn new_with_capability_requirements(
        module_id: ModuleId,
        module_path: ModulePath,
        dependencies: Vec<ModuleDependency>,
        provider_modules: Vec<ModuleId>,
        adapter_proxy_modules: Vec<ModuleId>,
        exports: Vec<ModuleExportDescriptor>,
        has_initializer: bool,
        interface_hash: ContentHash,
        chunk_hash: ContentHash,
    ) -> Result<Self, ModuleDescriptorError> {
        let mut descriptor = Self {
            module_id,
            module_path,
            dependencies,
            provider_modules,
            adapter_proxy_modules,
            exports,
            has_initializer,
            interface_hash,
            chunk_hash,
        };
        descriptor.canonicalize()?;

        Ok(descriptor)
    }

    fn canonicalize(&mut self) -> Result<(), ModuleDescriptorError> {
        self.dependencies.sort_unstable();
        self.provider_modules.sort_unstable();
        self.adapter_proxy_modules.sort_unstable();
        self.exports.sort_unstable_by(|left, right| {
            left.runtime_export_id
                .cmp(&right.runtime_export_id)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.kind.cmp(&right.kind))
        });

        if let Some(dependency) = self
            .dependencies
            .windows(2)
            .find(|pair| pair[0].module_id == pair[1].module_id)
            .map(|pair| pair[0].module_id)
        {
            return Err(ModuleDescriptorError::DuplicateDependency {
                module_id: self.module_id,
                dependency,
            });
        }

        if let Some(provider_module) = self
            .provider_modules
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ModuleDescriptorError::DuplicateProviderModule {
                module_id: self.module_id,
                provider_module,
            });
        }

        if let Some(adapter_proxy_module) = self
            .adapter_proxy_modules
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ModuleDescriptorError::DuplicateAdapterProxyModule {
                module_id: self.module_id,
                adapter_proxy_module,
            });
        }

        if let Some(runtime_export_id) = self
            .exports
            .windows(2)
            .find(|pair| pair[0].runtime_export_id == pair[1].runtime_export_id)
            .map(|pair| pair[0].runtime_export_id)
        {
            return Err(ModuleDescriptorError::DuplicateRuntimeExportId {
                module_id: self.module_id,
                runtime_export_id,
            });
        }

        for export in &self.exports {
            let expected = RuntimeExportId::new(self.module_id, export.kind, export.name.as_str());
            if export.runtime_export_id != expected {
                return Err(ModuleDescriptorError::RuntimeExportIdMismatch {
                    module_id: self.module_id,
                    name: export.name.clone(),
                    expected,
                    actual: export.runtime_export_id,
                });
            }
        }

        Ok(())
    }

    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub fn module_path(&self) -> &ModulePath {
        &self.module_path
    }

    pub fn dependencies(&self) -> &[ModuleDependency] {
        self.dependencies.as_slice()
    }

    /// Direct provider modules this module can invoke through its emitted body.
    pub fn provider_modules(&self) -> &[ModuleId] {
        self.provider_modules.as_slice()
    }

    /// Direct adapter proxy modules this module can invoke through its emitted body.
    pub fn adapter_proxy_modules(&self) -> &[ModuleId] {
        self.adapter_proxy_modules.as_slice()
    }

    pub fn exports(&self) -> &[ModuleExportDescriptor] {
        self.exports.as_slice()
    }

    pub const fn has_initializer(&self) -> bool {
        self.has_initializer
    }

    pub const fn interface_hash(&self) -> ContentHash {
        self.interface_hash
    }

    pub const fn chunk_hash(&self) -> ContentHash {
        self.chunk_hash
    }

    /// Hashes the stable interface fields independently from the body chunk.
    pub fn computed_interface_hash(&self) -> Result<ContentHash, postcard::Error> {
        let bytes = postcard::to_stdvec(&ModuleInterface {
            module_id: self.module_id,
            module_path: &self.module_path,
            dependencies: self.dependencies.as_slice(),
            provider_modules: self.provider_modules.as_slice(),
            adapter_proxy_modules: self.adapter_proxy_modules.as_slice(),
            exports: self.exports.as_slice(),
            has_initializer: self.has_initializer,
        })?;
        Ok(ContentHash::of(bytes.as_slice()))
    }
}

/// Immutable, canonical catalog of every module a package may materialize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCatalog {
    descriptors: Vec<ModuleDescriptor>,
    indexes_by_id: HashMap<ModuleId, usize>,
}

impl ModuleCatalog {
    pub fn new(mut descriptors: Vec<ModuleDescriptor>) -> Result<Self, ModuleCatalogError> {
        descriptors.sort_unstable_by_key(ModuleDescriptor::module_id);

        if let Some(module_id) = descriptors
            .windows(2)
            .find(|pair| pair[0].module_id == pair[1].module_id)
            .map(|pair| pair[0].module_id)
        {
            return Err(ModuleCatalogError::DuplicateModuleId { module_id });
        }

        let mut paths = BTreeSet::new();
        let mut exports = BTreeSet::new();
        let mut indexes_by_id = HashMap::with_capacity(descriptors.len());

        for (index, descriptor) in descriptors.iter_mut().enumerate() {
            descriptor.canonicalize()?;

            if !paths.insert(descriptor.module_path.clone()) {
                return Err(ModuleCatalogError::DuplicateModulePath {
                    module_path: descriptor.module_path.clone(),
                });
            }

            for export in &descriptor.exports {
                if !exports.insert((descriptor.module_id, export.runtime_export_id)) {
                    return Err(ModuleCatalogError::DuplicateRuntimeExportId {
                        module_id: descriptor.module_id,
                        runtime_export_id: export.runtime_export_id,
                    });
                }
            }

            indexes_by_id.insert(descriptor.module_id, index);
        }

        for descriptor in &descriptors {
            for dependency in &descriptor.dependencies {
                if !indexes_by_id.contains_key(&dependency.module_id) {
                    return Err(ModuleCatalogError::MissingDependency {
                        module_id: descriptor.module_id,
                        dependency: dependency.module_id,
                    });
                }
            }
            for provider_module in &descriptor.provider_modules {
                if !indexes_by_id.contains_key(provider_module) {
                    return Err(ModuleCatalogError::MissingProviderModule {
                        module_id: descriptor.module_id,
                        provider_module: *provider_module,
                    });
                }
            }
            for adapter_proxy_module in &descriptor.adapter_proxy_modules {
                if !indexes_by_id.contains_key(adapter_proxy_module) {
                    return Err(ModuleCatalogError::MissingAdapterProxyModule {
                        module_id: descriptor.module_id,
                        adapter_proxy_module: *adapter_proxy_module,
                    });
                }
            }
        }

        Ok(Self {
            descriptors,
            indexes_by_id,
        })
    }

    pub fn get(&self, module_id: ModuleId) -> Option<&ModuleDescriptor> {
        self.indexes_by_id
            .get(&module_id)
            .map(|index| &self.descriptors[*index])
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ModuleDescriptor> {
        self.descriptors.iter()
    }

    pub const fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

impl Serialize for ModuleCatalog {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.descriptors.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModuleCatalog {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let descriptors = Vec::<ModuleDescriptor>::deserialize(deserializer)?;
        Self::new(descriptors).map_err(<D::Error as serde::de::Error>::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModuleCatalogError {
    #[error("module catalog contains module ID {module_id:?} more than once")]
    DuplicateModuleId { module_id: ModuleId },
    #[error("module catalog maps logical path {module_path} more than once")]
    DuplicateModulePath { module_path: ModulePath },
    #[error("module {module_id:?} depends on absent module {dependency:?}")]
    MissingDependency {
        module_id: ModuleId,
        dependency: ModuleId,
    },
    #[error("module {module_id:?} requires absent provider module {provider_module:?}")]
    MissingProviderModule {
        module_id: ModuleId,
        provider_module: ModuleId,
    },
    #[error("module {module_id:?} requires absent adapter proxy module {adapter_proxy_module:?}")]
    MissingAdapterProxyModule {
        module_id: ModuleId,
        adapter_proxy_module: ModuleId,
    },
    #[error("module {module_id:?} lists runtime export ID {runtime_export_id:?} more than once")]
    DuplicateRuntimeExportId {
        module_id: ModuleId,
        runtime_export_id: RuntimeExportId,
    },
    #[error(transparent)]
    InvalidDescriptor(#[from] ModuleDescriptorError),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModuleDescriptorError {
    #[error("module {module_id:?} lists dependency {dependency:?} more than once")]
    DuplicateDependency {
        module_id: ModuleId,
        dependency: ModuleId,
    },
    #[error("module {module_id:?} lists provider module {provider_module:?} more than once")]
    DuplicateProviderModule {
        module_id: ModuleId,
        provider_module: ModuleId,
    },
    #[error(
        "module {module_id:?} lists adapter proxy module {adapter_proxy_module:?} more than once"
    )]
    DuplicateAdapterProxyModule {
        module_id: ModuleId,
        adapter_proxy_module: ModuleId,
    },
    #[error("module {module_id:?} lists runtime export ID {runtime_export_id:?} more than once")]
    DuplicateRuntimeExportId {
        module_id: ModuleId,
        runtime_export_id: RuntimeExportId,
    },
    #[error(
        "module {module_id:?} export {name} has runtime export ID {actual:?}, expected {expected:?}"
    )]
    RuntimeExportIdMismatch {
        module_id: ModuleId,
        name: String,
        expected: RuntimeExportId,
        actual: RuntimeExportId,
    },
}
