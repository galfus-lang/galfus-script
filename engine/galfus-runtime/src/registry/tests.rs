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

    registry
        .register(id, VmThreadState::test_new(), None)
        .unwrap();

    assert!(registry.contains(id));
    assert_eq!(id.raw(), 42);
}

#[test]
fn registry_keeps_the_mailbox_and_key_while_a_thread_is_running() {
    let id = galfus_core::ThreadId::new(1);
    let thread = VmThreadState::test_new();
    let key = Some("worker".to_string());
    let mut registry = ThreadRegistry::new();

    registry.register(id, thread, key).unwrap();
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

    registry
        .register(id, VmThreadState::test_new(), None)
        .unwrap();
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
fn registry_reports_an_exited_spawned_thread_on_its_first_observation() {
    let id = galfus_core::ThreadId::new(1);
    let mut registry = ThreadRegistry::new();

    registry
        .register(id, VmThreadState::test_new(), None)
        .unwrap();
    registry.mark_spawned(id).unwrap();
    assert!(registry.mark_running(id));
    let _thread = registry.take(id).expect("thread is available to run");

    assert!(registry.mark_exited(id, Ok(galfus_contract::BoundaryValue::I32(7))));
    assert!(!registry.is_running(id));
    assert!(registry.is_exited(id));
    assert!(registry.is_exited(id));
}

#[test]
fn exited_threads_keep_only_terminal_metadata() {
    let id = galfus_core::ThreadId::new(1);
    let mut registry = ThreadRegistry::new();

    registry
        .register(id, VmThreadState::test_new(), None)
        .unwrap();
    assert!(registry.mark_exited(id, Ok(galfus_contract::BoundaryValue::Null)));

    assert!(registry.take(id).is_none());
    assert!(registry.get_mailbox(id).is_none());
    assert!(matches!(registry.state(id), Some(ThreadState::Exited(_))));
}

#[test]
fn exited_tombstones_have_bounded_retention() {
    let mut registry = ThreadRegistry::new();
    for raw_id in 1..=1025 {
        let id = galfus_core::ThreadId::new(raw_id);
        registry
            .register(id, VmThreadState::test_new(), None)
            .unwrap();
        assert!(registry.mark_exited(id, Ok(galfus_contract::BoundaryValue::Null)));
    }

    assert!(!registry.contains(galfus_core::ThreadId::new(1)));
    assert!(registry.contains(galfus_core::ThreadId::new(1025)));
}

#[test]
fn registry_only_releases_a_created_thread_once_for_spawn() {
    let id = galfus_core::ThreadId::new(1);
    let mut registry = ThreadRegistry::new();

    registry
        .register(id, VmThreadState::test_new(), None)
        .unwrap();

    assert!(registry.take_created(id).is_some());
    assert!(registry.take_created(id).is_none());
}

#[test]
fn registry_rejects_a_duplicate_nominal_key_without_replacing_the_thread() {
    let first = galfus_core::ThreadId::new(1);
    let second = galfus_core::ThreadId::new(2);
    let mut registry = ThreadRegistry::new();

    registry
        .register(first, VmThreadState::test_new(), Some("worker".to_string()))
        .unwrap();
    let error = registry
        .register(
            second,
            VmThreadState::test_new(),
            Some("worker".to_string()),
        )
        .unwrap_err();

    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::DuplicateThreadKey
    );
    assert_eq!(registry.lookup_key("worker"), Some(first));
    assert!(registry.contains(first));
    assert!(!registry.contains(second));
}
