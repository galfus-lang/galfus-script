#[cfg(test)]
mod tests;

use super::pending::PendingContinuation;
use crate::event::FutureResult;
use crate::registry::ThreadId;
use galfus_bytecode::instruction::{FuncIdx, TypeIdx};
use galfus_contract::{BoundaryValue, ExecutionFailure, SurfaceValue};
use galfus_core::ModuleId;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone)]
pub enum ProviderArguments {
    Surface(Vec<SurfaceValue>),
}

#[derive(Debug, Clone)]
pub enum Activation {
    GalfusFunction {
        module_id: ModuleId,
        func_idx: FuncIdx,
        args: Vec<galfus_vm::VmValue>,
    },
    Internal {
        operation: String,
        args: Vec<BoundaryValue>,
    },
    Provider {
        alias: String,
        name: String,
        args: ProviderArguments,
        request_id: Option<galfus_core::RequestId>,
    },
    Adapter {
        proxy_module: String,
        symbol: String,
        args: Vec<BoundaryValue>,
        request_id: Option<galfus_core::RequestId>,
    },
}

impl Activation {
    /// Keeps only the data needed after dispatch for cancellation and completion bookkeeping.
    fn running_descriptor(&self) -> Self {
        match self {
            Self::GalfusFunction {
                module_id,
                func_idx,
                ..
            } => Self::GalfusFunction {
                module_id: *module_id,
                func_idx: *func_idx,
                args: Vec::new(),
            },
            Self::Internal { operation, .. } => Self::Internal {
                operation: operation.clone(),
                args: Vec::new(),
            },
            Self::Provider { alias, name, .. } => Self::Provider {
                alias: alias.clone(),
                name: name.clone(),
                args: ProviderArguments::Surface(Vec::new()),
                request_id: None,
            },
            Self::Adapter {
                proxy_module,
                symbol,
                ..
            } => Self::Adapter {
                proxy_module: proxy_module.clone(),
                symbol: symbol.clone(),
                args: Vec::new(),
                request_id: None,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum FutureState {
    Created(Activation),
    Running(Activation),
    Resolved(FutureResult),
    Discarded,
}

pub struct Waiter {
    pub continuation: PendingContinuation,
}

enum Waiters {
    Public {
        first: Option<Waiter>,
        rest: Vec<Waiter>,
    },
    Direct(Option<Waiter>),
}

impl Waiters {
    fn is_empty(&self) -> bool {
        match self {
            Self::Public { first, .. } => first.is_none(),
            Self::Direct(waiter) => waiter.is_none(),
        }
    }

    fn push(&mut self, waiter: Waiter) -> Result<(), ExecutionFailure> {
        match self {
            Self::Public { first, rest } => {
                if first.is_none() {
                    *first = Some(waiter);
                } else {
                    rest.push(waiter);
                }
                Ok(())
            }
            Self::Direct(slot) if slot.is_none() => {
                *slot = Some(waiter);
                Ok(())
            }
            Self::Direct(_) => Err(ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InvalidContinuation,
                "direct-await future already has a waiter",
            )),
        }
    }

    fn take(&mut self) -> Vec<Waiter> {
        match self {
            Self::Public { first, rest } => {
                let mut waiters = Vec::with_capacity(first.is_some() as usize + rest.len());
                if let Some(waiter) = first.take() {
                    waiters.push(waiter);
                }
                waiters.append(rest);
                waiters
            }
            Self::Direct(waiter) => waiter.take().into_iter().collect(),
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Public { first, rest } => {
                *first = None;
                rest.clear();
            }
            Self::Direct(waiter) => *waiter = None,
        }
    }

    fn is_direct(&self) -> bool {
        matches!(self, Self::Direct(_))
    }
}

pub enum WaitDisposition {
    Registered,
    Resolved {
        waiter: Waiter,
        result: FutureResult,
    },
    Discarded,
}

pub enum DiscardDisposition {
    Created(Activation),
    Running(Activation),
    Retained,
    Terminal,
}

pub struct FutureRecord {
    pub payload_type: Option<TypeIdx>,
    pub payload_module_id: Option<ModuleId>,
    pub state: FutureState,
    waiters: Waiters,
    active: Option<Arc<AtomicBool>>,
}

#[derive(Default)]
pub struct FutureRegistry {
    records: HashMap<(ThreadId, galfus_core::FutureId), FutureRecord>,
    tombstones: std::collections::VecDeque<(ThreadId, galfus_core::FutureId)>,
    tombstone_index: std::collections::HashSet<(ThreadId, galfus_core::FutureId)>,
}

impl FutureRegistry {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            tombstones: std::collections::VecDeque::new(),
            tombstone_index: std::collections::HashSet::new(),
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
            active: matches!(
                activation,
                Activation::Provider { .. } | Activation::Adapter { .. }
            )
            .then(|| Arc::new(AtomicBool::new(true))),
            state: FutureState::Created(activation),
            waiters: Waiters::Public {
                first: None,
                rest: Vec::new(),
            },
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

    pub fn insert_direct_await(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
        payload_type: TypeIdx,
        payload_module_id: ModuleId,
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
        self.records.insert(
            (owner_thread_id, future_id),
            FutureRecord {
                payload_type: Some(payload_type),
                payload_module_id: Some(payload_module_id),
                active: None,
                state: FutureState::Created(activation),
                waiters: Waiters::Direct(None),
            },
        );
        Ok(())
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
        match &mut record.state {
            FutureState::Created(_) => {
                let activation = match std::mem::replace(&mut record.state, FutureState::Discarded)
                {
                    FutureState::Created(activation) => activation,
                    _ => unreachable!(),
                };
                record.state = FutureState::Running(activation.running_descriptor());
                Ok(Some(activation))
            }
            FutureState::Running(_) | FutureState::Resolved(_) | FutureState::Discarded => Ok(None),
        }
    }

    pub fn take_inline_galfus_activation(
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

        if !matches!(
            record.state,
            FutureState::Created(Activation::GalfusFunction { .. })
        ) {
            return Ok(None);
        }

        let activation = match std::mem::replace(&mut record.state, FutureState::Discarded) {
            FutureState::Created(activation @ Activation::GalfusFunction { .. }) => activation,
            _ => unreachable!(),
        };
        record.state = FutureState::Running(activation.running_descriptor());
        Ok(Some(activation))
    }

    pub fn adapter_proxy_module(
        &self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> Option<String> {
        let record = self.records.get(&(owner_thread_id, future_id))?;
        match &record.state {
            FutureState::Running(Activation::Adapter { proxy_module, .. }) => {
                Some(proxy_module.clone())
            }
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
        let activation = match &mut record.state {
            FutureState::Running(activation) => activation,
            _ => {
                return Err(ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    "future request id was assigned before activation started",
                )
                .with_thread_id(owner_thread_id)
                .with_future_id(future_id));
            }
        };
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

    pub fn request_id(
        &self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> Option<galfus_core::RequestId> {
        let record = self.records.get(&(owner_thread_id, future_id))?;
        match &record.state {
            FutureState::Running(Activation::Provider { request_id, .. })
            | FutureState::Running(Activation::Adapter { request_id, .. }) => *request_id,
            _ => None,
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
            FutureState::Created(_) | FutureState::Running(_) => record
                .waiters
                .push(waiter)
                .map(|()| WaitDisposition::Registered),
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
                if let Some(active) = &record.active {
                    active.store(false, Ordering::Release);
                }
                record.waiters.clear();
                self.record_tombstone(owner, future_id);
                let activation = match record.state {
                    FutureState::Running(activation) => Some(activation),
                    _ => None,
                };
                Some((future_id, activation))
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

    pub(super) fn discard_inner(
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
                FutureState::Created(_) => {
                    let activation =
                        match std::mem::replace(&mut record.state, FutureState::Discarded) {
                            FutureState::Created(activation) => activation,
                            _ => unreachable!(),
                        };
                    if let Some(active) = &record.active {
                        active.store(false, Ordering::Release);
                    }
                    (true, Ok(DiscardDisposition::Created(activation)))
                }
                FutureState::Running(_) => {
                    let activation =
                        match std::mem::replace(&mut record.state, FutureState::Discarded) {
                            FutureState::Running(activation) => activation,
                            _ => unreachable!(),
                        };
                    if let Some(active) = &record.active {
                        active.store(false, Ordering::Release);
                    }
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

    pub fn complete<V: Into<crate::event::FutureValue>>(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
        result: Result<V, ExecutionFailure>,
    ) -> Result<Vec<Waiter>, ExecutionFailure> {
        let result = result.map(Into::into);
        let record = match self.records.get_mut(&(owner_thread_id, future_id)) {
            Some(r) => r,
            None => {
                if self.tombstone_index.contains(&(owner_thread_id, future_id)) {
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
        record.state = FutureState::Resolved(result);
        Ok(record.waiters.take())
    }

    pub fn is_direct_await(
        &self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) -> bool {
        self.records
            .get(&(owner_thread_id, future_id))
            .is_some_and(|record| record.waiters.is_direct())
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
            .and_then(|record| record.active.clone())
    }

    pub(super) fn record_tombstone(
        &mut self,
        owner_thread_id: ThreadId,
        future_id: galfus_core::FutureId,
    ) {
        if self.tombstones.len() >= 1024
            && let Some(tombstone) = self.tombstones.pop_front()
        {
            self.tombstone_index.remove(&tombstone);
        }
        let tombstone = (owner_thread_id, future_id);
        self.tombstones.push_back(tombstone);
        self.tombstone_index.insert(tombstone);
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
