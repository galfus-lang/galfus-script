use crate::builtins::BridgeModule;
use crate::{AdapterSchema, is_builtin_module};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[cfg(test)]
mod tests;

const FINGERPRINT_FORMAT_VERSION: u8 = 1;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CapabilityCatalogError {
    #[error("provider module path '{0}' is invalid or non-canonical")]
    InvalidProviderPath(String),
    #[error("provider module path '{0}' conflicts with an internal builtin module")]
    ProviderBuiltinConflict(String),
    #[error("duplicate provider module path '{0}'")]
    DuplicateProviderPath(String),
    #[error("duplicate adapter schema '{0}'")]
    DuplicateAdapterSchema(String),
}

/// A declarative catalog of capability module sources and external adapter schemas.
#[derive(Clone)]
pub struct CapabilityCatalog {
    provider_modules: HashMap<String, String>,
    adapter_schemas: HashMap<String, Arc<dyn AdapterSchema>>,
    fingerprint: u64,
}

impl Default for CapabilityCatalog {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new()).expect("the empty catalog is valid")
    }
}

impl CapabilityCatalog {
    pub fn new(
        provider_modules: Vec<BridgeModule>,
        adapters: Vec<Arc<dyn AdapterSchema>>,
    ) -> Result<Self, CapabilityCatalogError> {
        let mut provider_modules_by_path = HashMap::new();
        for provider in provider_modules {
            Self::validate_provider_path(&provider.name)?;
            if is_builtin_module(&provider.name) {
                return Err(CapabilityCatalogError::ProviderBuiltinConflict(
                    provider.name,
                ));
            }
            if provider_modules_by_path
                .insert(provider.name.clone(), provider.source)
                .is_some()
            {
                return Err(CapabilityCatalogError::DuplicateProviderPath(provider.name));
            }
        }

        let mut adapter_schemas = HashMap::new();
        for adapter in adapters {
            let name = adapter.name().to_string();
            if adapter_schemas.insert(name.clone(), adapter).is_some() {
                return Err(CapabilityCatalogError::DuplicateAdapterSchema(name));
            }
        }

        let fingerprint = Self::fingerprint_for(&provider_modules_by_path, &adapter_schemas);
        Ok(Self {
            provider_modules: provider_modules_by_path,
            adapter_schemas,
            fingerprint,
        })
    }

    fn validate_provider_path(path: &str) -> Result<(), CapabilityCatalogError> {
        if !path.is_empty()
            && !path.contains('\0')
            && !path.contains('\\')
            && path
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        {
            Ok(())
        } else {
            Err(CapabilityCatalogError::InvalidProviderPath(
                path.to_string(),
            ))
        }
    }

    fn fingerprint_for(
        provider_modules: &HashMap<String, String>,
        adapter_schemas: &HashMap<String, Arc<dyn AdapterSchema>>,
    ) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        FINGERPRINT_FORMAT_VERSION.hash(&mut hasher);

        let mut providers = provider_modules.iter().collect::<Vec<_>>();
        providers.sort_unstable_by_key(|(path, _)| *path);
        for (path, source) in providers {
            path.hash(&mut hasher);
            source.hash(&mut hasher);
        }

        let mut adapters = adapter_schemas.iter().collect::<Vec<_>>();
        adapters.sort_unstable_by_key(|(name, _)| *name);
        for (name, adapter) in adapters {
            name.hash(&mut hasher);
            adapter.catalog_schema().hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Returns the deterministic fingerprint of this catalog's declarative contents.
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Checks whether a path is an authorized provider module.
    pub fn is_provider_module(&self, path: &str) -> bool {
        self.provider_modules.contains_key(path)
    }

    /// Retrieves an authorized provider module's declarative `.gfs` source.
    pub fn provider_source(&self, path: &str) -> Option<&str> {
        self.provider_modules.get(path).map(String::as_str)
    }

    pub fn provider_schema_fingerprint(&self, path: &str) -> Option<u64> {
        self.provider_source(path).map(provider_schema_fingerprint)
    }

    /// Retrieves the declarative schema for an adapter, if registered.
    pub fn adapter_schema(&self, name: &str) -> Option<Arc<dyn AdapterSchema>> {
        self.adapter_schemas.get(name).cloned()
    }

    pub fn has_adapter(&self, name: &str) -> bool {
        self.adapter_schemas.contains_key(name)
    }
}

pub fn provider_schema_fingerprint(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}
