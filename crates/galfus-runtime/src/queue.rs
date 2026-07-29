use crate::registry::ThreadId;
use std::collections::{HashMap, HashSet, VecDeque};

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
    timeouts: HashMap<ThreadId, u64>,
}

impl BlockedQueue {
    pub fn new() -> Self {
        Self {
            blocked: HashSet::new(),
            timeouts: HashMap::new(),
        }
    }

    pub fn block(&mut self, id: ThreadId) {
        self.blocked.insert(id);
    }

    pub fn block_with_timeout(&mut self, id: ThreadId, timeout_ms: u64) {
        self.blocked.insert(id);
        self.timeouts.insert(id, timeout_ms);
    }

    pub fn unblock(&mut self, id: ThreadId) -> bool {
        self.timeouts.remove(&id);
        self.blocked.remove(&id)
    }

    pub fn remove(&mut self, id: ThreadId) {
        self.timeouts.remove(&id);
        self.blocked.remove(&id);
    }

    /// Reduces the timeout of all blocked threads by delta_ms.
    /// Returns a list of ThreadIds whose timeouts reached 0.
    pub fn tick_timeouts(&mut self, delta_ms: u64) -> Vec<ThreadId> {
        let mut woke_up = Vec::new();
        self.timeouts.retain(|&id, remaining_ms| {
            if *remaining_ms <= delta_ms {
                woke_up.push(id);
                false
            } else {
                *remaining_ms -= delta_ms;
                true
            }
        });

        // Unblock them
        for id in &woke_up {
            self.blocked.remove(id);
        }

        woke_up
    }
}

impl Default for BlockedQueue {
    fn default() -> Self {
        Self::new()
    }
}
