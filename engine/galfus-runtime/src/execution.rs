#[cfg(test)]
mod tests;

use crate::driver::{ExecutionDriver, RuntimeEventSink};
use crate::event::{FutureValue, RuntimeEvent};
use galfus_contract::{
    AdapterBindingsCloseReport, ExecutionFailure, ExecutionFailureKind, ExecutorStepResult,
    SurfaceContract, SurfaceValue, ThreadResult,
};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A single-owner runtime execution.
///
/// The owner advances this state exclusively through `&mut self`. Host callbacks may use an
/// [`ExecutionHandle`] to submit events, but never receive mutable access to the runtime core.
/// The future `ExecutionHost` is responsible for owning this execution lane.
pub struct Execution {
    orchestrator: Option<crate::orchestrator::Orchestrator>,
    driver: Rc<dyn ExecutionDriver>,
    event_sink: std::sync::Arc<dyn RuntimeEventSink>,
    result: Option<Result<i32, ExecutionFailure>>,
    shutdown_report: Option<ShutdownReport>,
    state: ExecutionState,
    initialization_complete: Arc<AtomicBool>,
    exit_notified: bool,
    _single_owner: PhantomData<Rc<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    Created,
    Initializing,
    Running,
    Waiting,
    Closing,
    Closed,
}

/// The immutable outcome produced after all execution-owned resources are released.
#[derive(Debug, Clone, PartialEq)]
pub struct ShutdownReport {
    pub result: Result<i32, ExecutionFailure>,
    pub adapter_close: AdapterBindingsCloseReport,
    pub cancellations: CancellationReport,
    pub completions: CompletionMetrics,
    #[cfg(feature = "metrics")]
    pub futures: FutureMetrics,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CancellationReport {
    pub confirmed: usize,
    pub best_effort: usize,
    pub unsupported: usize,
    pub already_completed: usize,
}

impl CancellationReport {
    pub(crate) fn record(&mut self, outcome: galfus_contract::CancellationOutcome) {
        match outcome {
            galfus_contract::CancellationOutcome::Confirmed => self.confirmed += 1,
            galfus_contract::CancellationOutcome::BestEffort => self.best_effort += 1,
            galfus_contract::CancellationOutcome::Unsupported => self.unsupported += 1,
            galfus_contract::CancellationOutcome::AlreadyCompleted => self.already_completed += 1,
        }
    }

    pub fn has_unconfirmed(&self) -> bool {
        self.best_effort != 0 || self.unsupported != 0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionMetrics {
    pub accepted: usize,
    pub duplicate: usize,
    pub late_after_cancel: usize,
    pub unknown_request: usize,
}

#[cfg(feature = "metrics")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FutureMetrics {
    pub runtime_events: usize,
    pub thread_spawned_events: usize,
    pub thread_exited_events: usize,
    pub thread_failed_events: usize,
    pub initialized_events: usize,
    pub syscall_events: usize,
    pub yield_events: usize,
    pub effect_completed_events: usize,
    pub future_completed_events: usize,
    pub future_worker_completed_events: usize,
    pub tick_events: usize,
    pub cancellation_events: usize,
    pub dispatched_threads: usize,
    pub front_dispatched_threads: usize,
    pub resumed_continuations: usize,
    pub blocked_threads: usize,
    pub unblocked_threads: usize,
    pub mailbox_waits_registered: usize,
    pub mailbox_waits_completed: usize,
    pub mailbox_waits_timed_out: usize,
    pub timer_waits_registered: usize,
    pub timer_waits_completed: usize,
    pub created: usize,
    pub awaited: usize,
    pub dropped: usize,
    pub boundary_arguments: usize,
    pub yields: usize,
    pub galfus_activations: usize,
    pub internal_activations: usize,
    pub internal_immediate: usize,
    pub internal_suspended: usize,
    pub internal_await_immediate: usize,
    pub internal_await_suspended: usize,
    pub provider_activations: usize,
    pub adapter_activations: usize,
}
impl Execution {
    pub(crate) fn new(
        mut orchestrator: crate::orchestrator::Orchestrator,
        driver: Rc<dyn ExecutionDriver>,
        initialization_complete: Arc<AtomicBool>,
        is_initializing: bool,
    ) -> Self {
        let event_sink = driver.event_sink();
        orchestrator.set_event_sink(event_sink.clone());
        orchestrator.set_driver(driver.clone());
        Self {
            orchestrator: Some(orchestrator),
            driver,
            event_sink,
            result: None,
            shutdown_report: None,
            state: if is_initializing {
                ExecutionState::Initializing
            } else {
                ExecutionState::Created
            },
            initialization_complete,
            exit_notified: false,
            _single_owner: PhantomData,
        }
    }

    pub fn handle(&self) -> ExecutionHandle {
        ExecutionHandle {
            sink: self.event_sink.clone(),
        }
    }

    pub fn status(&self) -> ExecutionState {
        self.state
    }

    pub fn result(&self) -> Option<&Result<i32, ExecutionFailure>> {
        self.result.as_ref()
    }

    pub fn shutdown_report(&self) -> Option<&ShutdownReport> {
        self.shutdown_report.as_ref()
    }

    /// Advances virtual time; the change is applied by the execution owner on poll.
    pub fn tick_timeouts(&self, delta_ms: u64) {
        let _ = self.event_sink.submit(RuntimeEvent::Tick { delta_ms });
    }

    pub fn poll(&mut self, budget: usize) -> Result<ExecutorStepResult, ExecutionFailure> {
        if matches!(self.state, ExecutionState::Closed) {
            return self.step_result();
        }
        if matches!(self.state, ExecutionState::Created) {
            self.state = ExecutionState::Running;
        }
        if matches!(self.state, ExecutionState::Initializing)
            && self.initialization_complete.load(Ordering::Acquire)
        {
            self.state = ExecutionState::Running;
        }
        if let Some(orchestrator) = &mut self.orchestrator {
            match orchestrator.step(budget) {
                ThreadResult::Discarded => {
                    if let Some(failure) = orchestrator.failure.take() {
                        self.close_with(Err(failure));
                    }
                }
                ThreadResult::Completed(res) => {
                    self.close_with(res);
                }
                ThreadResult::Blocked { .. } => {
                    self.state = ExecutionState::Waiting;
                }
            }
        }
        if self.result.is_some() {
            return self.step_result();
        }
        let state = self.driver.step();
        if matches!(state, ExecutorStepResult::Completed(_)) && self.orchestrator.is_some() {
            return Ok(ExecutorStepResult::Running);
        }
        if matches!(state, ExecutorStepResult::Blocked { .. }) {
            self.state = ExecutionState::Waiting;
        }
        Ok(state)
    }

    pub fn run_until_blocked(&mut self) -> Result<ExecutorStepResult, ExecutionFailure> {
        loop {
            match self.poll(100)? {
                ExecutorStepResult::Running => continue,
                state => return Ok(state),
            }
        }
    }

    pub fn run_sync_to_completion(&mut self) -> Result<i32, ExecutionFailure> {
        loop {
            match self.poll(100)? {
                ExecutorStepResult::Completed(_) => {
                    return self.result.clone().unwrap_or_else(|| {
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::InternalRuntimeFailure,
                            "execution completed without yielding a result",
                        ))
                    });
                }
                ExecutorStepResult::Blocked { .. } => {
                    if !self.driver.has_pending_events() {
                        let failure_info = self
                            .orchestrator
                            .as_ref()
                            .and_then(|o| o.failure.as_ref())
                            .map(|f| f.message.as_str())
                            .unwrap_or("no failure recorded");
                        let states_info = self
                            .orchestrator
                            .as_ref()
                            .map(|o| format!("states={:?}", o.debug_states()))
                            .unwrap_or_default();
                        return Err(ExecutionFailure::new(
                            ExecutionFailureKind::InternalRuntimeFailure,
                            format!("execution is blocked ({failure_info}, {states_info})"),
                        ));
                    }
                }
                ExecutorStepResult::Running => {}
            }
        }
    }

    pub fn cancel(&mut self) {
        if matches!(
            self.state,
            ExecutionState::Created
                | ExecutionState::Initializing
                | ExecutionState::Running
                | ExecutionState::Waiting
        ) {
            let _ = self.event_sink.submit(RuntimeEvent::CancelExecution);
            self.state = ExecutionState::Closing;
        }
    }

    /// Releases every resource owned by this execution without waiting for a driver turn.
    pub fn shutdown(&mut self) -> ShutdownReport {
        if let Some(report) = &self.shutdown_report {
            return report.clone();
        }
        self.close_with(Err(ExecutionFailure::new(
            ExecutionFailureKind::Cancelled,
            "execution shut down before completion",
        )));
        let report = self
            .shutdown_report
            .clone()
            .expect("closing an execution produces a shutdown report");
        self.notify_exit(&report.result);
        report
    }

    fn close_with(&mut self, result: Result<i32, ExecutionFailure>) {
        if self.shutdown_report.is_some() {
            return;
        }
        self.state = ExecutionState::Closing;
        let mut orchestrator = self.orchestrator.take();
        let (adapter_close, orchestrator_cancellation_report, orchestrator_completion_metrics) =
            match orchestrator.as_mut() {
                None => (
                    AdapterBindingsCloseReport::default(),
                    CancellationReport::default(),
                    CompletionMetrics::default(),
                ),
                Some(orchestrator) => {
                    let adapter_close = orchestrator.shutdown();
                    (
                        adapter_close,
                        orchestrator.cancellation_report().clone(),
                        orchestrator.completion_metrics().clone(),
                    )
                }
            };
        #[cfg(feature = "metrics")]
        let orchestrator_future_metrics = orchestrator
            .as_ref()
            .map_or_else(FutureMetrics::default, |orchestrator| {
                orchestrator.future_metrics().clone()
            });
        let result = if adapter_close.is_complete() {
            result
        } else {
            let failure = ExecutionFailure::new(
                ExecutionFailureKind::AdapterCallFailure,
                format!(
                    "execution teardown failed to release {} adapter handle(s)",
                    adapter_close.failures.len()
                ),
            );
            Err(match result {
                Ok(_) => failure,
                Err(error) => failure.with_cause(error),
            })
        };
        self.result = Some(result.clone());
        self.shutdown_report = Some(ShutdownReport {
            result,
            adapter_close,
            cancellations: orchestrator_cancellation_report,
            completions: orchestrator_completion_metrics,
            #[cfg(feature = "metrics")]
            futures: orchestrator_future_metrics,
        });
        self.state = ExecutionState::Closed;
    }

    fn step_result(&mut self) -> Result<ExecutorStepResult, ExecutionFailure> {
        let result = self
            .result
            .as_ref()
            .expect("closed execution has a result")
            .clone();
        self.notify_exit(&result);
        match &result {
            Ok(code) => Ok(ExecutorStepResult::Completed(*code)),
            Err(error) => Err(error.clone()),
        }
    }

    fn notify_exit(&mut self, result: &Result<i32, ExecutionFailure>) {
        if self.exit_notified {
            return;
        }
        self.exit_notified = true;
        self.driver.complete(match result {
            Ok(code) => Ok(*code),
            Err(error) => Err(error.clone()),
        });
    }
}

impl Drop for Execution {
    fn drop(&mut self) {
        if self.shutdown_report.is_none() {
            let _report = self.shutdown();
        }
    }
}

/// Thread-safe ingress for external integrations.
///
/// This handle queues requests for the exclusive owner to process; it cannot mutate the runtime
/// core directly.
#[derive(Clone)]
pub struct ExecutionHandle {
    sink: std::sync::Arc<dyn RuntimeEventSink>,
}

impl ExecutionHandle {
    pub fn cancel_thread(&self, thread_id: galfus_core::ThreadId) {
        let _ = self.sink.submit(RuntimeEvent::CancelThread { thread_id });
    }

    pub fn cancel(&self) {
        let _ = self.sink.submit(RuntimeEvent::CancelExecution);
    }

    pub fn resolve_request(
        &self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        contract: SurfaceContract,
        result: Result<SurfaceValue, ExecutionFailure>,
    ) -> Result<(), galfus_contract::MessageInjectionError> {
        self.sink
            .submit(RuntimeEvent::EffectCompleted {
                thread_id,
                request_lease,
                contract,
                result,
            })
            .map_err(|_| galfus_contract::MessageInjectionError::ExecutionClosed)
    }

    pub fn resolve_future(
        &self,
        thread_id: galfus_core::ThreadId,
        future_lease: galfus_core::FutureLease,
        result: crate::event::FutureResult,
    ) -> Result<(), galfus_contract::MessageInjectionError> {
        self.sink
            .submit(RuntimeEvent::FutureCompleted {
                thread_id,
                future_lease,
                result,
            })
            .map_err(|_| galfus_contract::MessageInjectionError::ExecutionClosed)
    }
}

impl galfus_contract::MessageInjector for ExecutionHandle {
    fn inject_system_response(
        &self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        result: Result<SurfaceValue, ExecutionFailure>,
    ) -> Result<(), galfus_contract::MessageInjectionError> {
        let contract = SurfaceContract::new(
            "runtime::system-response",
            1,
            galfus_contract::SurfaceDirection::FromProvider,
            galfus_contract::SurfaceSchema::Null,
        );
        self.resolve_request(thread_id, request_lease, contract, result)
    }
}

pub(crate) struct FutureCompletionInjector {
    sink: std::sync::Arc<dyn RuntimeEventSink>,
    owner_thread_id: crate::registry::ThreadId,
    request_lease: galfus_core::RequestLease,
    future_lease: galfus_core::FutureLease,
    surface_result: Option<SurfaceContract>,
}

impl FutureCompletionInjector {
    pub(crate) fn new(
        sink: std::sync::Arc<dyn RuntimeEventSink>,
        owner_thread_id: crate::registry::ThreadId,
        request_lease: galfus_core::RequestLease,
        future_lease: galfus_core::FutureLease,
        surface_result: Option<SurfaceContract>,
    ) -> Self {
        Self {
            sink,
            owner_thread_id,
            request_lease,
            future_lease,
            surface_result,
        }
    }
}

impl galfus_contract::MessageInjector for FutureCompletionInjector {
    fn inject_system_response(
        &self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        result: Result<SurfaceValue, ExecutionFailure>,
    ) -> Result<(), galfus_contract::MessageInjectionError> {
        if thread_id != self.owner_thread_id || request_lease != self.request_lease {
            return Err(galfus_contract::MessageInjectionError::HostProtocolViolation);
        }
        let Some(contract) = self.surface_result.clone() else {
            return Err(galfus_contract::MessageInjectionError::UnsupportedSurfaceContract);
        };
        self.sink
            .submit(RuntimeEvent::FutureCompleted {
                thread_id: self.owner_thread_id,
                future_lease: self.future_lease,
                result: result.map(|value| FutureValue::Surface { contract, value }),
            })
            .map_err(|_| galfus_contract::MessageInjectionError::ExecutionClosed)
    }

    fn inject_surface_response(
        &self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        result: Result<SurfaceValue, ExecutionFailure>,
    ) -> Result<(), galfus_contract::MessageInjectionError> {
        let Some(contract) = &self.surface_result else {
            return Err(galfus_contract::MessageInjectionError::UnsupportedSurfaceContract);
        };
        if thread_id != self.owner_thread_id || request_lease != self.request_lease {
            return Err(galfus_contract::MessageInjectionError::HostProtocolViolation);
        }
        let result = result.map(|value| FutureValue::Surface {
            contract: contract.clone(),
            value,
        });
        self.sink
            .submit(RuntimeEvent::FutureCompleted {
                thread_id: self.owner_thread_id,
                future_lease: self.future_lease,
                result,
            })
            .map_err(|_| galfus_contract::MessageInjectionError::ExecutionClosed)
    }
}
