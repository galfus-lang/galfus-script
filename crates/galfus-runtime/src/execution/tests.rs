use super::*;
use crate::orchestrator::Orchestrator;
use galfus_contract::KernelTask;

struct IdleDriver;

impl KernelDriver for IdleDriver {
    fn dispatch(&self, _task: KernelTask) {}

    fn on_exit(&self, _callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {}

    fn run(&self) {}
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
