use super::*;
use crate::driver::{ExecutionDriver, NativeEventBridge, RuntimeEventSink};
use crate::orchestrator::Orchestrator;
use galfus_contract::{
    AdapterBindings, AdapterModuleBinding, AdapterModuleDescriptor, AdapterReleaseError,
    ExecutorStepResult, HandleReleaseOutcome, KernelDriver, KernelTask, SurfaceValue,
};
use galfus_core::{HandleId, OpaqueTypeId};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct IdleDriver {
    events: Arc<NativeEventBridge>,
    exits: Arc<std::sync::Mutex<Vec<Result<i32, ExecutionFailure>>>>,
}

impl IdleDriver {
    fn new() -> Self {
        Self {
            events: Arc::new(NativeEventBridge::new()),
            exits: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl KernelDriver for IdleDriver {
    fn dispatch(&self, _task: KernelTask) {}

    fn on_exit(&self, _callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {}

    fn run(&self) {}

    fn complete(&self, result: Result<i32, ExecutionFailure>) {
        self.exits.lock().expect("exit log lock").push(result);
    }

    fn step(&self) -> ExecutorStepResult {
        ExecutorStepResult::Running
    }
}

impl ExecutionDriver for IdleDriver {
    fn event_sink(&self) -> Arc<dyn RuntimeEventSink> {
        self.events.clone()
    }

    fn drain_events(&self) -> Vec<(crate::event::EventSequence, crate::event::RuntimeEvent)> {
        self.events.drain()
    }

    fn has_pending_events(&self) -> bool {
        self.events.has_pending()
    }
}

struct ReleaseRecordingAdapter(Arc<std::sync::atomic::AtomicUsize>);

struct FailingReleaseAdapter;

impl AdapterModuleBinding for ReleaseRecordingAdapter {
    fn descriptor(&self) -> AdapterModuleDescriptor {
        AdapterModuleDescriptor::empty()
    }

    fn dispatch(
        &mut self,
        _symbol: &str,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        _args: &[SurfaceValue],
        _injector: Arc<dyn galfus_contract::MessageInjector>,
    ) {
    }

    fn release_handle(
        &mut self,
        _type_id: &OpaqueTypeId,
        _id: HandleId,
    ) -> Result<HandleReleaseOutcome, AdapterReleaseError> {
        self.0.fetch_add(1, Ordering::AcqRel);
        Ok(HandleReleaseOutcome::Released)
    }
}

impl AdapterModuleBinding for FailingReleaseAdapter {
    fn descriptor(&self) -> AdapterModuleDescriptor {
        AdapterModuleDescriptor::empty()
    }

    fn dispatch(
        &mut self,
        _symbol: &str,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        _args: &[SurfaceValue],
        _injector: Arc<dyn galfus_contract::MessageInjector>,
    ) {
    }

    fn release_handle(
        &mut self,
        _type_id: &OpaqueTypeId,
        _id: HandleId,
    ) -> Result<HandleReleaseOutcome, AdapterReleaseError> {
        Err(AdapterReleaseError {
            code: "unavailable".to_string(),
            message: "adapter cannot release the resource".to_string(),
        })
    }
}

#[test]
fn execution_transitions_from_created_to_running_and_preserves_completion() {
    let orchestrator = Orchestrator::test_new();
    let mut execution = Execution::new(
        orchestrator,
        Rc::new(IdleDriver::new()),
        Arc::new(AtomicBool::new(true)),
        false,
    );
    assert_eq!(execution.status(), ExecutionState::Created);

    assert!(matches!(
        execution.poll(1),
        Ok(ExecutorStepResult::Completed(0))
    ));
    assert_eq!(execution.status(), ExecutionState::Closed);
    assert_eq!(execution.result(), Some(&Ok(0)));
    assert_eq!(execution.run_sync_to_completion(), Ok(0));
    assert_eq!(execution.result(), Some(&Ok(0)));
}

#[test]
fn execution_shutdown_is_idempotent_and_preserves_its_final_report() {
    let mut execution = Execution::new(
        Orchestrator::test_new(),
        Rc::new(IdleDriver::new()),
        Arc::new(AtomicBool::new(true)),
        false,
    );

    let first = execution.shutdown();
    let second = execution.shutdown();

    assert_eq!(first, second);
    assert_eq!(execution.status(), ExecutionState::Closed);
    assert!(execution.orchestrator.is_none());
    assert_eq!(execution.shutdown_report(), Some(&first));
    assert!(
        matches!(first.result, Err(ref error) if error.kind == ExecutionFailureKind::Cancelled)
    );
}

#[test]
fn dropping_an_incomplete_execution_notifies_the_driver_of_shutdown_failure() {
    let driver = Rc::new(IdleDriver::new());
    let exits = Arc::clone(&driver.exits);
    let execution = Execution::new(
        Orchestrator::test_new(),
        driver,
        Arc::new(AtomicBool::new(true)),
        false,
    );

    drop(execution);

    let exits = exits.lock().expect("exit log remains available");
    assert!(matches!(
        exits.as_slice(),
        [Err(error)] if error.kind == ExecutionFailureKind::Cancelled
    ));
}

#[test]
fn execution_shutdown_reports_adapter_handle_teardown() {
    let releases = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut bindings = AdapterBindings::default();
    let binding_id = bindings
        .register_module(
            "graphics",
            Box::new(ReleaseRecordingAdapter(releases.clone())),
        )
        .expect("adapter binding registers");
    let type_id = OpaqueTypeId::new("graphics", "Texture").expect("valid type id");
    bindings
        .register_handle(binding_id, type_id, HandleId::new(1))
        .expect("handle registers");

    let mut orchestrator = Orchestrator::test_new();
    orchestrator.set_adapter_bindings(Some(Arc::new(std::sync::Mutex::new(bindings))));
    let mut execution = Execution::new(
        orchestrator,
        Rc::new(IdleDriver::new()),
        Arc::new(AtomicBool::new(true)),
        false,
    );

    let report = execution.shutdown();

    assert!(report.adapter_close.is_complete());
    assert_eq!(report.adapter_close.released, 1);
    assert_eq!(releases.load(Ordering::Acquire), 1);
}

#[test]
fn execution_shutdown_propagates_adapter_teardown_failures() {
    let mut bindings = AdapterBindings::default();
    let binding_id = bindings
        .register_module("graphics", Box::new(FailingReleaseAdapter))
        .expect("adapter binding registers");
    let type_id = OpaqueTypeId::new("graphics", "Texture").expect("valid type id");
    bindings
        .register_handle(binding_id, type_id, HandleId::new(1))
        .expect("handle registers");

    let mut orchestrator = Orchestrator::test_new();
    orchestrator.set_adapter_bindings(Some(Arc::new(std::sync::Mutex::new(bindings))));
    let mut execution = Execution::new(
        orchestrator,
        Rc::new(IdleDriver::new()),
        Arc::new(AtomicBool::new(true)),
        false,
    );

    let report = execution.shutdown();

    assert_eq!(report.adapter_close.failures.len(), 1);
    assert!(matches!(
        report.result,
        Err(ref error) if error.kind == ExecutionFailureKind::AdapterCallFailure
    ));
}

#[test]
fn execution_remains_initializing_until_the_orchestrator_signal() {
    let initialization_complete = Arc::new(AtomicBool::new(false));
    let mut orchestrator = Orchestrator::test_new();
    let thread_id = orchestrator
        .kernel_mut()
        .spawn(galfus_vm::thread::VmThreadState::test_new(), None)
        .unwrap();
    let thread = orchestrator.kernel_mut().take_thread(thread_id).unwrap();
    orchestrator
        .kernel_mut()
        .enqueue_runnable(thread_id, thread)
        .unwrap();

    let driver = Rc::new(IdleDriver::new());
    orchestrator.set_vm(Arc::new(galfus_vm::VirtualMachine::new(Default::default())));
    orchestrator.set_driver(driver.clone());

    let mut execution = Execution::new(orchestrator, driver, initialization_complete.clone(), true);

    execution.poll(1).expect("initializing slice succeeds");
    assert_eq!(execution.status(), ExecutionState::Initializing);
    initialization_complete.store(true, Ordering::Release);
    execution.poll(1).expect("running slice succeeds");
    assert_eq!(execution.status(), ExecutionState::Running);
}

#[test]
fn cancellation_transitions_the_execution_to_cancelled() {
    let orchestrator = Orchestrator::test_new();
    let mut execution = Execution::new(
        orchestrator,
        Rc::new(IdleDriver::new()),
        Arc::new(AtomicBool::new(true)),
        false,
    );

    execution.cancel();
    assert_eq!(execution.status(), ExecutionState::Closing);

    let Err(error) = execution.poll(100) else {
        panic!("cancellation must produce a structured failure");
    };
    assert_eq!(error.kind, ExecutionFailureKind::Cancelled);
    assert_eq!(execution.status(), ExecutionState::Closed);
}

#[test]
fn execution_drops_orchestrator_and_sets_failed_state_on_error() {
    let mut orchestrator = Orchestrator::test_new();
    let thread_id = orchestrator
        .kernel_mut()
        .spawn(galfus_vm::thread::VmThreadState::test_new(), None)
        .unwrap();
    orchestrator.set_root_thread(thread_id);
    let thread = orchestrator.kernel_mut().take_thread(thread_id).unwrap();
    orchestrator.kernel_mut().mark_exited(
        thread_id,
        thread,
        Err(ExecutionFailure::new(
            ExecutionFailureKind::VmPanic,
            "test panic",
        )),
    );
    let mut execution = Execution::new(
        orchestrator,
        Rc::new(IdleDriver::new()),
        Arc::new(AtomicBool::new(true)),
        false,
    );
    assert_eq!(execution.status(), ExecutionState::Created);
    let error = match execution.poll(1) {
        Err(e) => e,
        Ok(_) => panic!("execution must fail"),
    };
    assert_eq!(error.kind, ExecutionFailureKind::VmPanic);
    assert_eq!(execution.status(), ExecutionState::Closed);
    // orchestrator should be dropped. Since it is Option<Orchestrator>, it should be None.
    // wait, we can't easily assert on `execution.orchestrator` because it's private and we might not be in the same exact scope, but wait, `execution.rs` and `execution/tests.rs` are in `execution` module.
    assert!(execution.orchestrator.is_none());
}
