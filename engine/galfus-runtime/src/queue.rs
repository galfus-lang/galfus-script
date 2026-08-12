#[cfg(test)]
mod tests;

use crate::registry::ThreadId;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};
use galfus_core::TimerId;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

pub struct RunnableQueue {
    queue: VecDeque<(ThreadId, bool)>,
    queued: HashSet<ThreadId>,
}

impl RunnableQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            queued: HashSet::new(),
        }
    }

    pub fn enqueue(&mut self, id: ThreadId) {
        if self.queued.insert(id) {
            self.queue.push_back((id, false));
        }
    }

    pub fn enqueue_front(&mut self, id: ThreadId) {
        if self.queued.insert(id) {
            self.queue.push_front((id, true));
        }
    }

    pub fn dequeue_detailed(&mut self) -> Option<(ThreadId, bool)> {
        let entry = self.queue.pop_front()?;
        self.queued.remove(&entry.0);
        Some(entry)
    }

    pub fn dequeue(&mut self) -> Option<ThreadId> {
        self.dequeue_detailed().map(|(id, _)| id)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn remove(&mut self, id: ThreadId) {
        self.queue.retain(|(queued, _)| *queued != id);
        self.queued.remove(&id);
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
    timer_id_manager: galfus_core::id_manager::IdManager<TimerId>,
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
            timer_id_manager: galfus_core::id_manager::IdManager::new(1),
            timers: BTreeSet::new(),
            active_timers: HashMap::new(),
        }
    }

    pub fn block(&mut self, id: ThreadId) -> bool {
        let had_timer = self.remove_timer(id).is_some();
        self.blocked.insert(id);
        had_timer
    }

    pub fn block_with_timeout(
        &mut self,
        id: ThreadId,
        timeout_ms: u64,
    ) -> Result<bool, ExecutionFailure> {
        let timer_id = self.timer_id_manager.try_allocate().ok_or_else(|| {
            ExecutionFailure::new(
                ExecutionFailureKind::IdSpaceExhausted,
                "timer id space exhausted",
            )
        })?;
        let timer = TimerEntry {
            deadline_ms: self.clock_ms.saturating_add(timeout_ms),
            timer_id,
            thread_id: id,
        };
        let had_timer = self.remove_timer(id).is_some();
        self.blocked.insert(id);
        self.timers.insert(timer);
        self.active_timers.insert(id, timer);
        Ok(had_timer)
    }

    pub fn unblock(&mut self, id: ThreadId) -> (bool, bool) {
        let had_timer = self.remove_timer(id).is_some();
        let was_blocked = self.blocked.remove(&id);
        (was_blocked, had_timer)
    }

    pub fn remove(&mut self, id: ThreadId) {
        self.remove_timer(id);
        self.blocked.remove(&id);
    }

    fn remove_timer(&mut self, id: ThreadId) -> Option<TimerEntry> {
        if let Some(timer) = self.active_timers.remove(&id) {
            self.timers.remove(&timer);
            self.timer_id_manager.free(timer.timer_id);
            Some(timer)
        } else {
            None
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
            self.timer_id_manager.free(timer.timer_id);
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
