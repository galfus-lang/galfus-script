#[cfg(test)]
mod tests;

use galfus_bytecode::PackageImage;
use galfus_bytecode::{BytecodeNode, ModuleCatalog, ModuleResolveContext, ModuleResolveError};
use galfus_contract::{
    AdapterArtifactIntegrityError, AdapterBindings, AdapterLoadContext, AdapterLoadError,
    AdapterModuleLoader, AdapterModuleRequirement, SelectedAdapterTarget,
};
use std::collections::HashMap;
use std::sync;

use crate::{ModuleProducer, RuntimeError};
use galfus_contract::{ProviderModuleRequirement, Providers};
use galfus_core::ModuleId;

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

/// Validates every capability declared by a standalone package before any
/// executable module is materialized or initialized.
pub(crate) fn validate_package_capabilities(
    adapter_requirements: &[AdapterModuleRequirement],
    provider_requirements: &[ProviderModuleRequirement],
    providers: Option<&sync::Arc<sync::Mutex<Providers>>>,
    adapter_bindings: &sync::Arc<sync::Mutex<AdapterBindings>>,
) -> Result<(), RuntimeError> {
    let bindings = adapter_bindings
        .lock()
        .expect("runtime owns the adapter capability table");
    for requirement in adapter_requirements {
        if !bindings.validates(requirement) {
            return Err(RuntimeError::AdapterRequirementUnsatisfied {
                proxy_module: requirement.proxy_module.clone(),
            });
        }
    }
    drop(bindings);

    for requirement in provider_requirements {
        let is_satisfied = providers
            .and_then(|providers| providers.lock().ok())
            .is_some_and(|providers| providers.validates(requirement));
        if !is_satisfied {
            return Err(RuntimeError::ProviderRequirementUnsatisfied {
                module_path: requirement.module_path.clone(),
            });
        }
    }

    Ok(())
}

/// Validates source-workspace capability declarations at startup, then checks
/// only the requirements named by each materialized module.
pub(crate) struct LazyCapabilityPreflight {
    catalog: ModuleCatalog,
    provider_requirements: HashMap<ModuleId, ProviderModuleRequirement>,
    adapter_requirements: HashMap<ModuleId, AdapterModuleRequirement>,
    providers: Option<sync::Arc<sync::Mutex<Providers>>>,
    adapter_bindings: sync::Arc<sync::Mutex<AdapterBindings>>,
    provider_bindings: sync::Mutex<HashMap<ModuleId, Result<(), String>>>,
    adapter_bindings_cache: sync::Mutex<HashMap<ModuleId, Result<(), String>>>,
}

impl LazyCapabilityPreflight {
    pub(crate) fn new(
        catalog: &ModuleCatalog,
        adapter_requirements: &[AdapterModuleRequirement],
        provider_requirements: &[ProviderModuleRequirement],
        providers: Option<sync::Arc<sync::Mutex<Providers>>>,
        adapter_bindings: sync::Arc<sync::Mutex<AdapterBindings>>,
    ) -> Result<sync::Arc<Self>, RuntimeError> {
        let provider_requirements = requirement_modules(
            catalog,
            provider_requirements,
            |descriptor, requirement| {
                descriptor
                    .module_path()
                    .as_str()
                    .strip_suffix(".gfs")
                    .is_some_and(|path| path == requirement.module_path)
            },
            "provider",
        )?;
        let adapter_requirements = requirement_modules(
            catalog,
            adapter_requirements,
            |descriptor, requirement| descriptor.module_path().as_str() == requirement.proxy_module,
            "adapter proxy",
        )?;
        for descriptor in catalog.iter() {
            for provider_module in descriptor.provider_modules() {
                if !provider_requirements.contains_key(provider_module) {
                    return Err(RuntimeError::SourceCapabilityConfiguration {
                        reason: format!(
                            "module {:?} requires undeclared provider module {:?}",
                            descriptor.module_id(),
                            provider_module
                        ),
                    });
                }
            }
            for adapter_proxy_module in descriptor.adapter_proxy_modules() {
                if !adapter_requirements.contains_key(adapter_proxy_module) {
                    return Err(RuntimeError::SourceCapabilityConfiguration {
                        reason: format!(
                            "module {:?} requires undeclared adapter proxy module {:?}",
                            descriptor.module_id(),
                            adapter_proxy_module
                        ),
                    });
                }
            }
        }

        Ok(sync::Arc::new(Self {
            catalog: catalog.clone(),
            provider_requirements,
            adapter_requirements,
            providers,
            adapter_bindings,
            provider_bindings: sync::Mutex::new(HashMap::new()),
            adapter_bindings_cache: sync::Mutex::new(HashMap::new()),
        }))
    }

    pub(crate) fn validate_module(&self, module_id: ModuleId) -> Result<(), ModuleResolveError> {
        let descriptor =
            self.catalog
                .get(module_id)
                .ok_or_else(|| ModuleResolveError::UnknownModule {
                    context: ModuleResolveContext::new(module_id),
                })?;
        let context = ModuleResolveContext::with_path(module_id, descriptor.module_path().clone());

        for provider_module in descriptor.provider_modules() {
            let requirement = self.provider_requirements.get(provider_module).expect(
                "workspace capability declarations are validated before module materialization",
            );
            if let Err(module_path) = self.bind_provider(*provider_module, requirement) {
                return Err(ModuleResolveError::ProviderRequirementUnsatisfied {
                    context,
                    module_path,
                });
            }
        }
        for adapter_proxy_module in descriptor.adapter_proxy_modules() {
            let requirement = self.adapter_requirements.get(adapter_proxy_module).expect(
                "workspace capability declarations are validated before module materialization",
            );
            if let Err(proxy_module) = self.bind_adapter(*adapter_proxy_module, requirement) {
                return Err(ModuleResolveError::AdapterRequirementUnsatisfied {
                    context,
                    proxy_module,
                });
            }
        }
        Ok(())
    }

    fn bind_provider(
        &self,
        module_id: ModuleId,
        requirement: &ProviderModuleRequirement,
    ) -> Result<(), String> {
        cached_binding(&self.provider_bindings, module_id, || {
            self.providers
                .as_ref()
                .and_then(|providers| providers.lock().ok())
                .is_some_and(|providers| providers.validates(requirement))
                .then_some(())
                .ok_or_else(|| requirement.module_path.clone())
        })
    }

    fn bind_adapter(
        &self,
        module_id: ModuleId,
        requirement: &AdapterModuleRequirement,
    ) -> Result<(), String> {
        cached_binding(&self.adapter_bindings_cache, module_id, || {
            self.adapter_bindings
                .lock()
                .ok()
                .is_some_and(|bindings| bindings.validates(requirement))
                .then_some(())
                .ok_or_else(|| requirement.proxy_module.clone())
        })
    }
}

fn requirement_modules<R>(
    catalog: &ModuleCatalog,
    requirements: &[R],
    matches_descriptor: impl Fn(&galfus_bytecode::ModuleDescriptor, &R) -> bool,
    label: &str,
) -> Result<HashMap<ModuleId, R>, RuntimeError>
where
    R: Clone,
{
    let mut modules = HashMap::new();
    for requirement in requirements {
        let descriptor = catalog
            .iter()
            .find(|descriptor| matches_descriptor(descriptor, requirement))
            .ok_or_else(|| RuntimeError::SourceCapabilityConfiguration {
                reason: format!("{label} declaration does not match a source module"),
            })?;
        if modules
            .insert(descriptor.module_id(), requirement.clone())
            .is_some()
        {
            return Err(RuntimeError::SourceCapabilityConfiguration {
                reason: format!(
                    "{label} declaration is duplicated for module {:?}",
                    descriptor.module_id()
                ),
            });
        }
    }
    Ok(modules)
}

fn cached_binding(
    bindings: &sync::Mutex<HashMap<ModuleId, Result<(), String>>>,
    module_id: ModuleId,
    bind: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let mut bindings = bindings
        .lock()
        .expect("workspace capability binding cache is available");
    if let Some(result) = bindings.get(&module_id).cloned() {
        return result;
    }
    let result = bind();
    bindings.insert(module_id, result.clone());
    result
}

pub(crate) struct CapabilityValidatedModuleProducer {
    producer: sync::Arc<dyn ModuleProducer>,
    preflight: sync::Arc<LazyCapabilityPreflight>,
}

impl CapabilityValidatedModuleProducer {
    pub(crate) fn new(
        producer: sync::Arc<dyn ModuleProducer>,
        preflight: sync::Arc<LazyCapabilityPreflight>,
    ) -> Self {
        Self {
            producer,
            preflight,
        }
    }
}

impl ModuleProducer for CapabilityValidatedModuleProducer {
    fn produce(&self, module_id: ModuleId) -> Result<sync::Arc<BytecodeNode>, ModuleResolveError> {
        self.preflight.validate_module(module_id)?;
        self.producer.produce(module_id)
    }
}

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
