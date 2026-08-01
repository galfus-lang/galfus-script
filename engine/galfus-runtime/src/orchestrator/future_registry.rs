#[cfg(test)]
mod tests;

use super::pending::PendingContinuation;
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
    Discarded,
}

pub struct Waiter {
    pub continuation: PendingContinuation,
}

pub enum WaitDisposition {
    Registered,
    Resolved {
        waiter: Waiter,
        result: Result<BoundaryValue, ExecutionFailure>,
    },
    Discarded,
}

pub struct FutureRecord {
    pub owner_thread_id: ThreadId,
    pub future_id: u64,
    pub payload_type: Option<TypeIdx>,
    pub payload_module_id: Option<ModuleId>,
    pub activation: Option<Activation>,
    pub state: FutureState,
    pub waiters: Vec<Waiter>,
}

#[derive(Default)]
pub struct FutureRegistry {
    records: HashMap<(ThreadId, u64), FutureRecord>,
}

impl FutureRegistry {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    pub fn create(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
        payload_type: Option<TypeIdx>,
        payload_module_id: Option<ModuleId>,
        activation: Activation,
    ) -> Result<(), ExecutionFailure> {
        if self.records.contains_key(&(owner_thread_id, future_id)) {
            return Err(ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::DuplicateCompletion,
                "duplicate future id for owner thread",
            )
            .with_thread_id(owner_thread_id.raw())
            .with_future_id(future_id));
        }
        let record = FutureRecord {
            owner_thread_id,
            future_id,
            payload_type,
            payload_module_id,
            activation: Some(activation),
            state: FutureState::Created,
            waiters: Vec::new(),
        };
        self.records.insert((owner_thread_id, future_id), record);
        Ok(())
    }

    pub fn insert_created(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
        payload_type: Option<TypeIdx>,
        payload_module_id: Option<ModuleId>,
        activation: Activation,
    ) -> Result<(), ExecutionFailure> {
        self.create(
            owner_thread_id,
            future_id,
            payload_type,
            payload_module_id,
            activation,
        )
    }

    pub fn take_activation_for_start(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
    ) -> Result<Option<Activation>, ExecutionFailure> {
        let record = self
            .records
            .get_mut(&(owner_thread_id, future_id))
            .ok_or_else(|| {
                ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "unknown future",
                )
                .with_thread_id(owner_thread_id.raw())
                .with_future_id(future_id)
            })?;
        match record.state {
            FutureState::Created => {
                record.state = FutureState::Running;
                Ok(record.activation.take())
            }
            FutureState::Running | FutureState::Resolved(_) | FutureState::Discarded => Ok(None),
        }
    }

    pub fn add_waiter(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
        waiter: Waiter,
    ) -> Result<WaitDisposition, ExecutionFailure> {
        let record = self
            .records
            .get_mut(&(owner_thread_id, future_id))
            .ok_or_else(|| {
                ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "unknown or foreign future",
                )
                .with_thread_id(owner_thread_id.raw())
                .with_future_id(future_id)
            })?;
        match &record.state {
            FutureState::Resolved(result) => Ok(WaitDisposition::Resolved {
                waiter,
                result: result.clone(),
            }),
            FutureState::Discarded => Ok(WaitDisposition::Discarded),
            FutureState::Created | FutureState::Running => {
                record.waiters.push(waiter);
                Ok(WaitDisposition::Registered)
            }
        }
    }

    pub fn discard(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
    ) -> Result<(), ExecutionFailure> {
        let record = self
            .records
            .get_mut(&(owner_thread_id, future_id))
            .ok_or_else(|| {
                ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "unknown future",
                )
                .with_thread_id(owner_thread_id.raw())
                .with_future_id(future_id)
            })?;
        if matches!(record.state, FutureState::Created) {
            record.activation = None;
            record.state = FutureState::Discarded;
        }
        Ok(())
    }

    pub fn complete(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) -> Result<Vec<Waiter>, ExecutionFailure> {
        let record = self
            .records
            .get_mut(&(owner_thread_id, future_id))
            .ok_or_else(|| {
                ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "unknown future completion",
                )
                .with_thread_id(owner_thread_id.raw())
                .with_future_id(future_id)
            })?;
        if matches!(
            record.state,
            FutureState::Resolved(_) | FutureState::Discarded
        ) {
            return Err(ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::DuplicateCompletion,
                "future completed after reaching a terminal state",
            )
            .with_thread_id(owner_thread_id.raw())
            .with_future_id(future_id));
        }
        record.activation = None;
        record.state = FutureState::Resolved(result);
        Ok(std::mem::take(&mut record.waiters))
    }

    pub fn insert_resolved(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: u64,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) -> Result<(), ExecutionFailure> {
        self.create(
            owner_thread_id,
            future_id,
            None,
            None,
            Activation::Internal {
                operation: "intrinsic".to_string(),
                args: vec![],
                arg_types: vec![],
            },
        )?;
        let _waiters = self.complete(owner_thread_id, future_id, result)?;
        Ok(())
    }

    pub fn payload_schema(
        &self,
        owner_thread_id: ThreadId,
        future_id: u64,
    ) -> Option<(ModuleId, TypeIdx)> {
        let record = self.records.get(&(owner_thread_id, future_id))?;
        Some((record.payload_module_id?, record.payload_type?))
    }

    pub fn get(&self, thread_id: ThreadId, future_id: u64) -> Option<&FutureRecord> {
        self.records.get(&(thread_id, future_id))
    }
}
