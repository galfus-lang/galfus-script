#[cfg(test)]
mod tests;

use crate::registry::ThreadId;
use galfus_contract::{ExecutionFailure, SurfaceContract, SurfaceValue};
use galfus_vm::thread::VmThreadState;
use galfus_vm::{Continuation, VmEffect};

/// Heap-independent completion data retained until the owning continuation resumes.
#[derive(Debug, Clone, PartialEq)]
pub enum FutureValue {
    I32(i32),
    I64(i64),
    F64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Null,
    Function {
        module_id: u32,
        func_idx: u32,
    },
    Surface {
        contract: SurfaceContract,
        value: SurfaceValue,
    },
    Aggregate(Vec<Self>),
}

pub type FutureResult = Result<FutureValue, ExecutionFailure>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSequence(pub u64);

impl EventSequence {
    pub const FIRST: Self = Self(1);

    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

pub enum RuntimeEvent {
    /// A thread created outside the kernel must be registered on the main thread.
    ThreadSpawned {
        thread: VmThreadState,
    },
    /// A thread encountered a VM effect that requires kernel intervention.
    Syscall {
        thread_id: ThreadId,
        thread: VmThreadState,
        effect: VmEffect,
        continuation: Continuation,
    },
    /// A thread has completed its execution naturally.
    Exited {
        thread_id: ThreadId,
        thread: VmThreadState,
        result: Result<i32, galfus_contract::ExecutionFailure>,
    },
    /// A module initializer completed and the startup sequence can advance.
    Initialized {
        thread_id: ThreadId,
        thread: VmThreadState,
        module_id: galfus_core::ModuleId,
    },
    /// A thread panicked or encountered a fatal error.
    Failed {
        thread_id: ThreadId,
        error: galfus_contract::ExecutionFailure,
    },
    /// A thread exhausted its budget and yielded to the kernel.
    Yielded {
        thread_id: ThreadId,
        thread: VmThreadState,
    },
    /// Completes a previously suspended provider effect.
    EffectCompleted {
        thread_id: ThreadId,
        request_lease: galfus_core::RequestLease,
        contract: galfus_contract::SurfaceContract,
        result: Result<galfus_contract::SurfaceValue, galfus_contract::ExecutionFailure>,
    },
    /// Completes a previously suspended future effect.
    FutureCompleted {
        thread_id: ThreadId,
        future_lease: galfus_core::FutureLease,
        result: FutureResult,
    },
    /// A dedicated worker completed a Galfus future activation.
    FutureWorkerCompleted {
        worker_thread_id: ThreadId,
        owner_thread_id: ThreadId,
        future_lease: galfus_core::FutureLease,
        thread: VmThreadState,
        result: Result<i32, galfus_contract::ExecutionFailure>,
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
