#[cfg(test)]
mod tests;

use super::pending::PendingContinuation;
use crate::registry::ThreadId;
use galfus_bytecode::instruction::{FuncIdx, TypeIdx};
use galfus_contract::{BoundaryValue, ExecutionFailure};
use galfus_core::ModuleId;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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
    },
    Provider {
        name: String,
        args: Vec<BoundaryValue>,
        request_id: Option<galfus_core::RequestId>,
    },
    Adapter {
        proxy_module: String,
        symbol: String,
        args: Vec<BoundaryValue>,
        request_id: Option<galfus_core::RequestId>,
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

pub enum DiscardDisposition {
    Created,
    Running(Activation),
    Retained,
    Terminal,
}

pub struct FutureRecord {
    pub payload_type: Option<TypeIdx>,
    pub payload_module_id: Option<ModuleId>,
    pub activation: Option<Activation>,
    pub running_activation: Option<Activation>,
    pub state: FutureState,
    pub waiters: Vec<Waiter>,
    pub active: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct FutureRegistry {
    records: HashMap<(ThreadId, galfus_core::FutureId), FutureRecord>,
    tombstones: std::collections::VecDeque<(ThreadId, galfus_core::FutureId)>,
}

impl FutureRegistry {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            tombstones: std::collections::VecDeque::new(),
        }
    }

    pub fn create(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
        payload_type: Option<TypeIdx>,
        payload_module_id: Option<ModuleId>,
        activation: Activation,
    ) -> Result<(), ExecutionFailure> {
        if self.records.contains_key(&(owner_thread_id, future_id)) {
            return Err(ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::DuplicateCompletion,
                "duplicate future id for owner thread",
            )
            .with_thread_id(owner_thread_id)
            .with_future_id(future_id));
        }
        let record = FutureRecord {
            payload_type,
            payload_module_id,
            activation: Some(activation),
            running_activation: None,
            state: FutureState::Created,
            waiters: Vec::new(),
            active: Arc::new(AtomicBool::new(true)),
        };
        self.records.insert((owner_thread_id, future_id), record);
        Ok(())
    }

    pub fn insert_created(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
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
        future_id: galfus_core::FutureId,
    ) -> Result<Option<Activation>, ExecutionFailure> {
        let record = self
            .records
            .get_mut(&(owner_thread_id, future_id))
            .ok_or_else(|| {
                ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "unknown future",
                )
                .with_thread_id(owner_thread_id)
                .with_future_id(future_id)
            })?;
        match record.state {
            FutureState::Created => {
                record.state = FutureState::Running;
                let activation = record.activation.take();
                record.running_activation = activation.clone();
                Ok(activation)
            }
            FutureState::Running | FutureState::Resolved(_) | FutureState::Discarded => Ok(None),
        }
    }

    pub fn adapter_proxy_module(
        &self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> Option<String> {
        let record = self.records.get(&(owner_thread_id, future_id))?;
        match record.running_activation.as_ref()? {
            Activation::Adapter { proxy_module, .. } => Some(proxy_module.clone()),
            _ => None,
        }
    }

    pub fn assign_request_id(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
        request_id: galfus_core::RequestId,
    ) -> Result<(), ExecutionFailure> {
        let record = self
            .records
            .get_mut(&(owner_thread_id, future_id))
            .ok_or_else(|| {
                ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "unknown future while assigning request id",
                )
                .with_thread_id(owner_thread_id)
                .with_future_id(future_id)
            })?;
        let activation = record.running_activation.as_mut().ok_or_else(|| {
            ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InvalidContinuation,
                "future request id was assigned before activation started",
            )
            .with_thread_id(owner_thread_id)
            .with_future_id(future_id)
        })?;
        match activation {
            Activation::Provider {
                request_id: active_request_id,
                ..
            }
            | Activation::Adapter {
                request_id: active_request_id,
                ..
            } => {
                *active_request_id = Some(request_id);
                Ok(())
            }
            Activation::GalfusFunction { .. } | Activation::Internal { .. } => {
                Err(ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "future activation does not dispatch an external request",
                )
                .with_thread_id(owner_thread_id)
                .with_future_id(future_id))
            }
        }
    }

    pub fn add_waiter(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
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
                .with_thread_id(owner_thread_id)
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
        future_id: galfus_core::FutureId,
    ) -> Result<DiscardDisposition, ExecutionFailure> {
        self.discard_inner(owner_thread_id, future_id, false)
    }

    pub fn discard_for_race(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> Result<DiscardDisposition, ExecutionFailure> {
        self.discard_inner(owner_thread_id, future_id, true)
    }

    pub fn discard_all_for_owner(
        &mut self,
        owner_thread_id: ThreadId,
    ) -> Vec<(galfus_core::FutureId, Option<Activation>)> {
        let mut keys = self
            .records
            .keys()
            .filter(|(owner, _)| *owner == owner_thread_id)
            .copied()
            .collect::<Vec<_>>();
        keys.sort_unstable();

        keys.into_iter()
            .filter_map(|(owner, future_id)| {
                let mut record = self.records.remove(&(owner, future_id))?;
                record.active.store(false, Ordering::Release);
                record.activation = None;
                record.waiters.clear();
                self.record_tombstone(owner, future_id);
                Some((future_id, record.running_activation.take()))
            })
            .collect()
    }

    pub fn discard_all(&mut self) -> Vec<(ThreadId, galfus_core::FutureId, Option<Activation>)> {
        let mut owners = self
            .records
            .keys()
            .map(|(owner, _)| *owner)
            .collect::<std::collections::HashSet<_>>();
        let mut owners = owners.drain().collect::<Vec<_>>();
        owners.sort_unstable();
        owners
            .into_iter()
            .flat_map(|owner| {
                self.discard_all_for_owner(owner)
                    .into_iter()
                    .map(move |(future_id, activation)| (owner, future_id, activation))
            })
            .collect()
    }

    fn discard_inner(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
        force: bool,
    ) -> Result<DiscardDisposition, ExecutionFailure> {
        let (is_terminal, res) = {
            let record = self
                .records
                .get_mut(&(owner_thread_id, future_id))
                .ok_or_else(|| {
                    ExecutionFailure::new(
                        galfus_contract::ExecutionFailureKind::InvalidContinuation,
                        "unknown future",
                    )
                    .with_thread_id(owner_thread_id)
                    .with_future_id(future_id)
                })?;
            if !force && !record.waiters.is_empty() {
                return Ok(DiscardDisposition::Retained);
            }
            match record.state {
                FutureState::Created => {
                    record.activation = None;
                    record.active.store(false, Ordering::Release);
                    record.state = FutureState::Discarded;
                    (true, Ok(DiscardDisposition::Created))
                }
                FutureState::Running => {
                    let activation = record.running_activation.take().ok_or_else(|| {
                        ExecutionFailure::new(
                            galfus_contract::ExecutionFailureKind::InvalidContinuation,
                            "running future has no activation descriptor",
                        )
                        .with_thread_id(owner_thread_id)
                        .with_future_id(future_id)
                    })?;
                    record.state = FutureState::Discarded;
                    record.active.store(false, Ordering::Release);
                    (true, Ok(DiscardDisposition::Running(activation)))
                }
                FutureState::Resolved(_) | FutureState::Discarded => {
                    (true, Ok(DiscardDisposition::Terminal))
                }
            }
        };

        if is_terminal {
            self.records.remove(&(owner_thread_id, future_id));
            self.record_tombstone(owner_thread_id, future_id);
        }
        res
    }

    pub fn complete(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) -> Result<Vec<Waiter>, ExecutionFailure> {
        let record = match self.records.get_mut(&(owner_thread_id, future_id)) {
            Some(r) => r,
            None => {
                if self.tombstones.contains(&(owner_thread_id, future_id)) {
                    return Err(ExecutionFailure::new(
                        galfus_contract::ExecutionFailureKind::DuplicateCompletion,
                        "future completed after being discarded",
                    )
                    .with_thread_id(owner_thread_id)
                    .with_future_id(future_id));
                } else {
                    return Err(ExecutionFailure::new(
                        galfus_contract::ExecutionFailureKind::InvalidContinuation,
                        "unknown future completion",
                    )
                    .with_thread_id(owner_thread_id)
                    .with_future_id(future_id));
                }
            }
        };
        if matches!(
            record.state,
            FutureState::Resolved(_) | FutureState::Discarded
        ) {
            return Err(ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::DuplicateCompletion,
                "future completed after reaching a terminal state",
            )
            .with_thread_id(owner_thread_id)
            .with_future_id(future_id));
        }
        record.activation = None;
        record.running_activation = None;
        record.state = FutureState::Resolved(result);
        Ok(std::mem::take(&mut record.waiters))
    }

    pub fn payload_schema(
        &self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> Option<(ModuleId, TypeIdx)> {
        let record = self.records.get(&(owner_thread_id, future_id))?;
        Some((record.payload_module_id?, record.payload_type?))
    }

    pub fn active_flag(
        &self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> Option<Arc<AtomicBool>> {
        self.records
            .get(&(owner_thread_id, future_id))
            .map(|record| record.active.clone())
    }

    fn record_tombstone(&mut self, owner_thread_id: ThreadId, future_id: galfus_core::FutureId) {
        if self.tombstones.len() >= 1024 {
            self.tombstones.pop_front();
        }
        self.tombstones.push_back((owner_thread_id, future_id));
    }

    #[cfg(test)]
    pub fn get(
        &self,
        thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> Option<&FutureRecord> {
        self.records.get(&(thread_id, future_id))
    }
}
