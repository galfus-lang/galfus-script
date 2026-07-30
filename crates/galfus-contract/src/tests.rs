use super::*;
use std::rc::Rc;
use std::sync::Arc;

struct DummyHost;

impl HostProvider for DummyHost {
    fn dispatch(
        &mut self,
        _thread_id: usize,
        _request_id: u64,
        _method: &str,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
        // dummy
    }
}

#[test]
fn providers_allow_execution_without_host() {
    assert!(Providers::new().host_mut().is_none());
}

#[test]
fn providers_allow_host() {
    let mut providers = Providers::with_host(Box::new(DummyHost));
    assert!(providers.host_mut().is_some());
}

#[test]
fn execution_failures_preserve_machine_readable_context() {
    let failure = ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "request failed")
        .with_thread_id(7)
        .with_module_id(3);

    assert_eq!(failure.thread_id, Some(7));
    assert_eq!(failure.module_id, Some(3));
}

struct MainThreadOnlyTask(Rc<()>);

impl RunnableTask for MainThreadOnlyTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        assert_eq!(Rc::strong_count(&self.0), 1);
        ThreadResult::Completed(0)
    }
}

#[test]
fn main_kernel_tasks_accept_non_send_state() {
    let task = KernelTask::Main(Box::new(MainThreadOnlyTask(Rc::new(()))));
    assert_eq!(task.affinity(), TaskAffinity::Main);
    let KernelTask::Main(task) = task else {
        panic!("main task must preserve its affinity");
    };
    assert!(matches!(task.run(1), ThreadResult::Completed(0)));
}
