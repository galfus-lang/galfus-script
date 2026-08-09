use super::*;
use galfus_core::{HandleId, OpaqueTypeId};
use std::rc::Rc;
use std::sync::Arc;

struct DummyHost;

struct IoHost;

struct DummyAdapter(Arc<std::sync::atomic::AtomicUsize>);

impl AdapterModuleBinding for DummyAdapter {
    fn descriptor(&self) -> AdapterModuleDescriptor {
        AdapterModuleDescriptor::empty()
    }

    fn dispatch(
        &mut self,
        _symbol: &str,
        _thread_id: usize,
        _request_id: u64,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
    }

    fn release_handle(&mut self, _type_id: &OpaqueTypeId, _id: HandleId) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
}

impl HostProvider for DummyHost {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor::default()
    }

    fn dispatch(
        &mut self,
        _thread_id: usize,
        _request_id: u64,
        _method: &str,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
        // dummy
    }
}

impl HostProvider for IoHost {
    fn descriptor(&self) -> ProviderDescriptor {
        std_io_provider_descriptor()
    }

    fn dispatch(
        &mut self,
        _thread_id: usize,
        _request_id: u64,
        _method: &str,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
    }
}

#[test]
fn providers_allow_execution_without_host() {
    assert!(Providers::new().host_mut().is_none());
}

#[test]
fn providers_allow_host() {
    let mut providers = Providers::with_host(Box::new(DummyHost));
    assert!(providers.host_mut().is_some());
}

#[test]
fn providers_default_to_main_thread_affinity() {
    assert_eq!(DummyHost.affinity("operation"), TaskAffinity::Main);
}

#[test]
fn provider_descriptors_validate_the_complete_compiled_requirement() {
    let descriptor = std_io_provider_descriptor();
    let module = descriptor.modules.first().unwrap();
    let requirement = ProviderModuleRequirement {
        module_path: module.module_path.clone(),
        schema_fingerprint: module.schema_fingerprint,
        boundary_abi: module.boundary_abi,
        exports: module.exports.clone(),
    };

    assert!(Providers::with_host(Box::new(IoHost)).validates(&requirement));
    assert!(!Providers::with_host(Box::new(DummyHost)).validates(&requirement));
}

#[test]
fn adapter_bindings_are_registered_by_nominal_proxy_module() {
    let mut bindings = AdapterBindings::default();
    bindings
        .register_module(
            "graphics",
            Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )))),
        )
        .expect("adapter binding registers");
    assert!(bindings.get_mut("graphics").is_some());
    assert!(bindings.get_mut("missing").is_none());
}

#[test]
fn adapter_bindings_reject_duplicate_proxy_modules() {
    let mut builder = RuntimeCapabilities::builder();
    builder
        .register_adapter(
            "graphics",
            Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )))),
        )
        .expect("initial adapter binding registers");

    let result = builder.register_adapter(
        "graphics",
        Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
            0,
        )))),
    );

    assert_eq!(
        result,
        Err(AdapterBindingError::DuplicateProxyModule(
            "graphics".to_string()
        ))
    );
}

#[test]
fn adapter_bindings_own_and_release_nominal_handles() {
    let releases = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut bindings = AdapterBindings::default();
    let binding_id = bindings
        .register_module("graphics", Box::new(DummyAdapter(releases.clone())))
        .expect("adapter binding registers");
    let type_id = OpaqueTypeId::new("graphics", "Texture").unwrap();
    let handle_id = HandleId::new(1);
    assert!(bindings.register_handle(binding_id, type_id.clone(), handle_id));
    assert!(bindings.contains_handle(binding_id, &type_id, handle_id));
    assert!(bindings.release_handle(binding_id, &type_id, handle_id));
    assert!(!bindings.contains_handle(binding_id, &type_id, handle_id));
    assert!(!bindings.release_handle(binding_id, &type_id, handle_id));
    assert_eq!(releases.load(std::sync::atomic::Ordering::Acquire), 1);
}

#[test]
fn adapter_handle_batches_are_registered_atomically() {
    let mut bindings = AdapterBindings::default();
    let binding_id = bindings
        .register_module(
            "graphics",
            Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )))),
        )
        .expect("adapter binding registers");
    let type_id = OpaqueTypeId::new("graphics", "Texture").unwrap();
    assert!(bindings.register_handle(binding_id, type_id.clone(), HandleId::new(1)));

    assert!(!bindings.register_handles(
        binding_id,
        &[
            (type_id.clone(), HandleId::new(2)),
            (type_id.clone(), HandleId::new(1))
        ],
    ));
    assert!(!bindings.contains_handle(binding_id, &type_id, HandleId::new(2)));
    assert!(bindings.contains_handle(binding_id, &type_id, HandleId::new(1)));
}

#[test]
fn adapter_handle_ids_are_monotonic_and_never_reused() {
    let mut bindings = AdapterBindings::default();
    let binding_id = bindings
        .register_module(
            "graphics",
            Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )))),
        )
        .expect("adapter binding registers");
    let type_id = OpaqueTypeId::new("graphics", "Texture").unwrap();

    assert!(bindings.register_handle(binding_id, type_id.clone(), HandleId::new(1)));
    assert!(bindings.release_handle(binding_id, &type_id, HandleId::new(1)));
    assert!(!bindings.register_handle(binding_id, type_id.clone(), HandleId::new(1)));
    assert!(bindings.register_handle(binding_id, type_id, HandleId::new(2)));
}

#[test]
fn adapter_handles_require_the_binding_that_created_them() {
    let mut bindings = AdapterBindings::default();
    let first_binding = bindings
        .register_module(
            "graphics.gfp",
            Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )))),
        )
        .expect("adapter binding registers");
    let second_binding = bindings
        .register_module(
            "audio.gfp",
            Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )))),
        )
        .expect("adapter binding registers");
    let type_id = OpaqueTypeId::new("graphics", "Texture").unwrap();
    let handle_id = HandleId::new(1);

    assert!(bindings.register_handle(first_binding, type_id.clone(), handle_id));
    assert!(!bindings.release_handle(second_binding, &type_id, handle_id));
    assert!(bindings.contains_handle(first_binding, &type_id, handle_id));
}

#[test]
fn adapter_handle_id_space_stops_after_u32_max() {
    let mut bindings = AdapterBindings::default();
    let binding_id = bindings
        .register_module(
            "graphics.gfp",
            Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )))),
        )
        .expect("adapter binding registers");
    let type_id = OpaqueTypeId::new("graphics", "Texture").unwrap();
    bindings
        .modules
        .get_mut("graphics.gfp")
        .unwrap()
        .next_handle_id = Some(HandleId::new(u32::MAX));

    assert!(bindings.register_handle(binding_id, type_id.clone(), HandleId::new(u32::MAX)));
    assert!(!bindings.register_handle(binding_id, type_id, HandleId::new(u32::MAX)));
}

#[test]
fn execution_failures_preserve_machine_readable_context() {
    let failure = ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "request failed")
        .with_thread_id(7)
        .with_module_id(3);

    assert_eq!(failure.thread_id, Some(7));
    assert_eq!(failure.module_id, Some(3));
}

#[test]
fn execution_failures_preserve_asynchronous_frames() {
    let stack = vec![ExecutionFrame {
        module_id: 3,
        function_id: 7,
        instruction_offset: 11,
    }];
    let failure =
        ExecutionFailure::new(ExecutionFailureKind::VmPanic, "failed").with_stack(stack.clone());
    assert_eq!(failure.stack, stack);
}

struct CancellationRecordingAdapter(std::sync::Arc<std::sync::atomic::AtomicU64>);

impl AdapterModuleBinding for CancellationRecordingAdapter {
    fn descriptor(&self) -> AdapterModuleDescriptor {
        AdapterModuleDescriptor::empty()
    }

    fn dispatch(
        &mut self,
        _symbol: &str,
        _thread_id: usize,
        _request_id: u64,
        _args: &[BoundaryValue],
        _injector: std::sync::Arc<dyn MessageInjector>,
    ) {
    }

    fn cancel(&mut self, _symbol: &str, thread_id: usize, request_id: u64) -> CancellationOutcome {
        self.0.store(
            ((thread_id as u64) << 32) | request_id,
            std::sync::atomic::Ordering::Release,
        );
        CancellationOutcome::Confirmed
    }
}

#[test]
fn adapter_bindings_route_cancellation_to_the_owning_symbol() {
    let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut bindings = AdapterBindings::default();
    bindings
        .register_module(
            "io",
            Box::new(CancellationRecordingAdapter(cancellation.clone())),
        )
        .expect("adapter binding registers");

    bindings.cancel("io", "read", 3, 4);

    assert_eq!(
        cancellation.load(std::sync::atomic::Ordering::Acquire),
        (3_u64 << 32) | 4
    );
}

struct MainThreadOnlyTask(Rc<()>);

impl RunnableTask for MainThreadOnlyTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        assert_eq!(Rc::strong_count(&self.0), 1);
        ThreadResult::Completed(Ok(BoundaryValue::I32(0)))
    }
}

#[test]
fn main_kernel_tasks_accept_non_send_state() {
    let task = KernelTask::Main(Box::new(MainThreadOnlyTask(Rc::new(()))));
    assert_eq!(task.affinity(), TaskAffinity::Main);
    let KernelTask::Main(task) = task else {
        panic!("main task must preserve its affinity");
    };
    assert!(matches!(
        task.run(1),
        ThreadResult::Completed(Ok(BoundaryValue::I32(0)))
    ));
}

fn assert_builtin_checks(name: &str, source: &str) {
    assert!(!source.is_empty());

    let source_file = galfus_core::SourceFile::new(
        galfus_core::SourceId::new(0),
        name.to_string(),
        source.to_string(),
    );
    let parse_result = galfus_frontend::parse(&source_file);
    let mut string_table = galfus_frontend::StringTable::new();
    assert!(
        !parse_result.has_errors(),
        "{name} parse errors: {:?}",
        parse_result.diagnostics()
    );

    let resolve_result =
        galfus_frontend::resolve(&source_file, parse_result.into_graph(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{name} resolve errors: {:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let type_result =
        galfus_frontend::check_declaration_types(&source_file, &graph, &string_table, false);
    assert!(
        !type_result.has_errors(),
        "{name} type errors: {:?}",
        type_result.diagnostics()
    );
}

#[test]
fn test_std_io_source_checks() {
    assert_builtin_checks("std/io", STD_IO_SOURCE);
    assert!(STD_IO_SOURCE.contains("print"));
    assert!(STD_IO_SOURCE.contains("read"));
}

#[test]
fn test_text_source_checks() {
    assert_builtin_checks("text", TEXT_SOURCE);
    assert!(TEXT_SOURCE.contains("length"));
    assert!(TEXT_SOURCE.contains("concat"));
}

#[test]
fn test_format_source_checks() {
    assert_builtin_checks("format", FORMAT_SOURCE);
    assert!(FORMAT_SOURCE.contains("stringify"));
    assert!(FORMAT_SOURCE.contains("parse"));
    assert!(FORMAT_SOURCE.contains("Result"));
}

#[test]
fn test_format_ansi_source_checks() {
    assert_builtin_checks("format/ansi", FORMAT_ANSI_SOURCE);
    assert!(FORMAT_ANSI_SOURCE.contains("Style"));
    assert!(FORMAT_ANSI_SOURCE.contains("apply"));
    assert!(FORMAT_ANSI_SOURCE.contains("red"));
}

#[test]
fn test_std_async_source_checks() {
    assert_builtin_checks("std/async", ASYNC_SOURCE);
    assert!(ASYNC_SOURCE.contains("Future"));
}

#[test]
fn test_thread_source_checks() {
    assert_builtin_checks("std/thread", THREAD_SOURCE);
    assert!(THREAD_SOURCE.contains("isRunning"));
    assert!(THREAD_SOURCE.contains("isExited"));
    assert!(THREAD_SOURCE.contains("exitReason"));
    assert!(THREAD_SOURCE.contains("fn Thread::send(self, data: [u8]): bool"));
}
