use super::*;
use crate::orchestrator::Orchestrator;
use galfus_contract::{ExecutorStepResult, KernelTask};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct IdleDriver;

impl KernelDriver for IdleDriver {
    fn dispatch(&self, _task: KernelTask) {}

    fn on_exit(&self, _callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {}

    fn run(&self) {}

    fn step(&self) -> ExecutorStepResult {
        ExecutorStepResult::Running
    }
}

#[test]
fn execution_transitions_from_created_to_running_and_preserves_completion() {
    let orchestrator = Orchestrator::new();
    let sink = orchestrator.sink();
    let mut execution = Execution::new(
        orchestrator,
        Rc::new(IdleDriver),
        sink,
        Arc::new(AtomicBool::new(true)),
        false,
    );
    assert_eq!(execution.status(), ExecutionState::Created);

    assert!(matches!(
        execution.poll(1),
        Ok(ExecutorStepResult::Completed(0))
    ));
    assert_eq!(execution.status(), ExecutionState::Completed);
    assert_eq!(execution.result(), Some(&Ok(BoundaryValue::I32(0))));
    assert_eq!(execution.run_to_completion(), Ok(BoundaryValue::I32(0)));
    assert_eq!(execution.result(), Some(&Ok(BoundaryValue::I32(0))));
}

#[test]
fn execution_remains_initializing_until_the_orchestrator_signal() {
    let initialization_complete = Arc::new(AtomicBool::new(false));
    let mut orchestrator = Orchestrator::new();
    let token = orchestrator.main_thread_token();
    let thread_id = orchestrator
        .kernel_mut(token)
        .spawn(galfus_vm::thread::VmThreadState::new(), None);
    let thread = orchestrator
        .kernel_mut(token)
        .take_thread(thread_id)
        .unwrap();
    orchestrator
        .kernel_mut(token)
        .enqueue_runnable(thread_id, thread);

    let driver = Rc::new(IdleDriver);
    orchestrator.set_vm(Arc::new(galfus_vm::VirtualMachine::new(Default::default())));
    orchestrator.set_driver(driver.clone());

    let sink = orchestrator.sink();
    let mut execution = Execution::new(
        orchestrator,
        driver,
        sink,
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
        orchestrator,
        Rc::new(IdleDriver),
        sink,
        Arc::new(AtomicBool::new(true)),
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
