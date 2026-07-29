#[cfg(test)]
mod tests;

use crate::event::{EventSink, RuntimeEvent};
use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, ExecutorStepResult, KernelDriver,
    RunnableTask, ThreadResult,
};
use std::rc::Rc;

/// Owns one running program and drives its orchestrator cooperatively.
pub struct Execution {
    root: Option<Box<dyn RunnableTask>>,
    driver: Rc<dyn KernelDriver>,
    sink: EventSink,
    result: Option<Result<BoundaryValue, ExecutionFailure>>,
    state: ExecutionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    Created,
    Running,
    Waiting,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl Execution {
    pub(crate) fn new(
        root: Box<dyn RunnableTask>,
        driver: Rc<dyn KernelDriver>,
        sink: EventSink,
    ) -> Self {
        Self {
            root: Some(root),
            driver,
            sink,
            result: None,
            state: ExecutionState::Created,
        }
    }

    pub fn handle(&self) -> ExecutionHandle {
        ExecutionHandle {
            sink: self.sink.clone(),
        }
    }

    pub fn into_task(mut self) -> Box<dyn RunnableTask> {
        self.root
            .take()
            .expect("execution task is available before polling")
    }

    pub fn status(&self) -> ExecutionState {
        self.state
    }

    pub fn result(&self) -> Option<&Result<BoundaryValue, ExecutionFailure>> {
        self.result.as_ref()
    }

    /// Advances virtual time; the change is applied by the main-thread orchestrator on poll.
    pub fn tick_timeouts(&self, delta_ms: u64) {
        self.sink.send(RuntimeEvent::Tick { delta_ms });
    }

    pub fn poll(&mut self, budget: usize) -> Result<ExecutorStepResult, ExecutionFailure> {
        if matches!(self.state, ExecutionState::Created) {
            self.state = ExecutionState::Running;
        }
        if let Some(root) = self.root.take() {
            match root.run(budget) {
                ThreadResult::Yielded(root) => self.root = Some(root),
                ThreadResult::Completed(code) => {
                    self.result = Some(Ok(BoundaryValue::I32(code)));
                    self.state = ExecutionState::Completed;
                }
                ThreadResult::Failed(error) => {
                    self.state = if error.kind == ExecutionFailureKind::Cancelled {
                        ExecutionState::Cancelled
                    } else {
                        ExecutionState::Failed
                    };
                    self.result = Some(Err(error));
                }
                ThreadResult::Blocked { .. } => {
                    self.state = ExecutionState::Waiting;
                }
            }
        }
        if let Some(result) = &self.result {
            return match result {
                Ok(BoundaryValue::I32(code)) => Ok(ExecutorStepResult::Completed(*code)),
                Ok(_) => Ok(ExecutorStepResult::Completed(0)),
                Err(error) => Err(error.clone()),
            };
        }
        let state = self.driver.step()?;
        if matches!(state, ExecutorStepResult::Completed(_)) && self.root.is_some() {
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

    pub fn run_to_completion(&mut self) -> Result<BoundaryValue, ExecutionFailure> {
        loop {
            match self.poll(100)? {
                ExecutorStepResult::Completed(_) => {
                    return self.result.take().unwrap_or(Ok(BoundaryValue::Null));
                }
                ExecutorStepResult::Blocked { .. } => {
                    if !self.sink.has_pending() {
                        return Err(ExecutionFailure::new(
                            ExecutionFailureKind::InternalRuntimeFailure,
                            "execution is blocked",
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
            ExecutionState::Created | ExecutionState::Running | ExecutionState::Waiting
        ) {
            self.sink.send(RuntimeEvent::CancelExecution);
            self.state = ExecutionState::Cancelling;
        }
    }
}

/// Thread-safe handle that lets external integrations request cancellation.
#[derive(Clone)]
pub struct ExecutionHandle {
    sink: EventSink,
}

impl ExecutionHandle {
    pub(crate) fn new(sink: EventSink) -> Self {
        Self { sink }
    }

    pub fn cancel_thread(&self, thread_id: usize) {
        if let Some(thread_id) = crate::registry::ThreadId::from_raw(thread_id as u64) {
            self.sink.send(RuntimeEvent::CancelThread { thread_id });
        }
    }
}

impl galfus_contract::MessageInjector for ExecutionHandle {
    fn inject_system_response(
        &self,
        thread_id: usize,
        request_id: u64,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let Some(thread_id) = crate::registry::ThreadId::from_raw(thread_id as u64) else {
            return;
        };
        self.sink.send(RuntimeEvent::EffectCompleted {
            thread_id,
            request_id,
            result,
        });
    }
}
