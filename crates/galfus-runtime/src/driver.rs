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

            let runnable = match task_entry {
                KernelTask::Main(task) => task,
                KernelTask::Any(task) => task,
            };

            match runnable.run(100) {
                ThreadResult::Yielded(task) => {
                    self.queue.lock().unwrap().push_back(KernelTask::Main(task))
                }
                ThreadResult::Blocked { timeout } => {
                    pending_timeout = match (pending_timeout, timeout) {
                        (Some(current), Some(next)) => Some(current.min(next)),
                        (Some(current), None) => Some(current),
                        (None, next) => next,
                    };
                }
                ThreadResult::Completed(code) => *self.exit_code.lock().unwrap() = code,
                ThreadResult::Failed(error) => {
                    if let Some(callback) = self.exit_callback.lock().unwrap().take() {
                        callback(Err(error));
                    }
                    return;
                }
            }
        }
        let code = *self.exit_code.lock().unwrap();
        if let Some(callback) = self.exit_callback.lock().unwrap().take() {
            callback(Ok(code));
        }
    }

    fn step(&self) -> Result<ExecutorStepResult, ExecutionFailure> {
        let task_entry = self.queue.lock().unwrap().pop_front();

        let Some(task_entry) = task_entry else {
            return Ok(ExecutorStepResult::Blocked { timeout: None });
        };

        let runnable = match task_entry {
            KernelTask::Main(task) => task,
            KernelTask::Any(task) => task,
        };

        match runnable.run(100) {
            ThreadResult::Yielded(task) => {
                self.queue.lock().unwrap().push_back(KernelTask::Main(task));
                Ok(ExecutorStepResult::Running)
            }
            ThreadResult::Blocked { timeout } => {
                let is_empty = self.queue.lock().unwrap().is_empty();
                if is_empty {
                    Ok(ExecutorStepResult::Blocked { timeout })
                } else {
                    Ok(ExecutorStepResult::Running)
                }
            }
            ThreadResult::Completed(code) => {
                *self.exit_code.lock().unwrap() = code;
                let is_empty = self.queue.lock().unwrap().is_empty();
                if is_empty {
                    Ok(ExecutorStepResult::Completed(code))
                } else {
                    Ok(ExecutorStepResult::Running)
                }
            }
            ThreadResult::Failed(error) => Err(error),
        }
    }
}

impl Default for CooperativeDriver {
    fn default() -> Self {
        Self::new()
    }
}
