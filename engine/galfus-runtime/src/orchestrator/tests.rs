use super::*;
use crate::orchestrator::adapter::ProviderDispatchTask;
use galfus_contract::{
    BoundaryValue, HostProvider, MessageInjector, Providers, RunnableTask, TaskAffinity,
    ThreadResult,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct RecordingProvider(Arc<AtomicBool>);

impl HostProvider for RecordingProvider {
    fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
        galfus_contract::ProviderDescriptor::default()
    }

    fn dispatch(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        _name: &str,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
        self.0.store(true, Ordering::Release);
    }
}

struct NoopInjector;

impl MessageInjector for NoopInjector {
    fn inject_system_response(
        &self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        _result: Result<BoundaryValue, galfus_contract::ExecutionFailure>,
    ) {
    }
}

fn provider_dispatch_task(called: Arc<AtomicBool>) -> ProviderDispatchTask {
    ProviderDispatchTask {
        providers: Arc::new(Mutex::new(Providers::with_host(Box::new(
            RecordingProvider(called),
        )))),
        thread_id: galfus_core::ThreadId::new(1),
        request_lease: galfus_core::RequestLease::new(galfus_core::RequestId::new(1), 1),
        name: "operation".to_string(),
        args: vec![],
        injector: Arc::new(NoopInjector),
        active: Arc::new(AtomicBool::new(true)),
    }
}

#[test]
fn provider_dispatch_tasks_use_the_declared_driver_lane() {
    let called = Arc::new(AtomicBool::new(false));
    let task = provider_dispatch_task(called.clone());
    let KernelTask::Main(task) = task.into_kernel_task(TaskAffinity::Main) else {
        panic!("main-affine provider must receive a main task");
    };
    assert!(matches!(task.run(1), ThreadResult::Discarded));
    assert!(called.load(Ordering::Acquire));

    let task = provider_dispatch_task(Arc::new(AtomicBool::new(false)));
    assert!(matches!(
        task.into_kernel_task(TaskAffinity::Any),
        KernelTask::Any(_)
    ));
}

#[test]
fn cancelled_provider_dispatch_tasks_do_not_start_adapter_work() {
    let called = Arc::new(AtomicBool::new(false));
    let task = provider_dispatch_task(called.clone());
    task.active.store(false, Ordering::Release);

    assert!(matches!(Box::new(task).run(1), ThreadResult::Discarded));
    assert!(!called.load(Ordering::Acquire));
}

#[test]
fn spawned_event_is_registered_and_queued_by_the_execution_owner() {
    let mut orchestrator = Orchestrator::new();
    orchestrator.submit_event(RuntimeEvent::ThreadSpawned {
        thread: galfus_vm::thread::VmThreadState::new(),
    });

    orchestrator.process_events();

    assert_eq!(orchestrator.kernel().active_count(), 1);
    assert_eq!(orchestrator.kernel().runnable_count(), 1);
}

#[test]
fn cancellation_event_removes_a_queued_thread() {
    let mut orchestrator = Orchestrator::new();
    let thread_id = {
        let kernel = orchestrator.kernel_mut();
        let thread_id = kernel
            .spawn(galfus_vm::thread::VmThreadState::new(), None)
            .unwrap();
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread is registered");
        kernel.enqueue_runnable(thread_id, thread);
        thread_id
    };
    orchestrator.submit_event(RuntimeEvent::CancelThread { thread_id });

    orchestrator.process_events();

    assert_eq!(orchestrator.kernel().active_count(), 0);
    assert_eq!(orchestrator.kernel().runnable_count(), 0);
}

#[test]
fn execution_cancellation_removes_every_thread_and_returns_a_structured_failure() {
    let mut orchestrator = Orchestrator::new();
    for _ in 0..2 {
        let kernel = orchestrator.kernel_mut();
        let thread_id = kernel
            .spawn(galfus_vm::thread::VmThreadState::new(), None)
            .unwrap();
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread is registered");
        kernel.enqueue_runnable(thread_id, thread);
    }
    orchestrator.submit_event(RuntimeEvent::CancelExecution);

    match orchestrator.step(100) {
        galfus_contract::ThreadResult::Discarded => {}
        _ => panic!("execution cancellation must fail with Cancelled"),
    }
}

#[test]
fn late_provider_completions_after_thread_cancellation_are_ignored() {
    let mut orchestrator = Orchestrator::new();
    let thread_id = orchestrator
        .kernel_mut()
        .spawn(galfus_vm::thread::VmThreadState::new(), None)
        .unwrap();
    orchestrator.submit_event(RuntimeEvent::CancelThread { thread_id });
    for _ in 0..2 {
        orchestrator.submit_event(RuntimeEvent::EffectCompleted {
            thread_id,
            request_lease: galfus_core::RequestLease::new(galfus_core::RequestId::new(1), 0),
            result: Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::ProviderFailure,
                "late provider completion",
            )),
        });
    }

    orchestrator.process_events();

    assert_eq!(orchestrator.kernel().active_count(), 0);
    assert!(orchestrator.failure.is_none());
    assert!(orchestrator.pending_continuations.is_empty());
    assert_eq!(orchestrator.late_completion_count(), 2);
}

#[test]
fn orchestrator_id_domains_fail_without_wrapping() {
    let mut orchestrator = Orchestrator::new();
    let thread_id = galfus_core::ThreadId::new(1);
    let thread = galfus_vm::thread::VmThreadState::new();

    assert!(
        orchestrator
            .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
            .is_some()
    );

    orchestrator
        .request_id_manager
        .set_next_id_for_test(u32::MAX);
    assert_eq!(
        orchestrator
            .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
            .unwrap()
            .id
            .raw(),
        u32::MAX
    );
    assert!(
        orchestrator
            .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
            .is_none()
    );
    assert_eq!(
        orchestrator.failure.as_ref().unwrap().kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );

    orchestrator.failure = None;
    orchestrator
        .future_id_manager
        .set_next_id_for_test(u32::MAX);
    assert_eq!(
        orchestrator
            .allocate_future_lease(thread_id, &thread)
            .unwrap()
            .id
            .raw(),
        u32::MAX
    );
    assert!(
        orchestrator
            .allocate_future_lease(thread_id, &thread)
            .is_none()
    );

    orchestrator.failure = None;
    orchestrator
        .coordinator_id_manager
        .set_next_id_for_test(u32::MAX);
    assert_eq!(
        orchestrator
            .allocate_coordinator_id(thread_id, &thread)
            .unwrap()
            .raw(),
        u32::MAX
    );
    assert!(
        orchestrator
            .allocate_coordinator_id(thread_id, &thread)
            .is_none()
    );
    assert_eq!(
        orchestrator.failure.as_ref().unwrap().kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );
}

#[test]
fn generations_prevent_reuse_collisions() {
    let mut orchestrator = Orchestrator::new();
    let thread_id = galfus_core::ThreadId::new(1);
    let thread = galfus_vm::thread::VmThreadState::new();

    // Allocate first time
    let lease1 = orchestrator
        .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
        .unwrap();
    assert_eq!(lease1.generation, 1);

    // Free the ID manually for test
    orchestrator.request_id_manager.free(lease1.id);

    // Allocate again, should get same ID but higher generation
    let lease2 = orchestrator
        .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
        .unwrap();
    assert_eq!(lease1.id, lease2.id);
    assert_eq!(lease2.generation, 2);

    // Complete using the old lease, should be ignored
    orchestrator.submit_event(RuntimeEvent::EffectCompleted {
        thread_id,
        request_lease: lease1,
        result: Ok(galfus_contract::BoundaryValue::Null),
    });

    orchestrator.process_events();
    assert_eq!(orchestrator.late_completion_count(), 0); // ignored because generation mismatch
}
