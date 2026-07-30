use super::VirtualKernel;
use galfus_vm::thread::VirtualThread;

#[test]
fn expired_timers_are_enqueued_in_deterministic_order() {
    let mut kernel = VirtualKernel::new();
    let first = kernel.spawn(VirtualThread::new());
    let second = kernel.spawn(VirtualThread::new());
    let earlier = kernel.spawn(VirtualThread::new());

    for (thread_id, timeout_ms) in [(first, 10), (second, 10), (earlier, 5)] {
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread remains available for blocking");
        kernel.block(thread_id, thread, Some(timeout_ms));
    }

    assert_eq!(kernel.tick(5), vec![earlier]);
    assert_eq!(kernel.next_runnable(), Some(earlier));
    assert_eq!(kernel.tick(5), vec![first, second]);
    assert_eq!(kernel.next_runnable(), Some(first));
    assert_eq!(kernel.next_runnable(), Some(second));
}
