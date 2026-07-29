use super::*;

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
