use super::CooperativeDriver;
use galfus_contract::{ExecutorStepResult, KernelDriver, KernelTask, RunnableTask, ThreadResult};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct YieldOnceTask {
    yielded: Arc<AtomicBool>,
}

impl RunnableTask for YieldOnceTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        if self.yielded.swap(true, Ordering::AcqRel) {
            ThreadResult::Completed(0)
        } else {
            ThreadResult::Yielded(self)
        }
    }

    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        Some(self)
    }
}

#[test]
fn yielded_any_thread_work_keeps_its_affinity() {
    let driver = CooperativeDriver::new();
    driver.dispatch(KernelTask::Any(Box::new(YieldOnceTask {
        yielded: Arc::new(AtomicBool::new(false)),
    })));

    assert!(matches!(driver.step(), Ok(ExecutorStepResult::Running)));
    assert!(matches!(
        driver.queue.lock().unwrap().pop_front(),
        Some(KernelTask::Any(_))
    ));
}
