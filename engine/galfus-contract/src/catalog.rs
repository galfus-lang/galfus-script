use crate::ExternalAdapterSchema;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// A declarative catalog of capabilities authorized for the workspace.
/// This includes the paths of modules allowed to declare providers and the
/// schemas of adapters allowed to be bound to external proxies.
#[derive(Clone)]
pub struct CapabilityCatalog {
    provider_modules: HashSet<String>,
    adapter_schemas: HashMap<String, Arc<dyn ExternalAdapterSchema>>,
    fingerprint: u64,
}

impl Default for CapabilityCatalog {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new())
    }
}

impl CapabilityCatalog {
    pub fn new(
        provider_modules: Vec<String>,
        adapters: Vec<Arc<dyn ExternalAdapterSchema>>,
    ) -> Self {
        let mut adapter_schemas = HashMap::new();
        let mut adapter_names = Vec::new();
        for adapter in adapters {
            let name = adapter.name().to_string();
            adapter_names.push(name.clone());
            adapter_schemas.insert(name, adapter);
        }

        let provider_modules: HashSet<_> = provider_modules.into_iter().collect();

        // Compute a deterministic fingerprint based on the semantic content of the catalog.
        // We hash the sorted names of providers and adapters.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        let mut sorted_providers: Vec<_> = provider_modules.iter().collect();
        sorted_providers.sort_unstable();
        sorted_providers.hash(&mut hasher);

        adapter_names.sort_unstable();
        adapter_names.hash(&mut hasher);

        let fingerprint = hasher.finish();

        Self {
            provider_modules,
            adapter_schemas,
            fingerprint,
        }
    }

    /// Returns the deterministic fingerprint of this catalog's declarative contents.
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Checks if a module is authorized to declare provider functions.
    pub fn is_provider_module(&self, path: &str) -> bool {
        self.provider_modules.contains(path)
    }

    /// Retrieves the declarative schema for an adapter, if registered.
    pub fn adapter_schema(&self, name: &str) -> Option<Arc<dyn ExternalAdapterSchema>> {
        self.adapter_schemas.get(name).cloned()
    }

    pub fn has_adapter(&self, name: &str) -> bool {
        self.adapter_schemas.contains_key(name)
    }
}
