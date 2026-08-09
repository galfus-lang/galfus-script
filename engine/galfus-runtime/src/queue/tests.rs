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
    queue.next_timer_id = u32::MAX - 1;

    queue.block_with_timeout(first, 10).unwrap();
    let error = queue.block_with_timeout(second, 10).unwrap_err();

    assert_eq!(error.kind, galfus_contract::ExecutionFailureKind::IdSpaceExhausted);
    assert_eq!(queue.tick_timeouts(10), vec![first]);
}
