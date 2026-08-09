use super::*;
use galfus_vm::thread::VmThreadState;

#[test]
fn thread_ids_are_executor_owned_and_non_zero() {
    assert_ne!(galfus_core::ThreadId::new(1), galfus_core::ThreadId::new(2));
}

#[test]
fn registry_preserves_the_executor_assigned_identity() {
    let id = galfus_core::ThreadId::new(42);
    let mut registry = ThreadRegistry::new();

    registry.register(id, VmThreadState::new(), None);

    assert!(registry.contains(id));
    assert_eq!(id.raw(), 42);
}

#[test]
fn registry_keeps_the_mailbox_and_key_while_a_thread_is_running() {
    let id = galfus_core::ThreadId::new(1);
    let thread = VmThreadState::new();
    let key = Some("worker".to_string());
    let mut registry = ThreadRegistry::new();

    registry.register(id, thread, key);
    let mailbox = registry.get_mailbox(id).expect("mailbox is registered");
    let _running_thread = registry.take(id).expect("thread is available to run");

    mailbox.lock().unwrap().push_back(MailboxMessage {
        sender_id: galfus_core::ThreadId::new(7),
        data: vec![42],
    });

    assert!(registry.contains(id));
    assert_eq!(registry.lookup_key("worker"), Some(id));
    assert_eq!(registry.state(id), Some(ThreadState::Created));
    let message = registry
        .get_mailbox(id)
        .unwrap()
        .lock()
        .unwrap()
        .pop_front()
        .expect("message is preserved");
    assert_eq!(message.sender_id, galfus_core::ThreadId::new(7));
    assert_eq!(message.data, vec![42]);
}

#[test]
fn registry_tracks_state_after_the_thread_body_is_taken() {
    let id = galfus_core::ThreadId::new(1);
    let mut registry = ThreadRegistry::new();

    registry.register(id, VmThreadState::new(), None);
    assert!(registry.mark_running(id));
    let _running_thread = registry.take(id).expect("thread is available to run");

    assert!(registry.mark_exited(id, Ok(galfus_contract::BoundaryValue::I32(7))));
    assert_eq!(
        registry.state(id),
        Some(ThreadState::Exited(Ok(
            galfus_contract::BoundaryValue::I32(7)
        )))
    );
}

#[test]
fn registry_only_releases_a_created_thread_once_for_spawn() {
    let id = galfus_core::ThreadId::new(1);
    let mut registry = ThreadRegistry::new();

    registry.register(id, VmThreadState::new(), None);

    assert!(registry.take_created(id).is_some());
    assert!(registry.take_created(id).is_none());
}
