#[cfg(test)]
mod tests;

use crate::queue::{BlockedQueue, RunnableQueue};
use crate::registry::MailboxMessage;
use crate::registry::{ThreadId, ThreadRegistry};
use galfus_vm::thread::VmThreadState;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Manages thread lifecycle, scheduling queues, and timers.
pub struct VirtualKernel {
    next_thread_id: AtomicU64,
    registry: ThreadRegistry,
    pub(crate) runnable: RunnableQueue,
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
    pub fn spawn(&mut self, thread: VmThreadState, key: Option<String>) -> ThreadId {
        let raw_id = self.next_thread_id.fetch_add(1, Ordering::Relaxed);
        let id = ThreadId::from_raw(raw_id).expect("thread id should be non-zero");
        self.registry.register(id, thread, key);
        id
    }

    /// Parks a currently running thread without blocking it.
    pub fn enqueue_runnable(&mut self, id: ThreadId, thread: VmThreadState) {
        self.registry.restore_vm_state(id, thread);
        self.runnable.enqueue(id);
    }

    pub fn park_running(&mut self, id: ThreadId, thread: VmThreadState) {
        self.registry.restore_vm_state(id, thread);
    }

    /// Blocks a thread, optionally with a timeout.
    pub fn block(&mut self, id: ThreadId, thread: VmThreadState, timeout: Option<u64>) {
        self.registry.restore_vm_state(id, thread);
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

    /// Removes a thread from every schedulable state.
    pub fn cancel(&mut self, id: ThreadId) -> bool {
        self.runnable.remove(id);
        self.blocked.remove(id);
        self.registry.cancel(id)
    }

    /// Removes every thread from runnable and blocked state during execution shutdown.
    pub fn cancel_all(&mut self) {
        let thread_ids = self
            .registry
            .debug_states()
            .into_iter()
            .map(|(thread_id, _)| thread_id)
            .collect::<Vec<_>>();
        for thread_id in thread_ids {
            self.cancel(thread_id);
        }
    }

    /// Returns the next runnable ThreadId.
    pub fn next_runnable(&mut self) -> Option<ThreadId> {
        self.runnable.dequeue()
    }

    pub fn active_count(&self) -> usize {
        self.registry.active_count()
    }

    pub fn get_exit_code(&self, id: ThreadId) -> Option<i32> {
        self.registry.get_exit_code(id)
    }

    #[cfg(test)]
    pub fn runnable_count(&self) -> usize {
        self.runnable.len()
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

    pub fn take_thread(&mut self, id: ThreadId) -> Option<VmThreadState> {
        self.registry.take(id)
    }

    pub fn state(&self, id: ThreadId) -> Option<crate::registry::ThreadState> {
        self.registry.state(id)
    }

    pub fn mark_running(&mut self, id: ThreadId) -> bool {
        self.registry.mark_running(id)
    }

    pub fn mark_exited(&mut self, id: ThreadId, thread: VmThreadState, code: i32) -> bool {
        self.registry.restore_vm_state(id, thread);
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
