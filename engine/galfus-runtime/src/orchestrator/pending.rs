use std::sync::{Arc, atomic::AtomicBool};

pub(crate) struct PendingContinuation {
    pub(crate) thread_id: crate::registry::ThreadId,
    pub(crate) continuation: galfus_vm::Continuation,
    pub(crate) module_id: galfus_core::ModuleId,
    pub(crate) return_type: galfus_bytecode::instruction::TypeIdx,
    pub(crate) stack: Vec<galfus_contract::ExecutionFrame>,
    pub(crate) operation: PendingOperation,
    pub(crate) active: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PendingKey {
    Request(galfus_core::RequestId),
    Future(galfus_core::FutureId),
    Coordinator(galfus_core::CoordinatorId),
}

pub(crate) enum PendingOperation {
    Future,
    AggregateMember {
        coordinator_id: galfus_core::CoordinatorId,
        index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LateCompletion {
    pub(crate) thread_id: crate::registry::ThreadId,
    pub(crate) key: PendingKey,
}

pub(crate) const MAX_LATE_COMPLETIONS: usize = 64;
