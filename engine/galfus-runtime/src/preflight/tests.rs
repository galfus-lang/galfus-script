use super::LazyCapabilityPreflight;
use galfus_bytecode::{ModuleCatalog, ModuleDescriptor};
use galfus_contract::{
    AdapterBindings, CURRENT_BOUNDARY_ABI_VERSION, ContentHash, HostProvider, ProviderDescriptor,
    ProviderModuleDescriptor, ProviderModuleRequirement, Providers,
};
use galfus_core::{ModuleId, ModulePath};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct CountingProvider {
    descriptor_calls: Arc<AtomicUsize>,
    descriptor: ProviderDescriptor,
}

impl HostProvider for CountingProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        self.descriptor_calls.fetch_add(1, Ordering::Relaxed);
        self.descriptor.clone()
    }
}

fn descriptor(
    module_id: ModuleId,
    path: &str,
    provider_modules: Vec<ModuleId>,
) -> ModuleDescriptor {
    ModuleDescriptor::new_with_capability_requirements(
        module_id,
        ModulePath::new(path).expect("valid module path"),
        Vec::new(),
        provider_modules,
        Vec::new(),
        Vec::new(),
        false,
        ContentHash::of(&[]),
        ContentHash::of(&[]),
    )
    .expect("valid descriptor")
}

#[test]
fn provider_binding_is_cached_across_modules() {
    let provider_module = ModuleId::new(1);
    let catalog = ModuleCatalog::new(vec![
        descriptor(provider_module, "std/io.gfs", Vec::new()),
        descriptor(ModuleId::new(2), "first.gfs", vec![provider_module]),
        descriptor(ModuleId::new(3), "second.gfs", vec![provider_module]),
    ])
    .expect("valid catalog");
    let requirement = ProviderModuleRequirement {
        alias: "io".to_string(),
        module_path: "std/io".to_string(),
        schema_fingerprint: 1,
        boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
        exports: Vec::new(),
    };
    let descriptor = ProviderDescriptor {
        modules: vec![ProviderModuleDescriptor {
            module_path: requirement.module_path.clone(),
            schema_fingerprint: requirement.schema_fingerprint,
            boundary_abi: requirement.boundary_abi,
            exports: requirement.exports.clone(),
            surface_contracts: Vec::new(),
        }],
    };
    let descriptor_calls = Arc::new(AtomicUsize::new(0));
    let providers = Providers::new().with_host(
        "io",
        Box::new(CountingProvider {
            descriptor_calls: Arc::clone(&descriptor_calls),
            descriptor,
        }),
    );
    let preflight = LazyCapabilityPreflight::new(
        &catalog,
        &[],
        &[requirement],
        Some(Arc::new(Mutex::new(providers))),
        Arc::new(Mutex::new(AdapterBindings::default())),
    )
    .expect("capability declarations are valid");

    preflight
        .validate_module(ModuleId::new(2))
        .expect("first use binds provider");
    preflight
        .validate_module(ModuleId::new(3))
        .expect("second use reuses provider binding");

    assert_eq!(descriptor_calls.load(Ordering::Relaxed), 1);
}
