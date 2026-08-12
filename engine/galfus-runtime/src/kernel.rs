#[cfg(test)]
mod tests;

use crate::queue::{BlockedQueue, RunnableQueue};
use crate::registry::{MailboxMessage, ThreadId, ThreadRegistry, ThreadState};
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};
use galfus_vm::thread::VmThreadState;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Manages thread lifecycle, scheduling queues, and timers.
pub struct VirtualKernel {
    thread_id_manager: galfus_core::id_manager::IdManager<ThreadId>,
    registry: ThreadRegistry,
    pub(crate) runnable: RunnableQueue,
    blocked: BlockedQueue,
}

impl VirtualKernel {
    pub fn new() -> Self {
        Self {
            thread_id_manager: galfus_core::id_manager::IdManager::new(1),
            registry: ThreadRegistry::new(),
            runnable: RunnableQueue::new(),
            blocked: BlockedQueue::new(),
        }
    }

    /// Allocates a new ThreadId and registers the thread as runnable.
    pub fn spawn(
        &mut self,
        thread: VmThreadState,
        key: Option<String>,
    ) -> Result<ThreadId, ExecutionFailure> {
        thread.quota().lock().unwrap().try_reserve_threads(1).map_err(|kind| {
            ExecutionFailure::new(kind, "max threads limit exceeded")
        })?;
        if !self.registry.key_is_available(key.as_deref()) {
            return Err(ExecutionFailure::new(
                ExecutionFailureKind::DuplicateThreadKey,
                format!(
                    "thread key '{}' is already registered",
                    key.as_deref().unwrap_or_default()
                ),
            ));
        }
        let id = self.thread_id_manager.try_allocate().ok_or_else(|| {
            ExecutionFailure::new(
                ExecutionFailureKind::IdSpaceExhausted,
                "thread id space exhausted",
            )
        })?;
        if let Err(error) = self.registry.register(id, thread, key) {
            self.thread_id_manager.free(id);
            return Err(error);
        }
        Ok(id)
    }

    /// Parks a currently running thread without blocking it.
    pub fn enqueue_runnable(&mut self, id: ThreadId, thread: VmThreadState) {
        self.registry.restore_vm_state(id, thread);
        self.runnable.enqueue(id);
    }

    pub fn enqueue_runnable_front(&mut self, id: ThreadId, thread: VmThreadState) {
        self.registry.restore_vm_state(id, thread);
        self.runnable.enqueue_front(id);
    }

    pub fn park_running(&mut self, id: ThreadId, thread: VmThreadState) {
        self.registry.restore_vm_state(id, thread);
    }

    /// Blocks a thread, optionally with a timeout.
    pub fn block(
        &mut self,
        id: ThreadId,
        thread: VmThreadState,
        timeout: Option<u64>,
    ) -> Result<(), ExecutionFailure> {
        self.registry.restore_vm_state(id, thread);
        if let Some(ms) = timeout {
            self.blocked.block_with_timeout(id, ms)?;
        } else {
            self.blocked.block(id);
        }
        Ok(())
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
        let removed = self.registry.cancel(id);
        if removed {
            self.thread_id_manager.free(id);
        }
        removed
    }

    /// Returns the next runnable ThreadId.
    #[allow(dead_code)]
    pub fn next_runnable(&mut self) -> Option<ThreadId> {
        self.runnable.dequeue()
    }

    pub fn next_runnable_detailed(&mut self) -> Option<(ThreadId, bool)> {
        self.runnable.dequeue_detailed()
    }

    pub fn active_count(&self) -> usize {
        self.registry.active_count()
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

    pub fn take_created_thread(&mut self, id: ThreadId) -> Option<VmThreadState> {
        self.registry.take_created(id)
    }

    pub fn state(&self, id: ThreadId) -> Option<crate::registry::ThreadState> {
        self.registry.state(id)
    }

    pub fn mark_spawned(&mut self, id: ThreadId) {
        self.registry.mark_spawned(id);
    }

    pub fn is_running(&self, id: ThreadId) -> bool {
        self.registry.is_running(id)
    }

    pub fn is_exited(&mut self, id: ThreadId) -> bool {
        self.registry.is_exited(id)
    }

    pub fn mark_running(&mut self, id: ThreadId) -> bool {
        self.registry.mark_running(id)
    }

    pub fn mark_exited(
        &mut self,
        id: ThreadId,
        thread: VmThreadState,
        result: Result<galfus_contract::BoundaryValue, galfus_contract::ExecutionFailure>,
    ) -> bool {
        self.registry.restore_vm_state(id, thread);
        self.registry.mark_exited(id, result)
    }

    pub fn lookup_key(&self, key: &str) -> Option<ThreadId> {
        self.registry.lookup_key(key)
    }

    pub fn get_mailbox(&self, id: ThreadId) -> Option<Arc<Mutex<VecDeque<MailboxMessage>>>> {
        self.registry.get_mailbox(id)
    }

    pub fn debug_states(&self) -> Vec<(ThreadId, ThreadState)> {
        self.registry.debug_states()
    }
}

impl Default for VirtualKernel {
    fn default() -> Self {
        Self::new()
    }
}
