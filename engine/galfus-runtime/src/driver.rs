use std::collections::VecDeque;
use std::sync;
use std::sync::Mutex;
use std::thread;

use galfus_contract::{
    ExecutionFailure, ExecutorStepResult, KernelDriver, KernelTask, ThreadResult,
};

/// Runs Galfus tasks cooperatively on the calling host thread.
pub struct CooperativeDriver {
    queue: Mutex<VecDeque<KernelTask>>,
    exit_result: sync::Mutex<Option<Result<i32, ExecutionFailure>>>,
    exit_callback: Mutex<Option<Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>>>,
}

impl CooperativeDriver {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            exit_result: sync::Mutex::new(None),
            exit_callback: Mutex::new(None),
        }
    }
}

impl KernelDriver for CooperativeDriver {
    fn dispatch(&self, task: KernelTask) {
        self.queue.lock().unwrap().push_back(task);
    }

    fn dispatch_front(&self, task: KernelTask) {
        self.queue.lock().unwrap().push_front(task);
    }

    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {
        *self.exit_callback.lock().unwrap() = Some(callback);
    }

    fn run(&self) {
        let mut pending_timeout = None;
        loop {
            let task_entry = self.queue.lock().unwrap().pop_front();

            let Some(task_entry) = task_entry else {
                let Some(timeout) = pending_timeout.take() else {
                    break;
                };
                thread::sleep(timeout);
                continue;
            };

            let result = match task_entry {
                KernelTask::Main(task) => task.run(100),
                KernelTask::Any(task) => task.run(100),
            };

            match result {
                ThreadResult::Discarded => {}
                ThreadResult::Blocked { timeout } => {
                    pending_timeout = match (pending_timeout, timeout) {
                        (Some(current), Some(next)) => Some(current.min(next)),
                        (Some(current), None) => Some(current),
                        (None, next) => next,
                    };
                }
                ThreadResult::Completed(res) => {
                    let outcome = match res {
                        Ok(galfus_contract::BoundaryValue::I32(code)) => Ok(code),
                        Ok(_) => Ok(0),
                        Err(e) => Err(e),
                    };
                    *self.exit_result.lock().unwrap() = Some(outcome);
                }
            }
        }
        let outcome = self.exit_result.lock().unwrap().take().unwrap_or(Ok(0));
        if let Some(callback) = self.exit_callback.lock().unwrap().take() {
            callback(outcome);
        }
    }

    fn complete(&self, result: Result<i32, ExecutionFailure>) {
        *self.exit_result.lock().unwrap() = Some(result.clone());
        if let Some(callback) = self.exit_callback.lock().unwrap().take() {
            callback(result);
        }
    }

    fn step(&self) -> ExecutorStepResult {
        let task_entry = self.queue.lock().unwrap().pop_front();

        let Some(task_entry) = task_entry else {
            return ExecutorStepResult::Blocked { timeout: None };
        };

        let result = match task_entry {
            KernelTask::Main(task) => task.run(100),
            KernelTask::Any(task) => task.run(100),
        };

        match result {
            ThreadResult::Discarded => ExecutorStepResult::Running,
            ThreadResult::Blocked { timeout } => {
                let is_empty = self.queue.lock().unwrap().is_empty();
                if is_empty {
                    ExecutorStepResult::Blocked { timeout }
                } else {
                    ExecutorStepResult::Running
                }
            }
            ThreadResult::Completed(res) => {
                let outcome = match res {
                    Ok(galfus_contract::BoundaryValue::I32(c)) => Ok(c),
                    Ok(_) => Ok(0),
                    Err(e) => Err(e),
                };
                let code = match &outcome {
                    Ok(c) => *c,
                    Err(_) => 0,
                };
                *self.exit_result.lock().unwrap() = Some(outcome);
                let is_empty = self.queue.lock().unwrap().is_empty();
                if is_empty {
                    ExecutorStepResult::Completed(code)
                } else {
                    ExecutorStepResult::Running
                }
            }
        }
    }
}

impl Default for CooperativeDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use galfus_contract::ExecutionFailureKind;
    use std::sync::Arc;

    #[test]
    fn complete_stores_and_calls_callback_with_error() {
        let driver = CooperativeDriver::new();
        let callback_called = Arc::new(Mutex::new(false));
        let callback_called_clone = Arc::clone(&callback_called);

        driver.on_exit(Box::new(move |result| {
            assert!(result.is_err());
            let error = result.unwrap_err();
            assert_eq!(error.kind, ExecutionFailureKind::VmPanic);
            *callback_called_clone.lock().unwrap() = true;
        }));

        driver.complete(Err(ExecutionFailure::new(
            ExecutionFailureKind::VmPanic,
            "test error",
        )));

        assert!(*callback_called.lock().unwrap());

        let stored = driver.exit_result.lock().unwrap().clone();
        assert!(stored.is_some());
        let stored_result = stored.unwrap();
        assert!(stored_result.is_err());
        assert_eq!(
            stored_result.unwrap_err().kind,
            ExecutionFailureKind::VmPanic
        );
    }
}
