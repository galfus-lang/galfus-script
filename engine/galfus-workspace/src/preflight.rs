use galfus_bytecode::PackageImage;
use galfus_contract::{
    AdapterBindings, AdapterLoadContext, AdapterLoadError, AdapterModuleLoader,
    AdapterModuleRequirement,
};
use std::collections::HashMap;

#[derive(Debug)]
pub enum PreflightError {
    MissingLoader(String),
    LoadFailed {
        proxy_module: String,
        adapter: String,
        error: AdapterLoadError,
    },
    DuplicateLoader(String),
}

impl std::fmt::Display for PreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLoader(adapter) => {
                write!(f, "Missing required loader for adapter: {}", adapter)
            }
            Self::LoadFailed {
                proxy_module,
                adapter,
                error,
            } => write!(
                f,
                "Failed to load module {} using adapter {}: {}",
                proxy_module, adapter, error
            ),
            Self::DuplicateLoader(adapter) => {
                write!(f, "Duplicate loader registered for adapter: {}", adapter)
            }
        }
    }
}

impl std::error::Error for PreflightError {}

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

    /// Binds every external module declared by one immutable package image.
    pub fn bind_package(
        &self,
        package: &PackageImage,
        context: &AdapterLoadContext,
    ) -> Result<AdapterBindings, PreflightError> {
        self.bind_requirements(package.adapter_requirements(), context)
    }

    fn bind_requirements(
        &self,
        requirements: &[AdapterModuleRequirement],
        context: &AdapterLoadContext,
    ) -> Result<AdapterBindings, PreflightError> {
        let mut bindings = AdapterBindings::default();

        if requirements.is_empty() {
            return Ok(bindings);
        }

        for requirement in requirements {
            let adapter_name = &requirement.descriptor.adapter;
            let loader = self
                .loaders
                .get(adapter_name)
                .ok_or_else(|| PreflightError::MissingLoader(adapter_name.clone()))?;

            let bound_module = loader.load_module(requirement, context).map_err(|error| {
                PreflightError::LoadFailed {
                    proxy_module: requirement.proxy_module.clone(),
                    adapter: adapter_name.clone(),
                    error,
                }
            })?;

            bindings.register_module(requirement.proxy_module.clone(), bound_module);
        }

        Ok(bindings)
    }
}

#[cfg(test)]
mod tests;
