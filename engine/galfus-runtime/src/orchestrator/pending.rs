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
    Request(u64),
    Future(u64),
}

pub(crate) enum PendingOperation {
    Future,
    AggregateMember { coordinator_id: u64, index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LateCompletion {
    pub(crate) thread_id: crate::registry::ThreadId,
    pub(crate) request_id: u64,
}

pub(crate) const MAX_LATE_COMPLETIONS: usize = 64;
