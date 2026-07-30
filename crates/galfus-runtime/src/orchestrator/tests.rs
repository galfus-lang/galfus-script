use super::*;
use galfus_contract::RunnableTask;
use std::thread;

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
        thread: galfus_vm::thread::VirtualThread::new(),
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
        let thread_id = kernel.spawn(galfus_vm::thread::VirtualThread::new());
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
        let thread_id = kernel.spawn(galfus_vm::thread::VirtualThread::new());
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread is registered");
        kernel.enqueue_runnable(thread_id, thread);
    }
    orchestrator.sink().send(RuntimeEvent::CancelExecution);

    match Box::new(orchestrator).run(100) {
        galfus_contract::ThreadResult::Failed(error) => {
            assert_eq!(error.kind, galfus_contract::ExecutionFailureKind::Cancelled);
        }
        _ => panic!("execution cancellation must fail with Cancelled"),
    }
}

#[test]
fn late_provider_completions_after_thread_cancellation_are_ignored() {
    let mut orchestrator = Orchestrator::new();
    let token = orchestrator.main_thread_token();
    let thread_id = orchestrator
        .kernel_mut(token)
        .spawn(galfus_vm::thread::VirtualThread::new());
    let sink = orchestrator.sink();
    sink.send(RuntimeEvent::CancelThread { thread_id });
    for _ in 0..2 {
        sink.send(RuntimeEvent::EffectCompleted {
            thread_id,
            request_id: 1,
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
}
