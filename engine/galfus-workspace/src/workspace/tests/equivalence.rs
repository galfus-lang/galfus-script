use super::compilation::io_catalog;
use super::*;
use galfus_contract::{HostProvider, MessageInjector, RuntimeCapabilities};
use galfus_runtime::Runtime;

struct RecordingIo {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
}

struct DemoAdapterSchema;

impl galfus_contract::AdapterSchema for DemoAdapterSchema {
    fn name(&self) -> &str {
        "demo"
    }

    fn catalog_schema(&self) -> String {
        "adapter demo { fn add(i32, i32): i32 }".to_string()
    }

    fn validate_schema(
        &self,
        _descriptor: &galfus_contract::AdapterModuleDescriptor,
    ) -> Result<(), galfus_contract::AdapterValidationError> {
        Ok(())
    }
}

impl HostProvider for RecordingIo {
    fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
        galfus_contract::std_io_provider_descriptor()
    }

    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        method: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        let response = match (method, args) {
            ("io_write", [SurfaceValue::Bytes(bytes)]) => {
                self.writes
                    .lock()
                    .expect("writes are available")
                    .push(bytes.clone());
                Ok(SurfaceValue::Null)
            }
            _ => Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::ProviderFailure,
                "unexpected std/io call",
            )),
        };
        let _ = injector.inject_surface_response(thread_id, request_lease, response);
        true
    }
}

fn recording_providers(writes: Arc<Mutex<Vec<Vec<u8>>>>) -> Providers {
    Providers::new().with_host("io", Box::new(RecordingIo { writes }))
}

fn equivalence_workspace() -> Workspace {
    let mut workspace = Workspace::new();
    workspace.set_catalog(io_catalog(galfus_contract::STD_IO_SOURCE));
    workspace
        .load_manifest(
            toml::from_str(
                r#"
                [module]
                name = "eager-lazy-equivalence"
                target = "app"
                [entry]
                path = "main.gfs"
                "#,
            )
            .expect("valid configuration"),
        )
        .expect("configuration loads");
    workspace
        .load_module(
            "state.gfs",
            br#"
            export const base: i32 = 40

            export fn increase(value: i32): i32 {
                return value + 2
            }
            "#,
        )
        .expect("state source loads");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { base as initial, increase } from "./state"
            import { println } from "std/io"

            export fn main(args: [[u8]]): i32 {
                println("ready")
                return increase(initial)
            }
            "#,
        )
        .expect("entry source loads");
    workspace
}

fn adapter_requirement_workspace() -> Workspace {
    let mut workspace = Workspace::new();
    workspace.set_catalog(Arc::new(
        galfus_contract::CapabilityCatalog::new(Vec::new(), vec![Arc::new(DemoAdapterSchema)])
            .expect("adapter catalog is valid"),
    ));
    workspace
        .load_manifest(
            toml::from_str(
                r#"
                [module]
                name = "eager-lazy-adapter"
                target = "app"
                [entry]
                path = "main.gfs"
                "#,
            )
            .expect("valid configuration"),
        )
        .expect("configuration loads");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { add } from "./math.gfp"

            export fn main(args: [[u8]]): i32 {
                return 0
            }
            "#,
        )
        .expect("entry source loads");
    workspace
        .load_module(
            "math.gfp",
            br#"---
adapter = "demo"
[config]
test = "memory"
---

export fn(async) add(left: i32, right: i32): i32
"#,
        )
        .expect("adapter proxy source loads");
    workspace
}

#[test]
fn eager_package_and_lazy_workspace_match_result_output_alias_global_initializer_and_async_io() {
    let mut workspace = equivalence_workspace();
    assert!(workspace.check().is_valid);
    let package = workspace.compile().expect("workspace compiles").package;

    let lazy_writes = Arc::new(Mutex::new(Vec::new()));
    let lazy_result = workspace
        .run(
            &[],
            Some(recording_providers(Arc::clone(&lazy_writes))),
            std::rc::Rc::new(CooperativeDriver::new()),
        )
        .expect("lazy workspace execution succeeds");

    let eager_writes = Arc::new(Mutex::new(Vec::new()));
    let mut eager_execution = Runtime::new(
        package,
        RuntimeCapabilities::builder()
            .with_providers(recording_providers(Arc::clone(&eager_writes)))
            .build(),
    )
    .start(&[], std::rc::Rc::new(CooperativeDriver::new()))
    .expect("eager package execution starts");
    let eager_result = eager_execution
        .run_sync_to_completion()
        .expect("eager package execution succeeds");

    assert_eq!(lazy_result, eager_result);
    assert_eq!(lazy_result, 42);
    assert_eq!(
        *lazy_writes.lock().expect("lazy writes are available"),
        *eager_writes.lock().expect("eager writes are available")
    );
    assert_eq!(
        *lazy_writes.lock().expect("lazy writes are available"),
        vec![b"ready".to_vec(), vec![b'\n']]
    );
}

#[test]
fn eager_and_lazy_report_the_same_reached_provider_requirement() {
    let mut workspace = equivalence_workspace();
    assert!(workspace.check().is_valid);
    let package = workspace.compile().expect("workspace compiles").package;

    let lazy_error = workspace
        .run(&[], None, std::rc::Rc::new(CooperativeDriver::new()))
        .expect_err("lazy execution rejects a reached provider requirement");
    assert!(matches!(
        lazy_error,
        crate::state::WorkspaceRunError::RuntimeStart(
            galfus_runtime::RuntimeError::EagerModuleResolution(
                galfus_bytecode::ModuleResolveError::ProviderRequirementUnsatisfied {
                    module_path,
                    ..
                },
            ),
        ) if module_path == "std/io"
    ));

    let eager_error = match Runtime::new(package, RuntimeCapabilities::builder().build())
        .start(&[], std::rc::Rc::new(CooperativeDriver::new()))
    {
        Ok(_) => panic!("eager execution must reject the provider requirement"),
        Err(error) => error,
    };
    assert!(matches!(
        eager_error,
        galfus_runtime::RuntimeError::ProviderRequirementUnsatisfied { module_path }
            if module_path == "std/io"
    ));
}

#[test]
fn eager_and_lazy_report_the_same_reached_adapter_requirement() {
    let mut workspace = adapter_requirement_workspace();
    assert!(workspace.check().is_valid);
    let package = workspace.compile().expect("workspace compiles").package;

    let lazy_error = workspace
        .run(&[], None, std::rc::Rc::new(CooperativeDriver::new()))
        .expect_err("lazy execution rejects a reached adapter requirement");
    assert!(matches!(
        lazy_error,
        crate::state::WorkspaceRunError::RuntimeStart(
            galfus_runtime::RuntimeError::EagerModuleResolution(
                galfus_bytecode::ModuleResolveError::AdapterRequirementUnsatisfied {
                    proxy_module,
                    ..
                },
            ),
        ) if proxy_module == "math.gfp"
    ));

    let eager_error = match Runtime::new(package, RuntimeCapabilities::builder().build())
        .start(&[], std::rc::Rc::new(CooperativeDriver::new()))
    {
        Ok(_) => panic!("eager execution must reject the adapter requirement"),
        Err(error) => error,
    };
    assert!(matches!(
        eager_error,
        galfus_runtime::RuntimeError::AdapterRequirementUnsatisfied { proxy_module }
            if proxy_module == "math.gfp"
    ));
}

#[test]
fn type_errors_are_rejected_before_either_execution_boundary() {
    let mut workspace = Workspace::new();
    workspace
        .load_manifest(
            toml::from_str(
                r#"
                [module]
                name = "eager-lazy-type-error"
                target = "app"
                [entry]
                path = "main.gfs"
                "#,
            )
            .expect("valid configuration"),
        )
        .expect("configuration loads");
    workspace
        .load_module(
            "main.gfs",
            b"export fn main(args: [[u8]]): i32 { return \"not-an-integer\" }",
        )
        .expect("entry source loads");

    let report = workspace.check();
    assert!(
        !report.is_valid,
        "type errors must block execution boundaries"
    );
    assert!(workspace.compile().is_err());
    assert!(matches!(
        workspace.run(&[], None, std::rc::Rc::new(CooperativeDriver::new())),
        Err(crate::state::WorkspaceRunError::Blocked(
            crate::state::RunBlocked::CheckRequired
        ))
    ));
}

#[test]
fn eager_and_lazy_report_the_same_initialization_cycle() {
    let mut workspace = Workspace::new();
    workspace
        .load_manifest(
            toml::from_str(
                r#"
                [module]
                name = "eager-lazy-cycle"
                target = "app"
                [entry]
                path = "main.gfs"
                "#,
            )
            .expect("valid configuration"),
        )
        .expect("configuration loads");
    workspace
        .load_module(
            "main.gfs",
            b"import { value } from \"./first\"\nexport fn main(args: [[u8]]): i32 { return value() }",
        )
        .expect("entry source loads");
    workspace
        .load_module(
            "first.gfs",
            b"import { helper } from \"./second\"\nexport fn value(): i32 { return helper() }",
        )
        .expect("first source loads");
    workspace
        .load_module(
            "second.gfs",
            b"import { value } from \"./first\"\nexport fn helper(): i32 { return value() }",
        )
        .expect("second source loads");

    assert!(workspace.check().is_valid);
    let package = workspace.compile().expect("workspace compiles").package;

    let lazy_error =
        match workspace.start_execution(&[], None, std::rc::Rc::new(CooperativeDriver::new())) {
            Ok(_) => panic!("lazy execution must reject the initialization cycle"),
            Err(error) => error,
        };
    assert!(matches!(
        lazy_error,
        crate::state::WorkspaceRunError::RuntimeStart(
            galfus_runtime::RuntimeError::InitializationDependencyCycle { .. }
        )
    ));

    let eager_error = match Runtime::new(package, RuntimeCapabilities::builder().build())
        .start(&[], std::rc::Rc::new(CooperativeDriver::new()))
    {
        Ok(_) => panic!("eager execution must reject the initialization cycle"),
        Err(error) => error,
    };
    assert!(matches!(
        eager_error,
        galfus_runtime::RuntimeError::InitializationDependencyCycle { .. }
    ));
}
