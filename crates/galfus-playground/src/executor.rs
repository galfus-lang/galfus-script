use std::sync;

use galfus_contract::{ExecutorStepResult, KernelDriver, KernelTask, ThreadResult};
use std::collections::VecDeque;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

pub struct PlaygroundExecutor {
    queue: Mutex<VecDeque<KernelTask>>,
    next_thread_id: AtomicU64,
    exit_code: sync::Mutex<i32>,
    exit_callback: Mutex<Option<Box<dyn Fn(Result<i32, String>) + Send + Sync>>>,
}

impl PlaygroundExecutor {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            next_thread_id: AtomicU64::new(1),
            exit_code: sync::Mutex::new(0),
            exit_callback: Mutex::new(None),
        }
    }
}

impl KernelDriver for PlaygroundExecutor {
    fn dispatch(&self, task: KernelTask) {
        self.queue.lock().unwrap().push_back(task);
    }

    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, String>) + Send + Sync>) {
        *self.exit_callback.lock().unwrap() = Some(callback);
    }

    fn run(&self) {
        // NON-BLOCKING
    }

    fn step(&self) -> Result<ExecutorStepResult, String> {
        let task_entry = {
            let mut q = self.queue.lock().unwrap();
            q.pop_front()
        };

        let Some(task_entry) = task_entry else {
            return Ok(ExecutorStepResult::Blocked { timeout: None });
        };

        let runnable = match task_entry {
            KernelTask::Main(t) => t,
            KernelTask::Any(t) => t,
        };

        match runnable.run(100) {
            ThreadResult::Yielded(t) => {
                self.queue.lock().unwrap().push_back(KernelTask::Main(t));
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
                if let Some(cb) = self.exit_callback.lock().unwrap().take() {
                    cb(Ok(code));
                }
                let is_empty = self.queue.lock().unwrap().is_empty();
                if is_empty {
                    Ok(ExecutorStepResult::Completed(code))
                } else {
                    Ok(ExecutorStepResult::Running)
                }
            }
            ThreadResult::Failed(err) => {
                if let Some(cb) = self.exit_callback.lock().unwrap().take() {
                    cb(Err(err.clone()));
                }
                Err(err)
            }
        }
    }
}

impl Default for PlaygroundExecutor {
    fn default() -> Self {
        Self::new()
    }
}
