use crate::event::{EventSink, RuntimeEvent};
use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, ExecutorStepResult, KernelDriver,
    RunnableTask, ThreadResult,
};
use std::rc::Rc;

/// Owns one running program and drives its orchestrator cooperatively.
pub struct Execution {
    root: Option<Box<dyn RunnableTask>>,
    root_thread_id: u64,
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
    Completed,
    Failed,
    Cancelled,
}

impl Execution {
    pub(crate) fn new(
        root: Box<dyn RunnableTask>,
        root_thread_id: u64,
        driver: Rc<dyn KernelDriver>,
        sink: EventSink,
    ) -> Self {
        Self {
            root: Some(root),
            root_thread_id,
            driver,
            sink,
            result: None,
            state: ExecutionState::Created,
        }
    }

    pub fn handle(&self) -> ExecutionHandle {
        ExecutionHandle {
            sink: self.sink.clone(),
            continuation: None,
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
                    self.result = Some(Err(error));
                    self.state = ExecutionState::Failed;
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
                    return Err(ExecutionFailure::new(
                        ExecutionFailureKind::InternalRuntimeFailure,
                        "execution is blocked",
                    ));
                }
                ExecutorStepResult::Running => {}
            }
        }
    }

    pub fn cancel(&mut self) {
        if let Some(thread_id) = crate::registry::ThreadId::from_raw(self.root_thread_id) {
            self.sink.send(RuntimeEvent::CancelThread { thread_id });
            self.state = ExecutionState::Cancelled;
            self.result = Some(Err(ExecutionFailure::new(
                ExecutionFailureKind::Cancelled,
                "execution cancelled",
            )));
        }
    }
}

/// Thread-safe handle that lets external integrations request cancellation.
#[derive(Clone)]
pub struct ExecutionHandle {
    sink: EventSink,
    continuation: Option<galfus_vm::Continuation>,
}

impl ExecutionHandle {
    pub(crate) fn for_continuation(sink: EventSink, continuation: galfus_vm::Continuation) -> Self {
        Self {
            sink,
            continuation: Some(continuation),
        }
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
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let Some(thread_id) = crate::registry::ThreadId::from_raw(thread_id as u64) else {
            return;
        };
        let Some(continuation) = &self.continuation else {
            return;
        };
        self.sink.send(RuntimeEvent::EffectCompleted {
            thread_id,
            continuation: continuation.clone(),
            result,
        });
    }
}
