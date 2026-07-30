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
    exit_code: sync::Mutex<i32>,
    exit_callback: Mutex<Option<Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>>>,
}

impl CooperativeDriver {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            exit_code: sync::Mutex::new(0),
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
                ThreadResult::Completed(code) => *self.exit_code.lock().unwrap() = code,
            }
        }
        let code = *self.exit_code.lock().unwrap();
        if let Some(callback) = self.exit_callback.lock().unwrap().take() {
            callback(Ok(code));
        }
    }

    fn complete(&self, result: Result<i32, ExecutionFailure>) {
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
            ThreadResult::Completed(code) => {
                *self.exit_code.lock().unwrap() = code;
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
