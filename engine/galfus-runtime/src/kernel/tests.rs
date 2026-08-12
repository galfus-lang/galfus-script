use super::VirtualKernel;
use crate::registry::MailboxMessage;
use galfus_vm::thread::VmThreadState;

#[test]
fn expired_timers_are_enqueued_in_deterministic_order() {
    let mut kernel = VirtualKernel::new();
    let first = kernel.spawn(VmThreadState::test_new(), None).unwrap();
    let second = kernel.spawn(VmThreadState::test_new(), None).unwrap();
    let earlier = kernel.spawn(VmThreadState::test_new(), None).unwrap();

    for (thread_id, timeout_ms) in [(first, 10), (second, 10), (earlier, 5)] {
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread remains available for blocking");
        kernel.block(thread_id, thread, Some(timeout_ms)).unwrap();
    }

    assert_eq!(kernel.tick(5), vec![(earlier, Ok(()))]);
    assert_eq!(kernel.next_runnable(), Some(earlier));
    assert_eq!(kernel.tick(5), vec![(first, Ok(())), (second, Ok(()))]);
    assert_eq!(kernel.next_runnable(), Some(first));
    assert_eq!(kernel.next_runnable(), Some(second));
}

#[test]
fn mailbox_wakeups_keep_their_arrival_order() {
    let mut kernel = VirtualKernel::new();
    let first = kernel.spawn(VmThreadState::test_new(), None).unwrap();
    let second = kernel.spawn(VmThreadState::test_new(), None).unwrap();

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
        assert!(kernel.unblock(thread_id).unwrap());
    }

    assert_eq!(kernel.next_runnable(), Some(second));
    assert_eq!(kernel.next_runnable(), Some(first));
}

#[test]
fn thread_id_exhaustion_does_not_register_a_partial_thread() {
    let mut kernel = VirtualKernel::new();
    kernel.thread_id_manager.set_next_id_for_test(u32::MAX);

    assert_eq!(
        kernel.spawn(VmThreadState::test_new(), None).unwrap().raw(),
        u32::MAX
    );
    let error = kernel.spawn(VmThreadState::test_new(), None).unwrap_err();

    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );
    assert_eq!(kernel.active_count(), 1);
}

#[test]
fn duplicate_thread_key_does_not_consume_an_id_and_can_be_reused_after_cancellation() {
    let mut kernel = VirtualKernel::new();
    let first = kernel
        .spawn(VmThreadState::test_new(), Some("worker".to_string()))
        .unwrap();

    let error = kernel
        .spawn(VmThreadState::test_new(), Some("worker".to_string()))
        .unwrap_err();
    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::DuplicateThreadKey
    );
    assert_eq!(kernel.active_count(), 1);

    assert!(kernel.cancel(first));
    let reused = kernel
        .spawn(VmThreadState::test_new(), Some("worker".to_string()))
        .unwrap();
    assert_eq!(reused, first);
}
