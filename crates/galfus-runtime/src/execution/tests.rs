use super::*;
use crate::orchestrator::Orchestrator;
use galfus_contract::{ExecutorStepResult, KernelTask, RunnableTask, ThreadResult};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

struct IdleDriver;

impl KernelDriver for IdleDriver {
    fn dispatch(&self, _task: KernelTask) {}

    fn on_exit(&self, _callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {}

    fn run(&self) {}

    fn step(&self) -> Result<ExecutorStepResult, ExecutionFailure> {
        Ok(ExecutorStepResult::Running)
    }
}

struct YieldingTask;

impl RunnableTask for YieldingTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        ThreadResult::Yielded(self)
    }
}

struct CompletedTask;

impl RunnableTask for CompletedTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        ThreadResult::Completed(7)
    }
}

fn execution(root: Box<dyn RunnableTask>, initializing: bool, initialized: bool) -> Execution {
    let (sender, _receiver) = mpsc::channel();
    Execution::new(
        root,
        Rc::new(IdleDriver),
        crate::event::EventSink::new(sender),
        Arc::new(AtomicBool::new(initialized)),
        initializing,
    )
}

#[test]
fn execution_transitions_from_created_to_running_and_preserves_completion() {
    let mut execution = execution(Box::new(CompletedTask), false, true);
    assert_eq!(execution.status(), ExecutionState::Created);

    assert!(matches!(
        execution.poll(1),
        Ok(ExecutorStepResult::Completed(7))
    ));
    assert_eq!(execution.status(), ExecutionState::Completed);
    assert_eq!(execution.result(), Some(&Ok(BoundaryValue::I32(7))));
    assert_eq!(execution.run_to_completion(), Ok(BoundaryValue::I32(7)));
    assert_eq!(execution.result(), Some(&Ok(BoundaryValue::I32(7))));
}

#[test]
fn execution_remains_initializing_until_the_orchestrator_signal() {
    let initialization_complete = Arc::new(AtomicBool::new(false));
    let (sender, _receiver) = mpsc::channel();
    let mut execution = Execution::new(
        Box::new(YieldingTask),
        Rc::new(IdleDriver),
        crate::event::EventSink::new(sender),
        initialization_complete.clone(),
        true,
    );

    execution.poll(1).expect("initializing slice succeeds");
    assert_eq!(execution.status(), ExecutionState::Initializing);
    initialization_complete.store(true, Ordering::Release);
    execution.poll(1).expect("running slice succeeds");
    assert_eq!(execution.status(), ExecutionState::Running);
}

#[test]
fn cancellation_transitions_the_execution_to_cancelled() {
    let orchestrator = Orchestrator::new();
    let sink = orchestrator.sink();
    let mut execution = Execution::new(
        Box::new(orchestrator),
        Rc::new(IdleDriver),
        sink,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        false,
    );

    execution.cancel();
    assert_eq!(execution.status(), ExecutionState::Cancelling);

    let Err(error) = execution.poll(100) else {
        panic!("cancellation must produce a structured failure");
    };
    assert_eq!(error.kind, ExecutionFailureKind::Cancelled);
    assert_eq!(execution.status(), ExecutionState::Cancelled);
}
