use galfus_contract::{
    ExecutionFailure, ExecutorStepResult, KernelDriver, KernelTask, RunnableTask, ThreadResult,
};
use galfus_runtime::driver::{ExecutionDriver, NativeEventBridge, RuntimeEventSink};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct WebWorkersDriver {
    /// Fila de tarefas restritas à thread principal (Orchestrator)
    main_queue: RefCell<VecDeque<KernelTask>>,
    /// Fila compartilhada para tarefas 'Any' (Workers)
    shared_queue: Arc<Mutex<VecDeque<Box<dyn RunnableTask + Send>>>>,
    on_exit: RefCell<Option<Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>>>,
    event_bridge: Arc<NativeEventBridge>,
}

impl Default for WebWorkersDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl WebWorkersDriver {
    pub fn new() -> Self {
        Self {
            main_queue: RefCell::new(VecDeque::new()),
            shared_queue: Arc::new(Mutex::new(VecDeque::new())),
            on_exit: RefCell::new(None),
            event_bridge: Arc::new(NativeEventBridge::new()),
        }
    }

    /// Retorna uma referência clonada da fila compartilhada.
    /// Isso é útil para injetar a fila no worker em WASM (que roda a partir do JS).
    pub fn shared_queue(&self) -> Arc<Mutex<VecDeque<Box<dyn RunnableTask + Send>>>> {
        self.shared_queue.clone()
    }
}

impl KernelDriver for WebWorkersDriver {
    fn dispatch(&self, task: KernelTask) {
        match task {
            KernelTask::Main(_) => {
                self.main_queue.borrow_mut().push_back(task);
            }
            KernelTask::Any(any_task) => {
                // Ao adicionar à fila compartilhada, um worker será capaz de extrair a tarefa.
                self.shared_queue.lock().unwrap().push_back(any_task);
            }
        }
    }

    fn dispatch_front(&self, task: KernelTask) {
        match task {
            KernelTask::Main(_) => {
                self.main_queue.borrow_mut().push_front(task);
            }
            KernelTask::Any(any_task) => {
                self.shared_queue.lock().unwrap().push_front(any_task);
            }
        }
    }

    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {
        *self.on_exit.borrow_mut() = Some(callback);
    }

    fn run(&self) {
        loop {
            match self.step() {
                ExecutorStepResult::Running => continue,
                ExecutorStepResult::Blocked { .. } => return,
                ExecutorStepResult::Completed(_) => return,
            }
        }
    }

    fn step(&self) -> ExecutorStepResult {
        // A thread principal apenas consome as tarefas 'Main' (como o Orchestrator).
        let task = self.main_queue.borrow_mut().pop_front();

        let Some(KernelTask::Main(t)) = task else {
            // Se a fila principal está vazia mas há tarefas Any, estamos teoricamente rodando.
            if self.shared_queue.lock().unwrap().is_empty() {
                return ExecutorStepResult::Blocked { timeout: None };
            } else {
                return ExecutorStepResult::Running;
            }
        };

        match t.run(1000) {
            ThreadResult::Completed(_) | ThreadResult::Discarded => {
                if self.main_queue.borrow().is_empty()
                    && self.shared_queue.lock().unwrap().is_empty()
                {
                    ExecutorStepResult::Blocked { timeout: None }
                } else {
                    ExecutorStepResult::Running
                }
            }
            ThreadResult::Blocked { timeout } => {
                if self.main_queue.borrow().is_empty()
                    && self.shared_queue.lock().unwrap().is_empty()
                {
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

impl ExecutionDriver for WebWorkersDriver {
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

/// Helper para ser invocado dentro do loop do Worker WASM instanciado pelo Javascript.
/// Retorna `true` se processou uma tarefa ou `false` se a fila estava vazia.
pub fn worker_process_task(queue: &Mutex<VecDeque<Box<dyn RunnableTask + Send>>>) -> bool {
    let task_opt = {
        let mut q = queue.lock().unwrap();
        q.pop_front()
    };

    if let Some(task) = task_opt {
        let _ = task.run(1000);
        true
    } else {
        false
    }
}
