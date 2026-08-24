#[cfg(test)]
mod tests;

pub(crate) mod adapter;
pub(crate) mod cancellation;
pub(crate) mod effects;
pub(crate) mod future_registry;
pub(crate) mod pending;
pub(crate) mod startup;

use crate::driver::{ExecutionDriver, RuntimeEventSink};
use crate::event::{EventSequence, RuntimeEvent};
use crate::execution::{CancellationReport, CompletionMetrics};
use crate::kernel::VirtualKernel;
use crate::task::execution_stack;
use galfus_contract::{AdapterBindingsCloseReport, ExecutionFailure};
use galfus_core::{CoordinatorId, FutureId, RequestId};
use galfus_vm::VirtualMachine;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use future_registry::FutureRegistry;
use pending::{LateCompletion, PendingContinuation, PendingKey};
pub(crate) use startup::StartupPlan;

pub(crate) mod adapter_handles;
pub(crate) mod aggregates;
pub(crate) mod completion;
pub(crate) mod event_loop;
pub(crate) mod future_waits;
pub(crate) mod lifecycle;
pub(crate) mod state;

pub(crate) use aggregates::{AggregateCoordinator, AggregateMode};
pub(crate) use future_waits::{MailboxFutureWait, TimerFutureWait};
pub(crate) struct Orchestrator {
    kernel: VirtualKernel,
    driver: Option<Rc<dyn ExecutionDriver>>,
    event_sink: Option<Arc<dyn RuntimeEventSink>>,
    pending_events: BTreeMap<EventSequence, RuntimeEvent>,
    next_event_sequence: EventSequence,
    active_event_sequence: Option<EventSequence>,
    pending_aggregate_finishes: BTreeSet<CoordinatorId>,
    vm: Option<Arc<VirtualMachine>>,
    /// Keeps orchestration state owned by exactly one execution lane.
    _not_send_sync: PhantomData<Rc<()>>,
    pub(crate) failure: Option<galfus_contract::ExecutionFailure>,
    pending_continuations: HashMap<PendingKey, PendingContinuation>,
    startup_plans: HashMap<crate::registry::ThreadId, StartupPlan>,
    request_id_manager: galfus_core::id_manager::LocalIdManager<RequestId>,
    request_generations: HashMap<u32, u32>,
    future_id_manager: galfus_core::id_manager::LocalIdManager<FutureId>,
    future_generations: HashMap<u32, u32>,
    coordinator_id_manager: galfus_core::id_manager::LocalIdManager<CoordinatorId>,
    adapter_bindings: Option<Arc<std::sync::Mutex<galfus_contract::AdapterBindings>>>,
    initialization_complete: Arc<AtomicBool>,
    shutting_down: bool,
    shutdown_report: Option<AdapterBindingsCloseReport>,
    cancellation_report: CancellationReport,
    completion_metrics: CompletionMetrics,
    late_completions: VecDeque<LateCompletion>,
    root_thread_id: Option<crate::registry::ThreadId>,
    future_workers:
        HashMap<crate::registry::ThreadId, (crate::registry::ThreadId, galfus_core::FutureLease)>,
    thread_exit_waits: HashMap<
        crate::registry::ThreadId,
        Vec<(crate::registry::ThreadId, galfus_core::FutureLease)>,
    >,
    mailbox_future_waits: HashMap<crate::registry::ThreadId, VecDeque<MailboxFutureWait>>,
    mailbox_future_wait_targets: HashMap<
        (crate::registry::ThreadId, galfus_core::FutureId),
        (crate::registry::ThreadId, galfus_core::FutureLease),
    >,
    timer_future_waits: BTreeSet<TimerFutureWait>,
    virtual_time_ms: u64,
    pub(crate) future_registry: FutureRegistry,
    aggregate_coordinators: HashMap<galfus_core::CoordinatorId, AggregateCoordinator>,
    aggregate_registration: Option<(galfus_core::CoordinatorId, usize)>,
    quota: std::sync::Arc<std::sync::Mutex<galfus_vm::quota::GlobalQuota>>,
}
