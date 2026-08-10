use super::BlockedQueue;
use crate::registry::ThreadId;

fn thread_id(value: u32) -> ThreadId {
    galfus_core::ThreadId::new(value)
}

#[test]
fn timeouts_wake_in_deadline_then_timer_id_order() {
    let mut queue = BlockedQueue::new();
    let first = thread_id(1);
    let second = thread_id(2);
    let earlier = thread_id(3);

    queue.block_with_timeout(first, 10).unwrap();
    queue.block_with_timeout(second, 10).unwrap();
    queue.block_with_timeout(earlier, 5).unwrap();

    assert_eq!(queue.tick_timeouts(5), vec![earlier]);
    assert_eq!(queue.tick_timeouts(5), vec![first, second]);
}

#[test]
fn replaced_or_removed_timers_cannot_wake_a_thread() {
    let mut queue = BlockedQueue::new();
    let replaced = thread_id(1);
    let removed = thread_id(2);

    queue.block_with_timeout(replaced, 5).unwrap();
    queue.block_with_timeout(replaced, 10).unwrap();
    queue.block_with_timeout(removed, 5).unwrap();
    queue.remove(removed);

    assert!(queue.tick_timeouts(5).is_empty());
    assert_eq!(queue.tick_timeouts(5), vec![replaced]);
}

#[test]
fn timer_id_exhaustion_keeps_the_existing_queue_state() {
    let mut queue = BlockedQueue::new();
    let first = thread_id(1);
    let second = thread_id(2);
    queue.timer_id_manager.set_next_id_for_test(u32::MAX);

    queue.block_with_timeout(first, 10).unwrap();
    let error = queue.block_with_timeout(second, 10).unwrap_err();

    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );
    assert_eq!(queue.tick_timeouts(10), vec![first]);
}

#[test]
fn expired_timer_id_is_reused() {
    let mut queue = BlockedQueue::new();
    let first = thread_id(1);
    let second = thread_id(2);

    queue.block_with_timeout(first, 1).unwrap();
    queue.tick_timeouts(1);
    queue.block_with_timeout(second, 1).unwrap();

    assert_eq!(
        queue.active_timers.get(&second).unwrap().timer_id,
        galfus_core::TimerId::new(1)
    );
}
