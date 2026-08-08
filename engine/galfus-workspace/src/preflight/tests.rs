use super::*;
use galfus_bytecode::{BytecodeGraph, BytecodeModule, BytecodeNode, ConstantPool, PackageImage};
use galfus_contract::{
    AdapterConfigValue, AdapterLoadContext, AdapterLoadError, AdapterModuleBinding,
    AdapterModuleDescriptor, AdapterModuleLoader, AdapterModuleRequirement, BoundaryValue,
    CancellationOutcome, MessageInjector,
};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};
use std::collections::BTreeMap;

struct MockLoader {
    should_fail: bool,
}

impl AdapterModuleLoader for MockLoader {
    fn load_module(
        &self,
        requirement: &AdapterModuleRequirement,
        context: &AdapterLoadContext,
    ) -> Result<Box<dyn AdapterModuleBinding>, AdapterLoadError> {
        if self.should_fail {
            Err(AdapterLoadError {
                code: "unsupported_platform".into(),
                message: "mock failure".into(),
            })
        } else {
            // loader asserts that the context is passed
            assert!(context.properties.contains_key("os"));

            // assert that config was passed completely
            assert!(requirement.descriptor.config.contains_key("test_key"));
            Ok(Box::new(MockBoundModule))
        }
    }
}

struct MockBoundModule;
impl AdapterModuleBinding for MockBoundModule {
    fn dispatch(
        &mut self,
        _symbol: &str,
        _thread_id: usize,
        _request_id: u64,
        _args: &[BoundaryValue],
        _injector: std::sync::Arc<dyn MessageInjector>,
    ) {
    }

    fn cancel(
        &mut self,
        _symbol: &str,
        _thread_id: usize,
        _request_id: u64,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}

fn create_requirement(proxy_module: &str, adapter: &str) -> AdapterModuleRequirement {
    let mut config = BTreeMap::new();
    config.insert(
        "test_key".to_string(),
        AdapterConfigValue::String("test_val".to_string()),
    );

    AdapterModuleRequirement {
        proxy_module: format!("{proxy_module}.gfp"),
        descriptor: AdapterModuleDescriptor {
            adapter: adapter.to_string(),
            config,
            exports: vec![],
        },
    }
}

fn create_context() -> AdapterLoadContext {
    let mut properties = BTreeMap::new();
    properties.insert("os".to_string(), "linux".to_string());
    AdapterLoadContext { properties }
}

fn create_package(requirements: Vec<AdapterModuleRequirement>) -> PackageImage {
    let modules = requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| BytecodeNode {
            id: ModuleId::new(index as u32 + 1),
            path: ModulePath::new(requirement.proxy_module.as_str())
                .expect("valid proxy module path"),
            semantic_revision: SemanticRevision::new(1),
            module: BytecodeModule {
                name: requirement.proxy_module.clone(),
                global_count: 0,
                constants: ConstantPool::default(),
                functions: Vec::new(),
                types: Vec::new(),
                struct_layouts: Vec::new(),
                choice_layouts: Vec::new(),
                imports: Vec::new(),
                exports: Vec::new(),
                init_func_idx: None,
            },
            metadata: None,
        })
        .collect();
    let graph = BytecodeGraph::from_modules(SemanticRevision::new(1), modules, Vec::new())
        .expect("valid proxy graph");

    PackageImage::try_new(graph, None, requirements).expect("complete adapter manifest")
}

#[test]
fn no_requirements_produces_empty_bindings() {
    let preflight = AdapterBindingPreflight::new();
    let package = create_package(Vec::new());
    let _bindings = preflight.bind_package(&package, &create_context()).unwrap();
}

#[test]
fn missing_loader_returns_error() {
    let preflight = AdapterBindingPreflight::new();
    let req = create_requirement("my_proxy", "missing_adapter");
    let package = create_package(vec![req]);
    let err = preflight
        .bind_package(&package, &create_context())
        .err()
        .unwrap();

    assert!(matches!(err, PreflightError::MissingLoader(_)));
}

#[test]
fn load_failure_returns_adapter_load_error() {
    let mut preflight = AdapterBindingPreflight::new();
    preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: true }))
        .unwrap();

    let req = create_requirement("my_proxy", "test_adapter");
    let package = create_package(vec![req]);
    let err = preflight
        .bind_package(&package, &create_context())
        .err()
        .unwrap();

    assert!(matches!(
        err,
        PreflightError::LoadFailed {
            adapter,
            ..
        } if adapter == "test_adapter"
    ));
}

#[test]
fn multiple_modules_using_same_loader() {
    let mut preflight = AdapterBindingPreflight::new();
    preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: false }))
        .unwrap();

    let req1 = create_requirement("proxy1", "test_adapter");
    let req2 = create_requirement("proxy2", "test_adapter");

    let package = create_package(vec![req1, req2]);
    let mut bindings = preflight.bind_package(&package, &create_context()).unwrap();

    // confirm bindings are correctly populated
    assert!(bindings.get_mut("proxy1.gfp").is_some());
    assert!(bindings.get_mut("proxy2.gfp").is_some());
}

#[test]
fn duplicate_loader_registration_fails() {
    let mut preflight = AdapterBindingPreflight::new();
    preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: false }))
        .unwrap();

    let err = preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: false }))
        .unwrap_err();

    assert!(matches!(err, PreflightError::DuplicateLoader(_)));
}

#[test]
fn two_loaders_with_structurally_different_configurations() {
    let mut preflight = AdapterBindingPreflight::new();
    preflight
        .register_loader("loader1", Box::new(MockLoader { should_fail: false }))
        .unwrap();
    preflight
        .register_loader("loader2", Box::new(MockLoader { should_fail: false }))
        .unwrap();

    let mut config1 = BTreeMap::new();
    config1.insert(
        "test_key".to_string(),
        AdapterConfigValue::String("val1".to_string()),
    );
    let req1 = AdapterModuleRequirement {
        proxy_module: "proxy1.gfp".to_string(),
        descriptor: AdapterModuleDescriptor {
            adapter: "loader1".to_string(),
            config: config1,
            exports: vec![],
        },
    };

    let mut config2 = BTreeMap::new();
    let mut nested = BTreeMap::new();
    nested.insert("inner_key".to_string(), AdapterConfigValue::Integer(42));
    config2.insert("test_key".to_string(), AdapterConfigValue::Table(nested));
    let req2 = AdapterModuleRequirement {
        proxy_module: "proxy2.gfp".to_string(),
        descriptor: AdapterModuleDescriptor {
            adapter: "loader2".to_string(),
            config: config2,
            exports: vec![],
        },
    };

    let package = create_package(vec![req1, req2]);
    let mut bindings = preflight.bind_package(&package, &create_context()).unwrap();
    assert!(bindings.get_mut("proxy1.gfp").is_some());
    assert!(bindings.get_mut("proxy2.gfp").is_some());
}
