#[cfg(test)]
mod tests;

use crate::registry::ThreadId;
use galfus_vm::thread::VirtualThread;
use galfus_vm::{Continuation, VmEffect};
use std::sync::mpsc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

pub enum RuntimeEvent {
    /// A thread created outside the kernel must be registered on the main thread.
    ThreadSpawned {
        thread: VirtualThread,
    },
    /// A thread encountered a VM effect that requires kernel intervention.
    Syscall {
        thread_id: ThreadId,
        thread: VirtualThread,
        effect: VmEffect,
        continuation: Continuation,
    },
    /// A thread has completed its execution naturally.
    Exited {
        thread_id: ThreadId,
        thread: VirtualThread,
        code: i32,
    },
    /// A module initializer completed and the startup sequence can advance.
    Initialized {
        thread_id: ThreadId,
        thread: VirtualThread,
        module_id: galfus_core::ModuleId,
    },
    /// A thread panicked or encountered a fatal error.
    Failed {
        thread_id: ThreadId,
        error: galfus_contract::ExecutionFailure,
    },
    /// Completes a previously suspended provider effect.
    EffectCompleted {
        thread_id: ThreadId,
        request_id: u64,
        result: Result<galfus_contract::BoundaryValue, galfus_contract::ExecutionFailure>,
    },
    /// Completes a previously suspended future effect.
    FutureCompleted {
        thread_id: ThreadId,
        future_id: u64,
        result: Result<galfus_contract::BoundaryValue, galfus_contract::ExecutionFailure>,
    },
    /// Advances the virtual clock for blocked threads.
    Tick {
        delta_ms: u64,
    },
    /// Requests coordinated shutdown of every thread in this execution.
    CancelExecution,
    CancelThread {
        thread_id: ThreadId,
    },
}

#[derive(Clone)]
pub struct EventSink {
    sender: mpsc::Sender<(u64, RuntimeEvent)>,
    pending: Arc<AtomicUsize>,
    next_event_id: Arc<AtomicU64>,
    send_lock: Arc<Mutex<()>>,
}

impl EventSink {
    pub fn new(sender: mpsc::Sender<(u64, RuntimeEvent)>) -> Self {
        Self {
            sender,
            pending: Arc::new(AtomicUsize::new(0)),
            next_event_id: Arc::new(AtomicU64::new(1)),
            send_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn send(&self, event: RuntimeEvent) {
        let _send_guard = self.send_lock.lock().unwrap();
        let event_id = self.next_event_id.fetch_add(1, Ordering::Relaxed);
        self.pending.fetch_add(1, Ordering::Release);
        if self.sender.send((event_id, event)).is_err() {
            self.pending.fetch_sub(1, Ordering::Release);
        }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire) != 0
    }

    pub(crate) fn mark_received(&self) {
        self.pending.fetch_sub(1, Ordering::AcqRel);
    }
}
