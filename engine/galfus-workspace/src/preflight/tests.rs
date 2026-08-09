use super::*;
use galfus_bytecode::{BytecodeGraph, BytecodeModule, BytecodeNode, ConstantPool, PackageImage};
use galfus_contract::{
    AdapterArtifact, AdapterConfigValue, AdapterLoadContext, AdapterLoadError,
    AdapterModuleBinding, AdapterModuleDescriptor, AdapterModuleLoader, AdapterModuleRequirement,
    AdapterTarget, BoundaryValue, CancellationOutcome, ContentHash, MessageInjector,
    SelectedAdapterTarget, VerifiedAdapterArtifact,
};
use galfus_contract::{CURRENT_BOUNDARY_ABI_VERSION, ExecutionTarget};
use galfus_core::Version;
use galfus_core::{ModuleId, ModulePath, SemanticRevision};
use std::collections::BTreeMap;

struct MockLoader {
    should_fail: bool,
    corrupt_artifact: bool,
}

impl AdapterModuleLoader for MockLoader {
    fn load_artifact(
        &self,
        _selected_target: &SelectedAdapterTarget,
        _context: &AdapterLoadContext,
    ) -> Result<Vec<u8>, AdapterLoadError> {
        if self.should_fail {
            Err(AdapterLoadError {
                code: "unsupported_platform".into(),
                message: "mock failure".into(),
            })
        } else {
            Ok(if self.corrupt_artifact {
                b"corrupt artifact".to_vec()
            } else {
                b"mock artifact".to_vec()
            })
        }
    }

    fn load_module(
        &self,
        requirement: &AdapterModuleRequirement,
        _selected_target: &SelectedAdapterTarget,
        artifact: VerifiedAdapterArtifact,
        context: &AdapterLoadContext,
    ) -> Result<Box<dyn AdapterModuleBinding>, AdapterLoadError> {
        // loader asserts that the context is passed
        assert!(context.properties.contains_key("os"));
        assert_eq!(artifact.as_bytes(), b"mock artifact");

        // assert that config was passed completely
        assert!(requirement.descriptor.config.contains_key("test_key"));
        Ok(Box::new(MockBoundModule(requirement.descriptor.clone())))
    }
}

struct MockBoundModule(AdapterModuleDescriptor);
impl AdapterModuleBinding for MockBoundModule {
    fn descriptor(&self) -> AdapterModuleDescriptor {
        self.0.clone()
    }

    fn dispatch(
        &mut self,
        _symbol: &str,
        _thread_id: galfus_core::ThreadId,
        _request_id: galfus_core::RequestId,
        _args: &[BoundaryValue],
        _injector: std::sync::Arc<dyn MessageInjector>,
    ) {
    }

    fn cancel(
        &mut self,
        _symbol: &str,
        _thread_id: galfus_core::ThreadId,
        _request_id: galfus_core::RequestId,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}

struct DescriptorMismatchLoader;

impl AdapterModuleLoader for DescriptorMismatchLoader {
    fn load_artifact(
        &self,
        _selected_target: &SelectedAdapterTarget,
        _context: &AdapterLoadContext,
    ) -> Result<Vec<u8>, AdapterLoadError> {
        Ok(b"mock artifact".to_vec())
    }

    fn load_module(
        &self,
        _requirement: &AdapterModuleRequirement,
        _selected_target: &SelectedAdapterTarget,
        _artifact: VerifiedAdapterArtifact,
        _context: &AdapterLoadContext,
    ) -> Result<Box<dyn AdapterModuleBinding>, AdapterLoadError> {
        Ok(Box::new(MockBoundModule(AdapterModuleDescriptor::empty())))
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
            targets: vec![mock_target()],
            exports: vec![],
        },
        boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
    }
}

fn mock_target() -> AdapterTarget {
    AdapterTarget {
        target: ExecutionTarget::new("test").expect("valid target"),
        locator: "memory://mock".to_string(),
        platform: "test".to_string(),
        abi: "1".to_string(),
        artifact: AdapterArtifact {
            content_hash: ContentHash::of(b"mock artifact"),
            size_bytes: b"mock artifact".len() as u64,
            media_type: "application/x-galfus-test".to_string(),
            content_version: Version::new(1, 0, 0),
        },
    }
}

fn create_context() -> AdapterLoadContext {
    let mut properties = BTreeMap::new();
    properties.insert("os".to_string(), "linux".to_string());
    AdapterLoadContext {
        target: ExecutionTarget::new("test").expect("valid target"),
        properties,
    }
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

    PackageImage::try_new(
        graph,
        ExecutionTarget::new("test").expect("valid target"),
        None,
        requirements,
        Vec::new(),
    )
    .expect("complete adapter manifest")
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
        .register_loader(
            "test_adapter",
            Box::new(MockLoader {
                should_fail: true,
                corrupt_artifact: false,
            }),
        )
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
        .register_loader(
            "test_adapter",
            Box::new(MockLoader {
                should_fail: false,
                corrupt_artifact: false,
            }),
        )
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
        .register_loader(
            "test_adapter",
            Box::new(MockLoader {
                should_fail: false,
                corrupt_artifact: false,
            }),
        )
        .unwrap();

    let err = preflight
        .register_loader(
            "test_adapter",
            Box::new(MockLoader {
                should_fail: false,
                corrupt_artifact: false,
            }),
        )
        .unwrap_err();

    assert!(matches!(err, PreflightError::DuplicateLoader(_)));
}

#[test]
fn two_loaders_with_structurally_different_configurations() {
    let mut preflight = AdapterBindingPreflight::new();
    preflight
        .register_loader(
            "loader1",
            Box::new(MockLoader {
                should_fail: false,
                corrupt_artifact: false,
            }),
        )
        .unwrap();
    preflight
        .register_loader(
            "loader2",
            Box::new(MockLoader {
                should_fail: false,
                corrupt_artifact: false,
            }),
        )
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
            targets: vec![mock_target()],
            exports: vec![],
        },
        boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
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
            targets: vec![mock_target()],
            exports: vec![],
        },
        boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
    };

    let package = create_package(vec![req1, req2]);
    let mut bindings = preflight.bind_package(&package, &create_context()).unwrap();
    assert!(bindings.get_mut("proxy1.gfp").is_some());
    assert!(bindings.get_mut("proxy2.gfp").is_some());
}

#[test]
fn preflight_rejects_an_adapter_artifact_with_wrong_content() {
    let mut preflight = AdapterBindingPreflight::new();
    preflight
        .register_loader(
            "test_adapter",
            Box::new(MockLoader {
                should_fail: false,
                corrupt_artifact: true,
            }),
        )
        .unwrap();

    let package = create_package(vec![create_requirement("proxy", "test_adapter")]);
    let error = preflight
        .bind_package(&package, &create_context())
        .err()
        .expect("corrupt artifact must not bind");

    assert!(matches!(
        error,
        PreflightError::ArtifactIntegrityFailed { .. }
    ));
}

#[test]
fn preflight_rejects_a_binding_with_a_different_descriptor() {
    let mut preflight = AdapterBindingPreflight::new();
    preflight
        .register_loader("test-adapter", Box::new(DescriptorMismatchLoader))
        .unwrap();
    let package = create_package(vec![create_requirement("proxy", "test-adapter")]);

    assert!(matches!(
        preflight.bind_package(&package, &create_context()),
        Err(PreflightError::DescriptorMismatch { proxy_module }) if proxy_module == "proxy.gfp"
    ));
}
