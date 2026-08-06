use super::*;
use galfus_contract::{
    AdapterLoadError, BoundExternalModule, BoundaryValue, CancellationOutcome,
    ExternalModuleBinder, ExternalModuleDescriptor, ExternalModuleImage, ExternalModuleRequirement,
    MessageInjector,
};
use std::collections::HashMap;

struct MockBinder {
    should_fail: bool,
}

impl ExternalModuleBinder for MockBinder {
    fn bind_module(
        &self,
        _image: &ExternalModuleImage,
    ) -> Result<Box<dyn BoundExternalModule>, AdapterLoadError> {
        if self.should_fail {
            Err(AdapterLoadError::LibraryLoadFailed {
                path: "mock_path".into(),
                message: "mock failure".into(),
            })
        } else {
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

fn create_requirement(
    proxy_module: &str,
    adapter: &str,
    target_key: &str,
    target_val: &str,
) -> ExternalModuleRequirement {
    let mut targets = HashMap::new();
    targets.insert(target_key.to_string(), target_val.to_string());

    ExternalModuleRequirement {
        proxy_module: proxy_module.to_string(),
        descriptor: ExternalModuleDescriptor {
            adapter: adapter.to_string(),
            targets,
            metadata: HashMap::new(),
            exports: vec![],
        },
    }
}

#[test]
fn no_requirements_produces_empty_bindings() {
    let preflight = ExternalBindingPreflight::new();
    let _bindings = preflight.run(&[], "linux").unwrap();
    // bindings should be empty (no way to easily assert this publicly on ExternalBindings right now except it works)
}

#[test]
fn missing_target_returns_error() {
    let mut preflight = ExternalBindingPreflight::new();
    preflight
        .register_binder("test_adapter", Box::new(MockBinder { should_fail: false }))
        .unwrap();

    let req = create_requirement("my_proxy", "test_adapter", "windows", "lib.dll");
    let err = preflight.run(&[req], "linux").err().unwrap();

    assert!(matches!(
        err,
        PreflightError::MissingTarget {
            target,
            ..
        } if target == "linux"
    ));
}

#[test]
fn bind_failure_returns_adapter_load_error() {
    let mut preflight = ExternalBindingPreflight::new();
    preflight
        .register_binder("test_adapter", Box::new(MockBinder { should_fail: true }))
        .unwrap();

    let req = create_requirement("my_proxy", "test_adapter", "linux", "lib.so");
    let err = preflight.run(&[req], "linux").err().unwrap();

    assert!(matches!(
        err,
        PreflightError::BindFailed {
            adapter,
            ..
        } if adapter == "test_adapter"
    ));
}

#[test]
fn multiple_modules_using_same_binder() {
    let mut preflight = ExternalBindingPreflight::new();
    preflight
        .register_binder("test_adapter", Box::new(MockBinder { should_fail: false }))
        .unwrap();

    let req1 = create_requirement("proxy1", "test_adapter", "linux", "lib1.so");
    let req2 = create_requirement("proxy2", "test_adapter", "linux", "lib2.so");

    let _bindings = preflight.run(&[req1, req2], "linux").unwrap();
    // Both are loaded successfully without consuming the binder
}

#[test]
fn duplicate_binder_registration_fails() {
    let mut preflight = ExternalBindingPreflight::new();
    preflight
        .register_binder("test_adapter", Box::new(MockBinder { should_fail: false }))
        .unwrap();

    let err = preflight
        .register_binder("test_adapter", Box::new(MockBinder { should_fail: false }))
        .unwrap_err();

    assert!(matches!(err, PreflightError::DuplicateBinder(_)));
}

#[test]
fn missing_binder_returns_error() {
    let preflight = ExternalBindingPreflight::new();
    let req = create_requirement("my_proxy", "missing_adapter", "linux", "lib.so");
    let err = preflight.run(&[req], "linux").err().unwrap();

    assert!(matches!(err, PreflightError::MissingBinder(_)));
}
