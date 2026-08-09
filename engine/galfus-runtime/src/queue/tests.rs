use super::BlockedQueue;
use crate::registry::ThreadId;

fn thread_id(value: u64) -> ThreadId {
    galfus_core::ThreadId::new(value as u32)
}

#[test]
fn timeouts_wake_in_deadline_then_timer_id_order() {
    let mut queue = BlockedQueue::new();
    let first = thread_id(1);
    let second = thread_id(2);
    let earlier = thread_id(3);

    queue.block_with_timeout(first, 10);
    queue.block_with_timeout(second, 10);
    queue.block_with_timeout(earlier, 5);

    assert_eq!(queue.tick_timeouts(5), vec![earlier]);
    assert_eq!(queue.tick_timeouts(5), vec![first, second]);
}

#[test]
fn replaced_or_removed_timers_cannot_wake_a_thread() {
    let mut queue = BlockedQueue::new();
    let replaced = thread_id(1);
    let removed = thread_id(2);

    queue.block_with_timeout(replaced, 5);
    queue.block_with_timeout(replaced, 10);
    queue.block_with_timeout(removed, 5);
    queue.remove(removed);

    assert!(queue.tick_timeouts(5).is_empty());
    assert_eq!(queue.tick_timeouts(5), vec![replaced]);
}
