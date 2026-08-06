use galfus_contract::{
    AdapterLoadError, ExternalBindings, ExternalModuleBinder, ExternalModuleImage,
    ExternalModuleRequirement,
};
use std::collections::HashMap;

#[derive(Debug)]
pub enum PreflightError {
    MissingBinder(String),
    MissingTarget {
        proxy_module: String,
        adapter: String,
        target: String,
    },
    BindFailed {
        proxy_module: String,
        adapter: String,
        error: AdapterLoadError,
    },
    DuplicateBinder(String),
}

impl std::fmt::Display for PreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBinder(adapter) => {
                write!(f, "Missing required binder for adapter: {}", adapter)
            }
            Self::MissingTarget {
                proxy_module,
                adapter,
                target,
            } => write!(
                f,
                "Missing target platform '{}' for module {} with adapter {}",
                target, proxy_module, adapter
            ),
            Self::BindFailed {
                proxy_module,
                adapter,
                error,
            } => write!(
                f,
                "Failed to bind module {} using adapter {}: {}",
                proxy_module, adapter, error
            ),
            Self::DuplicateBinder(adapter) => {
                write!(f, "Duplicate binder registered for adapter: {}", adapter)
            }
        }
    }
}

impl std::error::Error for PreflightError {}

pub struct ExternalBindingPreflight {
    binders: HashMap<String, Box<dyn ExternalModuleBinder>>,
}

impl Default for ExternalBindingPreflight {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalBindingPreflight {
    pub fn new() -> Self {
        Self {
            binders: HashMap::new(),
        }
    }

    pub fn register_binder(
        &mut self,
        adapter_name: impl Into<String>,
        binder: Box<dyn ExternalModuleBinder>,
    ) -> Result<(), PreflightError> {
        let name = adapter_name.into();
        if self.binders.contains_key(&name) {
            return Err(PreflightError::DuplicateBinder(name));
        }
        self.binders.insert(name, binder);
        Ok(())
    }

    pub fn run(
        &self,
        requirements: &[ExternalModuleRequirement],
        platform_target: &str,
    ) -> Result<ExternalBindings, PreflightError> {
        let mut bindings = ExternalBindings::default();

        if requirements.is_empty() {
            return Ok(bindings);
        }

        for requirement in requirements {
            let adapter_name = &requirement.descriptor.adapter;
            let binder = self
                .binders
                .get(adapter_name)
                .ok_or_else(|| PreflightError::MissingBinder(adapter_name.clone()))?;

            let artifact = requirement
                .descriptor
                .targets
                .get(platform_target)
                .ok_or_else(|| PreflightError::MissingTarget {
                    proxy_module: requirement.proxy_module.clone(),
                    adapter: adapter_name.clone(),
                    target: platform_target.to_string(),
                })?;

            let image = ExternalModuleImage {
                requirement: requirement.clone(),
                artifact: artifact.clone(),
            };

            let bound_module =
                binder
                    .bind_module(&image)
                    .map_err(|error| PreflightError::BindFailed {
                        proxy_module: requirement.proxy_module.clone(),
                        adapter: adapter_name.clone(),
                        error,
                    })?;

            bindings.register_module(requirement.proxy_module.clone(), bound_module);
        }

        Ok(bindings)
    }
}

#[cfg(test)]
mod tests;
