use crate::queue::{BlockedQueue, RunnableQueue};
use crate::registry::{ThreadId, ThreadRegistry};
use galfus_vm::thread::MailboxMessage;
use galfus_vm::thread::VirtualThread;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Manages thread lifecycle, scheduling queues, and timers.
pub struct VirtualKernel {
    next_thread_id: AtomicU64,
    registry: ThreadRegistry,
    runnable: RunnableQueue,
    blocked: BlockedQueue,
}

impl VirtualKernel {
    pub fn new() -> Self {
        Self {
            next_thread_id: AtomicU64::new(1),
            registry: ThreadRegistry::new(),
            runnable: RunnableQueue::new(),
            blocked: BlockedQueue::new(),
        }
    }

    /// Allocates a new ThreadId and registers the thread as runnable.
    pub fn spawn(&mut self, thread: VirtualThread) -> ThreadId {
        let raw_id = self.next_thread_id.fetch_add(1, Ordering::Relaxed);
        let id = ThreadId::from_raw(raw_id).expect("thread id should be non-zero");
        self.registry.register(id, thread);
        self.runnable.enqueue(id);
        id
    }

    /// Parks a currently running thread without blocking it.
    pub fn park_running(&mut self, id: ThreadId, thread: VirtualThread) {
        self.registry.park(id, thread);
    }

    /// Blocks a thread, optionally with a timeout.
    pub fn block(&mut self, id: ThreadId, thread: VirtualThread, timeout: Option<u64>) {
        self.registry.park(id, thread);
        if let Some(ms) = timeout {
            self.blocked.block_with_timeout(id, ms);
        } else {
            self.blocked.block(id);
        }
    }

    /// Unblocks a thread and makes it runnable again.
    pub fn unblock(&mut self, id: ThreadId) -> bool {
        if self.blocked.unblock(id) {
            self.runnable.enqueue(id);
            true
        } else {
            false
        }
    }

    /// Returns the next runnable ThreadId.
    pub fn next_runnable(&mut self) -> Option<ThreadId> {
        self.runnable.dequeue()
    }

    /// Ticks timeouts and makes threads runnable if their timers expire.
    pub fn tick(&mut self, delta_ms: u64) -> Vec<ThreadId> {
        let woke_up = self.blocked.tick_timeouts(delta_ms);
        for &id in &woke_up {
            if self.registry.contains(id) {
                self.runnable.enqueue(id);
            }
        }
        woke_up
    }

    // Pass-through methods for tasks

    pub fn take_thread(&mut self, id: ThreadId) -> Option<VirtualThread> {
        self.registry.take(id)
    }

    pub fn mark_running(&mut self, id: ThreadId) -> bool {
        self.registry.mark_running(id)
    }

    pub fn mark_exited(&mut self, id: ThreadId, thread: VirtualThread, code: i32) -> bool {
        self.registry.register_with_id(id, thread);
        self.registry.mark_exited(id, code)
    }

    pub fn lookup_key(&self, key: &str) -> Option<ThreadId> {
        self.registry.lookup_key(key)
    }

    pub fn get_mailbox(&self, id: ThreadId) -> Option<Arc<Mutex<VecDeque<MailboxMessage>>>> {
        self.registry.get_mailbox(id)
    }
}

impl Default for VirtualKernel {
    fn default() -> Self {
        Self::new()
    }
}
