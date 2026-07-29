use crate::registry::ThreadId;
use galfus_vm::thread::VirtualThread;
use galfus_vm::{Continuation, VmEffect};
use std::sync::mpsc;

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
}

impl EventSink {
    pub fn new(sender: mpsc::Sender<RuntimeEvent>) -> Self {
        Self { sender }
    }

    pub fn send(&self, event: RuntimeEvent) {
        let _ = self.sender.send(event);
    }
}
