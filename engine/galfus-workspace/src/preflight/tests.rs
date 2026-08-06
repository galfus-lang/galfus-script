use super::*;
use galfus_contract::{
    AdapterConfigValue, AdapterLoadError, BoundExternalModule, BoundaryValue, CancellationOutcome,
    ExternalLoadContext, ExternalModuleDescriptor, ExternalModuleLoader, ExternalModuleRequirement,
    MessageInjector,
};
use std::collections::BTreeMap;

struct MockLoader {
    should_fail: bool,
}

impl ExternalModuleLoader for MockLoader {
    fn load_module(
        &self,
        _requirement: &ExternalModuleRequirement,
        context: &ExternalLoadContext,
    ) -> Result<Box<dyn BoundExternalModule>, AdapterLoadError> {
        if self.should_fail {
            Err(AdapterLoadError {
                code: "unsupported_platform".into(),
                message: "mock failure".into(),
            })
        } else {
            // loader asserts that the context is passed
            assert!(context.properties.contains_key("os"));
            Ok(Box::new(MockBoundModule))
        }
    }
}

struct MockBoundModule;
impl BoundExternalModule for MockBoundModule {
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

fn create_requirement(proxy_module: &str, adapter: &str) -> ExternalModuleRequirement {
    let mut config = BTreeMap::new();
    config.insert(
        "test_key".to_string(),
        AdapterConfigValue::String("test_val".to_string()),
    );

    ExternalModuleRequirement {
        proxy_module: proxy_module.to_string(),
        descriptor: ExternalModuleDescriptor {
            adapter: adapter.to_string(),
            config,
            exports: vec![],
        },
    }
}

fn create_context() -> ExternalLoadContext {
    let mut properties = BTreeMap::new();
    properties.insert("os".to_string(), "linux".to_string());
    ExternalLoadContext { properties }
}

#[test]
fn no_requirements_produces_empty_bindings() {
    let preflight = ExternalBindingPreflight::new();
    let _bindings = preflight.run(&[], &create_context()).unwrap();
}

#[test]
fn missing_loader_returns_error() {
    let preflight = ExternalBindingPreflight::new();
    let req = create_requirement("my_proxy", "missing_adapter");
    let err = preflight.run(&[req], &create_context()).err().unwrap();

    assert!(matches!(err, PreflightError::MissingLoader(_)));
}

#[test]
fn load_failure_returns_adapter_load_error() {
    let mut preflight = ExternalBindingPreflight::new();
    preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: true }))
        .unwrap();

    let req = create_requirement("my_proxy", "test_adapter");
    let err = preflight.run(&[req], &create_context()).err().unwrap();

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
    let mut preflight = ExternalBindingPreflight::new();
    preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: false }))
        .unwrap();

    let req1 = create_requirement("proxy1", "test_adapter");
    let req2 = create_requirement("proxy2", "test_adapter");

    let _bindings = preflight.run(&[req1, req2], &create_context()).unwrap();
}

#[test]
fn duplicate_loader_registration_fails() {
    let mut preflight = ExternalBindingPreflight::new();
    preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: false }))
        .unwrap();

    let err = preflight
        .register_loader("test_adapter", Box::new(MockLoader { should_fail: false }))
        .unwrap_err();

    assert!(matches!(err, PreflightError::DuplicateLoader(_)));
}
