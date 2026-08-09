#[cfg(test)]
mod tests;

use crate::registry::ThreadId;
use galfus_core::TimerId;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

pub struct RunnableQueue {
    queue: VecDeque<(ThreadId, bool)>,
}

impl RunnableQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn enqueue(&mut self, id: ThreadId) {
        self.queue.push_back((id, false));
    }

    pub fn enqueue_front(&mut self, id: ThreadId) {
        self.queue.push_front((id, true));
    }

    pub fn dequeue_detailed(&mut self) -> Option<(ThreadId, bool)> {
        self.queue.pop_front()
    }

    pub fn dequeue(&mut self) -> Option<ThreadId> {
        self.queue.pop_front().map(|(id, _)| id)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn remove(&mut self, id: ThreadId) {
        self.queue.retain(|(queued, _)| *queued != id);
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
    next_timer_id: u32,
    timers: BTreeSet<TimerEntry>,
    active_timers: HashMap<ThreadId, TimerEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TimerEntry {
    deadline_ms: u64,
    timer_id: TimerId,
    thread_id: ThreadId,
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

    pub fn block_with_timeout(&mut self, id: ThreadId, timeout_ms: u64) -> Result<(), ExecutionFailure> {
        self.blocked.insert(id);
        self.remove_timer(id);
        
        let timer = TimerEntry {
            deadline_ms: self.clock_ms.saturating_add(timeout_ms),
            timer_id: TimerId::new(self.next_timer_id),
            thread_id: id,
        };
        
        self.next_timer_id = self.next_timer_id.checked_add(1)
            .ok_or_else(|| ExecutionFailure::new(ExecutionFailureKind::IdSpaceExhausted, "timer id space exhausted"))?;
            
        self.timers.insert(timer);
        self.active_timers.insert(id, timer);
        Ok(())
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

            let thread_id = timer.thread_id;
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
