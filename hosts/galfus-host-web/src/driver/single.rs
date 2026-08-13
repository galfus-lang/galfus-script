use galfus_contract::{
    ExecutionFailure, ExecutorStepResult, KernelDriver, KernelTask, ThreadResult,
};
use galfus_runtime::driver::{ExecutionDriver, NativeEventBridge, RuntimeEventSink};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct WebKernelDriver {
    queue: RefCell<VecDeque<KernelTask>>,
    on_exit: RefCell<Option<Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>>>,
    event_bridge: Arc<NativeEventBridge>,
}

impl Default for WebKernelDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl WebKernelDriver {
    pub fn new() -> Self {
        Self {
            queue: RefCell::new(VecDeque::new()),
            on_exit: RefCell::new(None),
            event_bridge: Arc::new(NativeEventBridge::new()),
        }
    }
}

impl KernelDriver for WebKernelDriver {
    fn dispatch(&self, task: KernelTask) {
        self.queue.borrow_mut().push_back(task);
    }

    fn dispatch_front(&self, task: KernelTask) {
        self.queue.borrow_mut().push_front(task);
    }

    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {
        *self.on_exit.borrow_mut() = Some(callback);
    }

    fn run(&self) {
        // Em ambientes Web, o ideal é não bloquear a thread principal em um loop infinito,
        // mas sim delegar a chamada do `step()` para o event loop do JavaScript (ex: requestAnimationFrame).
        // Fornecemos essa implementação bloqueante caso seja invocada dentro de um Worker ou contexto off-main-thread.
        loop {
            match self.step() {
                ExecutorStepResult::Running => continue,
                ExecutorStepResult::Blocked { .. } => return,
                ExecutorStepResult::Completed(_) => return,
            }
        }
    }

    fn step(&self) -> ExecutorStepResult {
        let task = self.queue.borrow_mut().pop_front();

        let Some(task) = task else {
            return ExecutorStepResult::Blocked { timeout: None };
        };

        // Orçamento de instruções (budget) por slice (1000)
        let result = match task {
            KernelTask::Main(t) => t.run(1000),
            KernelTask::Any(t) => t.run(1000),
        };

        match result {
            ThreadResult::Completed(_) | ThreadResult::Discarded => {
                if self.queue.borrow().is_empty() {
                    ExecutorStepResult::Blocked { timeout: None }
                } else {
                    ExecutorStepResult::Running
                }
            }
            ThreadResult::Blocked { timeout } => {
                if self.queue.borrow().is_empty() {
                    ExecutorStepResult::Blocked { timeout }
                } else {
                    ExecutorStepResult::Running
                }
            }
        }
    }

    fn complete(&self, result: Result<i32, ExecutionFailure>) {
        if let Some(cb) = self.on_exit.borrow_mut().take() {
            cb(result);
        }
    }
}

impl ExecutionDriver for WebKernelDriver {
    fn event_sink(&self) -> Arc<dyn RuntimeEventSink> {
        self.event_bridge.clone()
    }

    fn drain_events(
        &self,
    ) -> Vec<(
        galfus_runtime::event::EventSequence,
        galfus_runtime::event::RuntimeEvent,
    )> {
        self.event_bridge.drain()
    }

    fn has_pending_events(&self) -> bool {
        self.event_bridge.has_pending()
    }

    fn configure_limits(
        &self,
        limits: &galfus_contract::LimitsMetadata,
    ) -> Result<(), galfus_runtime::driver::EventDeliveryError> {
        self.event_bridge.configure_limit(limits.max_event_queue)
    }
}
