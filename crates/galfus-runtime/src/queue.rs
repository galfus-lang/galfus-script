#[cfg(test)]
mod tests;

use crate::registry::ThreadId;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

pub struct RunnableQueue {
    queue: VecDeque<ThreadId>,
}

impl RunnableQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn enqueue(&mut self, id: ThreadId) {
        self.queue.push_back(id);
    }

    pub fn dequeue(&mut self) -> Option<ThreadId> {
        self.queue.pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn remove(&mut self, id: ThreadId) {
        self.queue.retain(|queued| *queued != id);
    }
}

impl Default for RunnableQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BlockedQueue {
    blocked: HashSet<ThreadId>,
    clock_ms: u64,
    next_timer_id: u64,
    timers: BTreeSet<TimerEntry>,
    active_timers: HashMap<ThreadId, TimerEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TimerEntry {
    deadline_ms: u64,
    timer_id: u64,
    thread_id: u64,
}

impl BlockedQueue {
    pub fn new() -> Self {
        Self {
            blocked: HashSet::new(),
            clock_ms: 0,
            next_timer_id: 1,
            timers: BTreeSet::new(),
            active_timers: HashMap::new(),
        }
    }

    pub fn block(&mut self, id: ThreadId) {
        self.remove_timer(id);
        self.blocked.insert(id);
    }

    pub fn block_with_timeout(&mut self, id: ThreadId, timeout_ms: u64) {
        self.blocked.insert(id);
        self.remove_timer(id);
        let timer = TimerEntry {
            deadline_ms: self.clock_ms.saturating_add(timeout_ms),
            timer_id: self.next_timer_id,
            thread_id: id.raw(),
        };
        self.next_timer_id = self.next_timer_id.saturating_add(1);
        self.timers.insert(timer);
        self.active_timers.insert(id, timer);
    }

    pub fn unblock(&mut self, id: ThreadId) -> bool {
        self.remove_timer(id);
        self.blocked.remove(&id)
    }

    pub fn remove(&mut self, id: ThreadId) {
        self.remove_timer(id);
        self.blocked.remove(&id);
    }

    fn remove_timer(&mut self, id: ThreadId) {
        if let Some(timer) = self.active_timers.remove(&id) {
            self.timers.remove(&timer);
        }
    }

    /// Advances virtual time and returns expired threads by deadline then timer ID.
    pub fn tick_timeouts(&mut self, delta_ms: u64) -> Vec<ThreadId> {
        self.clock_ms = self.clock_ms.saturating_add(delta_ms);
        let mut woke_up = Vec::new();

        while let Some(timer) = self.timers.iter().next().copied() {
            if timer.deadline_ms > self.clock_ms {
                break;
            }
            self.timers.remove(&timer);

            let Some(thread_id) = ThreadId::from_raw(timer.thread_id) else {
                continue;
            };
            if self.active_timers.get(&thread_id) != Some(&timer) {
                continue;
            }
            self.active_timers.remove(&thread_id);
            if self.blocked.remove(&thread_id) {
                woke_up.push(thread_id);
            }
        }

        woke_up
    }
}

impl Default for BlockedQueue {
    fn default() -> Self {
        Self::new()
    }
}
