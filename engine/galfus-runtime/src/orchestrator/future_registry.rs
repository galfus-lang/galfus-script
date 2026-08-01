use crate::registry::ThreadId;
use galfus_bytecode::instruction::{FuncIdx, TypeIdx};
use galfus_contract::{BoundaryValue, ExecutionFailure};
use galfus_core::ModuleId;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Activation {
    GalfusFunction {
        module_id: ModuleId,
        func_idx: FuncIdx,
        args: Vec<BoundaryValue>,
        arg_types: Vec<TypeIdx>,
    },
    Internal {
        operation: String,
        args: Vec<BoundaryValue>,
        arg_types: Vec<TypeIdx>,
    },
    Provider {
        name: String,
        args: Vec<BoundaryValue>,
        arg_types: Vec<TypeIdx>,
    },
    Adapter {
        proxy_module: String,
        symbol: String,
        args: Vec<BoundaryValue>,
        arg_types: Vec<TypeIdx>,
    },
}

#[derive(Debug, Clone)]
pub enum FutureState {
    Created,
    Running,
    Resolved(Result<BoundaryValue, ExecutionFailure>),
    Failed(ExecutionFailure),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Waiter {
    pub thread_id: ThreadId,
    pub future_id: u64,
}

#[derive(Debug, Clone)]
pub struct FutureRecord {
    pub owner_thread_id: ThreadId,
    pub future_id: u64,
    pub payload_type: Option<TypeIdx>,
    pub activation: Option<Activation>,
    pub state: FutureState,
    pub waiters: Vec<Waiter>,
}

#[derive(Default, Debug)]
pub struct FutureRegistry {
    records: HashMap<(ThreadId, u64), FutureRecord>,
}

impl FutureRegistry {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    pub fn insert_created(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
        payload_type: Option<TypeIdx>,
        activation: Activation,
    ) {
        let record = FutureRecord {
            owner_thread_id,
            future_id,
            payload_type,
            activation: Some(activation),
            state: FutureState::Created,
            waiters: Vec::new(),
        };
        self.records.insert((owner_thread_id, future_id), record);
    }

    pub fn insert_resolved(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        // Fallback for intrinsics that resolve immediately before Phase E unified boundaries
        let record = FutureRecord {
            owner_thread_id,
            future_id,
            payload_type: None,
            activation: Some(Activation::Internal {
                operation: "intrinsic".to_string(),
                args: vec![],
                arg_types: vec![],
            }),
            state: FutureState::Resolved(result),
            waiters: Vec::new(),
        };
        self.records.insert((owner_thread_id, future_id), record);
    }

    pub fn get(&self, thread_id: ThreadId, future_id: u64) -> Option<&FutureRecord> {
        self.records.get(&(thread_id, future_id))
    }

    pub fn get_mut(&mut self, thread_id: ThreadId, future_id: u64) -> Option<&mut FutureRecord> {
        self.records.get_mut(&(thread_id, future_id))
    }

    pub fn remove(&mut self, thread_id: ThreadId, future_id: u64) -> Option<FutureRecord> {
        self.records.remove(&(thread_id, future_id))
    }
}
