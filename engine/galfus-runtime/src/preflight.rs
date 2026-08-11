use galfus_bytecode::PackageImage;
use galfus_contract::{
    AdapterArtifactIntegrityError, AdapterBindings, AdapterLoadContext, AdapterLoadError,
    AdapterModuleLoader, AdapterModuleRequirement, SelectedAdapterTarget,
};
use std::collections::HashMap;

/// Errors raised while resolving the adapter requirements of a package for one host.
#[derive(Debug)]
pub enum PreflightError {
    MissingLoader(String),
    LoadFailed {
        proxy_module: String,
        adapter: String,
        error: AdapterLoadError,
    },
    DuplicateLoader(String),
    PackageTargetMismatch {
        package_target: String,
        host_target: String,
    },
    MissingAdapterTarget {
        proxy_module: String,
        target: String,
    },
    ArtifactIntegrityFailed {
        proxy_module: String,
        error: AdapterArtifactIntegrityError,
    },
    DescriptorMismatch {
        proxy_module: String,
    },
}

impl std::fmt::Display for PreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLoader(adapter) => {
                write!(f, "missing required loader for adapter: {adapter}")
            }
            Self::LoadFailed {
                proxy_module,
                adapter,
                error,
            } => write!(
                f,
                "failed to load module {proxy_module} using adapter {adapter}: {error}"
            ),
            Self::DuplicateLoader(adapter) => {
                write!(f, "duplicate loader registered for adapter: {adapter}")
            }
            Self::PackageTargetMismatch {
                package_target,
                host_target,
            } => write!(
                f,
                "package targets {package_target}, but this execution host targets {host_target}"
            ),
            Self::MissingAdapterTarget {
                proxy_module,
                target,
            } => write!(
                f,
                "adapter proxy {proxy_module} has no artifact target for {target}"
            ),
            Self::ArtifactIntegrityFailed {
                proxy_module,
                error,
            } => write!(
                f,
                "adapter artifact for {proxy_module} failed integrity verification: {error}"
            ),
            Self::DescriptorMismatch { proxy_module } => write!(
                f,
                "adapter binding descriptor does not match proxy module {proxy_module}"
            ),
        }
    }
}

impl std::error::Error for PreflightError {}

/// Resolves adapter declarations from a package into sealed runtime bindings.
pub struct AdapterBindingPreflight {
    loaders: HashMap<String, Box<dyn AdapterModuleLoader>>,
}

impl Default for AdapterBindingPreflight {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterBindingPreflight {
    pub fn new() -> Self {
        Self {
            loaders: HashMap::new(),
        }
    }

    pub fn register_loader(
        &mut self,
        adapter_name: impl Into<String>,
        loader: Box<dyn AdapterModuleLoader>,
    ) -> Result<(), PreflightError> {
        let name = adapter_name.into();
        if self.loaders.contains_key(&name) {
            return Err(PreflightError::DuplicateLoader(name));
        }
        self.loaders.insert(name, loader);
        Ok(())
    }

    pub fn bind_package(
        &self,
        package: &PackageImage,
        context: &AdapterLoadContext,
    ) -> Result<AdapterBindings, PreflightError> {
        if package.target() != &context.target {
            return Err(PreflightError::PackageTargetMismatch {
                package_target: package.target().as_str().to_string(),
                host_target: context.target.as_str().to_string(),
            });
        }
        self.bind_requirements(package.adapter_requirements(), context)
    }

    fn bind_requirements(
        &self,
        requirements: &[AdapterModuleRequirement],
        context: &AdapterLoadContext,
    ) -> Result<AdapterBindings, PreflightError> {
        let mut bindings = AdapterBindings::default();

        for requirement in requirements {
            let adapter_name = &requirement.descriptor.adapter;
            let loader = self
                .loaders
                .get(adapter_name)
                .ok_or_else(|| PreflightError::MissingLoader(adapter_name.clone()))?;
            let target = requirement
                .descriptor
                .targets
                .iter()
                .find(|target| target.target == context.target)
                .cloned()
                .ok_or_else(|| PreflightError::MissingAdapterTarget {
                    proxy_module: requirement.proxy_module.clone(),
                    target: context.target.as_str().to_string(),
                })?;
            let selected_target = SelectedAdapterTarget {
                proxy_module: requirement.proxy_module.clone(),
                target,
                boundary_abi: requirement.boundary_abi,
            };
            let artifact = loader
                .load_artifact(&selected_target, context)
                .map_err(|error| PreflightError::LoadFailed {
                    proxy_module: requirement.proxy_module.clone(),
                    adapter: adapter_name.clone(),
                    error,
                })?;
            let artifact = selected_target
                .target
                .artifact
                .verify(artifact)
                .map_err(|error| PreflightError::ArtifactIntegrityFailed {
                    proxy_module: requirement.proxy_module.clone(),
                    error,
                })?;
            let bound_module = loader
                .load_module(requirement, &selected_target, artifact, context)
                .map_err(|error| PreflightError::LoadFailed {
                    proxy_module: requirement.proxy_module.clone(),
                    adapter: adapter_name.clone(),
                    error,
                })?;

            if bound_module.descriptor() != requirement.descriptor {
                return Err(PreflightError::DescriptorMismatch {
                    proxy_module: requirement.proxy_module.clone(),
                });
            }

            bindings
                .register_module(requirement.proxy_module.clone(), bound_module)
                .map_err(|error| PreflightError::LoadFailed {
                    proxy_module: requirement.proxy_module.clone(),
                    adapter: adapter_name.clone(),
                    error: AdapterLoadError {
                        code: "duplicate_proxy_module".to_string(),
                        message: error.to_string(),
                    },
                })?;
        }

        Ok(bindings)
    }
}
