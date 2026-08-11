#[cfg(test)]
mod tests;

use crate::event::{EventSink, RuntimeEvent};
use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, ExecutorStepResult, KernelDriver,
    ThreadResult,
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
    driver: Rc<dyn KernelDriver>,
    sink: EventSink,
    result: Option<Result<BoundaryValue, ExecutionFailure>>,
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
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    ShuttingDown,
    Stopped,
}
impl Execution {
    pub(crate) fn new(
        orchestrator: crate::orchestrator::Orchestrator,
        driver: Rc<dyn KernelDriver>,
        sink: EventSink,
        initialization_complete: Arc<AtomicBool>,
        is_initializing: bool,
    ) -> Self {
        Self {
            orchestrator: Some(orchestrator),
            driver,
            sink,
            result: None,
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
            sink: self.sink.clone(),
        }
    }

    pub fn status(&self) -> ExecutionState {
        self.state
    }

    pub fn result(&self) -> Option<&Result<BoundaryValue, ExecutionFailure>> {
        self.result.as_ref()
    }

    /// Advances virtual time; the change is applied by the execution owner on poll.
    pub fn tick_timeouts(&self, delta_ms: u64) {
        self.sink.send(RuntimeEvent::Tick { delta_ms });
    }

    pub fn poll(&mut self, budget: usize) -> Result<ExecutorStepResult, ExecutionFailure> {
        if matches!(self.state, ExecutionState::Cancelling) {
            self.state = ExecutionState::ShuttingDown;
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
                        self.state =
                            if failure.kind == galfus_contract::ExecutionFailureKind::Cancelled {
                                ExecutionState::Cancelled
                            } else {
                                ExecutionState::Failed
                            };
                        self.result = Some(Err(failure));
                        self.orchestrator = None;
                    }
                }
                ThreadResult::Completed(res) => {
                    self.state = match res {
                        Ok(_) => ExecutionState::Completed,
                        Err(_) => ExecutionState::Failed,
                    };
                    self.result = Some(res);
                    self.orchestrator = None;
                }
                ThreadResult::Blocked { .. } => {
                    self.state = ExecutionState::Waiting;
                }
            }
        }
        if let Some(result) = &self.result {
            if !self.exit_notified {
                self.exit_notified = true;
                self.driver.complete(match result {
                    Ok(BoundaryValue::I32(code)) => Ok(*code),
                    Ok(_) => Ok(0),
                    Err(error) => Err(error.clone()),
                });
            }
            return match result {
                Ok(BoundaryValue::I32(code)) => Ok(ExecutorStepResult::Completed(*code)),
                Ok(_) => Ok(ExecutorStepResult::Completed(0)),
                Err(error) => Err(error.clone()),
            };
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

    pub fn run_sync_to_completion(&mut self) -> Result<BoundaryValue, ExecutionFailure> {
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
                    if !self.sink.has_pending() {
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
            self.sink.send(RuntimeEvent::CancelExecution);
            self.state = ExecutionState::Cancelling;
        }
    }
}

/// Thread-safe ingress for external integrations.
///
/// This handle queues requests for the exclusive owner to process; it cannot mutate the runtime
/// core directly.
#[derive(Clone)]
pub struct ExecutionHandle {
    sink: EventSink,
}

impl ExecutionHandle {
    pub fn cancel_thread(&self, thread_id: galfus_core::ThreadId) {
        self.sink.send(RuntimeEvent::CancelThread { thread_id });
    }

    pub fn cancel(&self) {
        self.sink.send(RuntimeEvent::CancelExecution);
    }

    pub fn resolve_request(
        &self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        self.sink.send(RuntimeEvent::EffectCompleted {
            thread_id,
            request_lease,
            result,
        });
    }

    pub fn resolve_future(
        &self,
        thread_id: galfus_core::ThreadId,
        future_lease: galfus_core::FutureLease,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        self.sink.send(RuntimeEvent::FutureCompleted {
            thread_id,
            future_lease,
            result,
        });
    }
}

impl galfus_contract::MessageInjector for ExecutionHandle {
    fn inject_system_response(
        &self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        self.resolve_request(thread_id, request_lease, result);
    }
}

pub(crate) struct FutureCompletionInjector {
    sink: EventSink,
    owner_thread_id: crate::registry::ThreadId,
    future_lease: galfus_core::FutureLease,
}

impl FutureCompletionInjector {
    pub(crate) fn new(
        sink: EventSink,
        owner_thread_id: crate::registry::ThreadId,
        future_lease: galfus_core::FutureLease,
    ) -> Self {
        Self {
            sink,
            owner_thread_id,
            future_lease,
        }
    }
}

impl galfus_contract::MessageInjector for FutureCompletionInjector {
    fn inject_system_response(
        &self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        self.sink.send(RuntimeEvent::FutureCompleted {
            thread_id: self.owner_thread_id,
            future_lease: self.future_lease,
            result,
        });
    }
}
