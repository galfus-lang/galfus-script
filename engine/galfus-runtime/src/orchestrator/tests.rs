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
use std::thread;

struct RecordingProvider(Arc<AtomicBool>);

impl HostProvider for RecordingProvider {
    fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
        galfus_contract::ProviderDescriptor::default()
    }

    fn dispatch(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_id: galfus_core::RequestId,
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
        _request_id: galfus_core::RequestId,
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
        request_id: galfus_core::RequestId::new(1),
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
#[should_panic(expected = "main-thread token used from another thread")]
fn main_thread_tokens_reject_a_different_thread_binding() {
    let other_thread_id = thread::spawn(|| thread::current().id())
        .join()
        .expect("thread identity is available");
    let token = MainThreadToken {
        thread_id: other_thread_id,
        _marker: std::marker::PhantomData,
    };

    token.assert_current();
}

#[test]
#[should_panic(expected = "orchestrator accessed from a non-main thread")]
fn orchestrator_rejects_a_different_thread_binding() {
    let other_thread_id = thread::spawn(|| thread::current().id())
        .join()
        .expect("thread identity is available");
    let mut orchestrator = Orchestrator::new();
    orchestrator.main_thread_id = other_thread_id;

    let _ = orchestrator.main_thread_token();
}

#[test]
fn spawned_event_is_registered_and_queued_on_the_main_thread() {
    let mut orchestrator = Orchestrator::new();
    orchestrator.sink().send(RuntimeEvent::ThreadSpawned {
        thread: galfus_vm::thread::VmThreadState::new(),
    });

    let token = orchestrator.main_thread_token();
    orchestrator.process_events(token);

    assert_eq!(orchestrator.kernel(token).active_count(), 1);
    assert_eq!(orchestrator.kernel(token).runnable_count(), 1);
}

#[test]
fn cancellation_event_removes_a_queued_thread() {
    let mut orchestrator = Orchestrator::new();
    let token = orchestrator.main_thread_token();
    let thread_id = {
        let kernel = orchestrator.kernel_mut(token);
        let thread_id = kernel
            .spawn(galfus_vm::thread::VmThreadState::new(), None)
            .unwrap();
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread is registered");
        kernel.enqueue_runnable(thread_id, thread);
        thread_id
    };
    orchestrator
        .sink()
        .send(RuntimeEvent::CancelThread { thread_id });

    orchestrator.process_events(token);

    assert_eq!(orchestrator.kernel(token).active_count(), 0);
    assert_eq!(orchestrator.kernel(token).runnable_count(), 0);
}

#[test]
fn execution_cancellation_removes_every_thread_and_returns_a_structured_failure() {
    let mut orchestrator = Orchestrator::new();
    let token = orchestrator.main_thread_token();
    for _ in 0..2 {
        let kernel = orchestrator.kernel_mut(token);
        let thread_id = kernel
            .spawn(galfus_vm::thread::VmThreadState::new(), None)
            .unwrap();
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread is registered");
        kernel.enqueue_runnable(thread_id, thread);
    }
    orchestrator.sink().send(RuntimeEvent::CancelExecution);

    match orchestrator.step(100) {
        galfus_contract::ThreadResult::Discarded => {}
        _ => panic!("execution cancellation must fail with Cancelled"),
    }
}

#[test]
fn late_provider_completions_after_thread_cancellation_are_ignored() {
    let mut orchestrator = Orchestrator::new();
    let token = orchestrator.main_thread_token();
    let thread_id = orchestrator
        .kernel_mut(token)
        .spawn(galfus_vm::thread::VmThreadState::new(), None)
        .unwrap();
    let sink = orchestrator.sink();
    sink.send(RuntimeEvent::CancelThread { thread_id });
    for _ in 0..2 {
        sink.send(RuntimeEvent::EffectCompleted {
            thread_id,
            request_id: galfus_core::RequestId::new(1),
            result: Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::ProviderFailure,
                "late provider completion",
            )),
        });
    }

    orchestrator.process_events(token);

    assert_eq!(orchestrator.kernel(token).active_count(), 0);
    assert!(orchestrator.failure.is_none());
    assert!(orchestrator.pending_continuations.is_empty());
    assert_eq!(orchestrator.late_completion_count(), 2);
}

#[test]
fn orchestrator_id_domains_fail_without_wrapping() {
    let mut orchestrator = Orchestrator::new();
    let thread_id = galfus_core::ThreadId::new(1);
    let thread = galfus_vm::thread::VmThreadState::new();

    orchestrator
        .request_id_manager
        .set_next_id_for_test(u32::MAX - 1);
    assert_eq!(
        orchestrator
            .allocate_request_id(thread_id, galfus_core::FutureId::new(1), &thread)
            .unwrap()
            .raw(),
        u32::MAX - 1
    );
    assert!(
        orchestrator
            .allocate_request_id(thread_id, galfus_core::FutureId::new(1), &thread)
            .is_none()
    );
    assert_eq!(
        orchestrator.failure.as_ref().unwrap().kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );

    orchestrator.failure = None;
    orchestrator
        .future_id_manager
        .set_next_id_for_test(u32::MAX - 1);
    assert_eq!(
        orchestrator
            .allocate_future_id(thread_id, &thread)
            .unwrap()
            .raw(),
        u32::MAX - 1
    );
    assert!(
        orchestrator
            .allocate_future_id(thread_id, &thread)
            .is_none()
    );

    orchestrator.failure = None;
    orchestrator
        .coordinator_id_manager
        .set_next_id_for_test(u32::MAX - 1);
    assert_eq!(
        orchestrator
            .allocate_coordinator_id(thread_id, &thread)
            .unwrap()
            .raw(),
        u32::MAX - 1
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
