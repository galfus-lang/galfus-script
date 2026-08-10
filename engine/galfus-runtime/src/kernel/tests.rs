use super::VirtualKernel;
use crate::registry::MailboxMessage;
use galfus_vm::thread::VmThreadState;

#[test]
fn expired_timers_are_enqueued_in_deterministic_order() {
    let mut kernel = VirtualKernel::new();
    let first = kernel.spawn(VmThreadState::new(), None).unwrap();
    let second = kernel.spawn(VmThreadState::new(), None).unwrap();
    let earlier = kernel.spawn(VmThreadState::new(), None).unwrap();

    for (thread_id, timeout_ms) in [(first, 10), (second, 10), (earlier, 5)] {
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread remains available for blocking");
        kernel.block(thread_id, thread, Some(timeout_ms)).unwrap();
    }

    assert_eq!(kernel.tick(5), vec![earlier]);
    assert_eq!(kernel.next_runnable(), Some(earlier));
    assert_eq!(kernel.tick(5), vec![first, second]);
    assert_eq!(kernel.next_runnable(), Some(first));
    assert_eq!(kernel.next_runnable(), Some(second));
}

#[test]
fn mailbox_wakeups_keep_their_arrival_order() {
    let mut kernel = VirtualKernel::new();
    let first = kernel.spawn(VmThreadState::new(), None).unwrap();
    let second = kernel.spawn(VmThreadState::new(), None).unwrap();

    for thread_id in [first, second] {
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread remains available for blocking");
        kernel.block(thread_id, thread, None).unwrap();
    }

    for thread_id in [second, first] {
        kernel
            .get_mailbox(thread_id)
            .expect("blocked thread keeps its mailbox")
            .lock()
            .unwrap()
            .push_back(MailboxMessage {
                sender_id: galfus_core::ThreadId::new(0),
                data: vec![thread_id.raw() as u8],
            });
        assert!(kernel.unblock(thread_id));
    }

    assert_eq!(kernel.next_runnable(), Some(second));
    assert_eq!(kernel.next_runnable(), Some(first));
}

#[test]
fn thread_id_exhaustion_does_not_register_a_partial_thread() {
    let mut kernel = VirtualKernel::new();
    kernel.thread_id_manager.set_next_id_for_test(u32::MAX - 1);

    assert_eq!(
        kernel.spawn(VmThreadState::new(), None).unwrap().raw(),
        u32::MAX - 1
    );
    let error = kernel.spawn(VmThreadState::new(), None).unwrap_err();

    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );
    assert_eq!(kernel.active_count(), 1);
}
