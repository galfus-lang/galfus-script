use super::*;
use std::rc::Rc;
use std::sync::Arc;

struct DummyHost;

struct DummyAdapter(Arc<std::sync::atomic::AtomicUsize>);

impl HostAdapter for DummyAdapter {
    fn dispatch(
        &mut self,
        _thread_id: usize,
        _request_id: u64,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
    }

    fn release_handle(&mut self, _kind: &str, _id: u64) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
}

impl HostProvider for DummyHost {
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
fn adapters_are_registered_by_nominal_module_and_symbol() {
    let mut adapters = Adapters::default();
    adapters.register(
        "graphics",
        "create",
        Box::new(DummyAdapter(Arc::new(std::sync::atomic::AtomicUsize::new(
            0,
        )))),
    );
    assert!(adapters.get_mut("graphics", "create").is_some());
    assert!(adapters.get_mut("graphics", "missing").is_none());
}

#[test]
fn adapters_own_and_release_nominal_handles() {
    let releases = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut adapters = Adapters::default();
    adapters.register(
        "graphics",
        "create",
        Box::new(DummyAdapter(releases.clone())),
    );
    assert!(adapters.register_handle("graphics", "create", "texture", 7));
    assert!(adapters.contains_handle("texture", 7));
    assert!(adapters.release_handle("texture", 7));
    assert!(!adapters.contains_handle("texture", 7));
    assert!(!adapters.release_handle("texture", 7));
    assert_eq!(releases.load(std::sync::atomic::Ordering::Acquire), 1);
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

impl HostAdapter for CancellationRecordingAdapter {
    fn dispatch(
        &mut self,
        _thread_id: usize,
        _request_id: u64,
        _args: &[BoundaryValue],
        _injector: std::sync::Arc<dyn MessageInjector>,
    ) {
    }

    fn cancel(&mut self, thread_id: usize, request_id: u64) {
        self.0.store(
            ((thread_id as u64) << 32) | request_id,
            std::sync::atomic::Ordering::Release,
        );
    }
}

#[test]
fn adapters_route_cancellation_to_the_owning_symbol() {
    let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut adapters = Adapters::default();
    adapters.register(
        "io",
        "read",
        Box::new(CancellationRecordingAdapter(cancellation.clone())),
    );

    adapters.cancel("io", "read", 3, 4);

    assert_eq!(
        cancellation.load(std::sync::atomic::Ordering::Acquire),
        (3_u64 << 32) | 4
    );
}

struct MainThreadOnlyTask(Rc<()>);

impl RunnableTask for MainThreadOnlyTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        assert_eq!(Rc::strong_count(&self.0), 1);
        ThreadResult::Completed(0)
    }
}

#[test]
fn main_kernel_tasks_accept_non_send_state() {
    let task = KernelTask::Main(Box::new(MainThreadOnlyTask(Rc::new(()))));
    assert_eq!(task.affinity(), TaskAffinity::Main);
    let KernelTask::Main(task) = task else {
        panic!("main task must preserve its affinity");
    };
    assert!(matches!(task.run(1), ThreadResult::Completed(0)));
}
