use crate::registry::ThreadId;
use galfus_vm::thread::VirtualThread;
use galfus_vm::{Continuation, VmEffect};
use std::sync::mpsc;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
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
    /// A thread panicked or encountered a fatal error.
    Failed {
        thread_id: ThreadId,
        error: galfus_contract::ExecutionFailure,
    },
    /// Completes a previously suspended provider effect.
    EffectCompleted {
        thread_id: ThreadId,
        continuation: Continuation,
        result: Result<galfus_contract::BoundaryValue, galfus_contract::ExecutionFailure>,
    },
    CancelThread {
        thread_id: ThreadId,
    },
}

#[derive(Clone)]
pub struct EventSink {
    sender: mpsc::Sender<RuntimeEvent>,
    pending: Arc<AtomicUsize>,
}

impl EventSink {
    pub fn new(sender: mpsc::Sender<RuntimeEvent>) -> Self {
        Self {
            sender,
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn send(&self, event: RuntimeEvent) {
        self.pending.fetch_add(1, Ordering::Release);
        if self.sender.send(event).is_err() {
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
