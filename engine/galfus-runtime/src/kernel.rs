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
        mut thread: VmThreadState,
        key: Option<String>,
    ) -> Result<ThreadId, ExecutionFailure> {
        thread
            .mark_spawned()
            .map_err(|kind| ExecutionFailure::new(kind, "max threads limit exceeded"))?;
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
    pub fn enqueue_runnable(
        &mut self,
        id: ThreadId,
        mut thread: VmThreadState,
    ) -> Result<(), galfus_contract::ExecutionFailureKind> {
        let result = thread
            .global_quota()
            .lock()
            .unwrap()
            .try_reserve_runnable_threads(1);
        self.registry.restore_vm_state(id, thread);
        result?;
        self.runnable.enqueue(id);
        Ok(())
    }

    pub fn enqueue_runnable_front(
        &mut self,
        id: ThreadId,
        mut thread: VmThreadState,
    ) -> Result<(), galfus_contract::ExecutionFailureKind> {
        let result = thread
            .global_quota()
            .lock()
            .unwrap()
            .try_reserve_runnable_threads(1);
        self.registry.restore_vm_state(id, thread);
        result?;
        self.runnable.enqueue_front(id);
        Ok(())
    }

    pub fn park_running(&mut self, id: ThreadId, thread: VmThreadState) {
        self.registry.restore_vm_state(id, thread);
    }

    pub fn block(
        &mut self,
        id: ThreadId,
        thread: VmThreadState,
        timeout: Option<u64>,
    ) -> Result<(), ExecutionFailure> {
        let reserve_states = thread
            .global_quota()
            .lock()
            .unwrap()
            .try_reserve_pending_states(1);
        if let Err(e) = reserve_states {
            self.registry.restore_vm_state(id, thread);
            return Err(
                ExecutionFailure::new(e, "pending states limit exceeded").with_thread_id(id)
            );
        }

        if let Some(ms) = timeout {
            let reserve_result = thread.global_quota().lock().unwrap().try_reserve_timers(1);
            if let Err(e) = reserve_result {
                thread
                    .global_quota()
                    .lock()
                    .unwrap()
                    .release_pending_states(1);
                self.registry.restore_vm_state(id, thread);
                return Err(ExecutionFailure::new(e, "timers limit exceeded").with_thread_id(id));
            }
            let had_timer = self.blocked.block_with_timeout(id, ms)?;
            if had_timer {
                thread.global_quota().lock().unwrap().release_timers(1);
            }
        } else {
            let had_timer = self.blocked.block(id);
            if had_timer {
                thread.global_quota().lock().unwrap().release_timers(1);
            }
        }
        self.registry.restore_vm_state(id, thread);
        Ok(())
    }

    pub fn unblock(&mut self, id: ThreadId) -> Result<bool, galfus_contract::ExecutionFailureKind> {
        let (was_blocked, had_timer) = self.blocked.unblock(id);
        if was_blocked {
            if let Some(thread) = self.registry.take(id) {
                thread
                    .global_quota()
                    .lock()
                    .unwrap()
                    .release_pending_states(1);
                if had_timer {
                    thread.global_quota().lock().unwrap().release_timers(1);
                }
                self.enqueue_runnable(id, thread)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn cancel(&mut self, id: ThreadId) -> bool {
        let was_runnable = self.runnable.remove(id);
        let blocked_info = self.blocked.remove(id);
        let thread_opt = self.registry.take(id);

        if let Some(thread) = thread_opt {
            let mut gq = thread.global_quota().lock().unwrap();
            if was_runnable {
                gq.release_runnable_threads(1);
            }
            if let Some(had_timer) = blocked_info {
                gq.release_pending_states(1);
                if had_timer {
                    gq.release_timers(1);
                }
            }
        }

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
    pub fn tick(
        &mut self,
        delta_ms: u64,
    ) -> Vec<(ThreadId, Result<(), galfus_contract::ExecutionFailureKind>)> {
        let woke_up = self.blocked.tick_timeouts(delta_ms);
        let mut results = Vec::new();
        for &id in &woke_up {
            if let Some(thread) = self.registry.take(id) {
                thread.global_quota().lock().unwrap().release_timers(1);
                let result = self.enqueue_runnable(id, thread);
                results.push((id, result));
            }
        }
        results
    }

    // Pass-through methods for tasks

    pub fn take_thread(&mut self, id: ThreadId) -> Option<VmThreadState> {
        let thread = self.registry.take(id);
        if let Some(ref t) = thread {
            t.global_quota().lock().unwrap().release_runnable_threads(1);
        }
        thread
    }

    pub fn take_created_thread(&mut self, id: ThreadId) -> Option<VmThreadState> {
        self.registry.take_created(id)
    }

    pub fn state(&self, id: ThreadId) -> Option<crate::registry::ThreadState> {
        self.registry.state(id)
    }

    pub fn mark_spawned(
        &mut self,
        id: ThreadId,
    ) -> Result<(), galfus_contract::ExecutionFailureKind> {
        self.registry.mark_spawned(id)
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

    pub fn get_thread_quota(
        &self,
        id: ThreadId,
    ) -> Option<Arc<Mutex<galfus_vm::quota::ThreadQuota>>> {
        self.registry.get_thread_quota(id)
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
