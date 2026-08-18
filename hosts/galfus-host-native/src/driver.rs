use crossbeam_channel::{Receiver, Sender, unbounded};
use galfus_contract::{
    ExecutionFailure, ExecutorStepResult, KernelDriver, KernelTask, LimitsMetadata, RunnableTask,
    ThreadResult,
};
use galfus_runtime::driver::{ExecutionDriver, NativeEventBridge, RuntimeEventSink};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

pub struct NativeDriver {
    main_queue_tx: Sender<KernelTask>,
    main_queue_rx: Receiver<KernelTask>,

    worker_queue_tx: Sender<Box<dyn RunnableTask + Send>>,

    event_bridge: Arc<NativeEventBridge>,
    active_workers: Arc<AtomicUsize>,

    exit_callback: Mutex<Option<Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>>>,
}

impl NativeDriver {
    pub fn new() -> Self {
        let (main_tx, main_rx) = unbounded();
        let (worker_tx, worker_rx) = unbounded::<Box<dyn RunnableTask + Send>>();
        let active_workers = Arc::new(AtomicUsize::new(0));

        // Use available logical cores, defaulting to 4 if detection fails.
        let num_workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        for _ in 0..num_workers {
            let rx = worker_rx.clone();
            let active = active_workers.clone();

            thread::spawn(move || {
                // Background worker loop
                while let Ok(task) = rx.recv() {
                    let _ = task.run(100);
                    active.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }

        Self {
            main_queue_tx: main_tx,
            main_queue_rx: main_rx,
            worker_queue_tx: worker_tx,
            event_bridge: Arc::new(NativeEventBridge::new()),
            active_workers,
            exit_callback: Mutex::new(None),
        }
    }

    fn run_main_task(task: Box<dyn RunnableTask>) -> Option<ExecutorStepResult> {
        let result = task.run(100); // budget arbitrária

        match result {
            ThreadResult::Discarded => Some(ExecutorStepResult::Running),
            ThreadResult::Blocked { timeout } => Some(ExecutorStepResult::Blocked { timeout }),
            ThreadResult::Completed(res) => {
                let code = if let Ok(galfus_contract::BoundaryValue::I32(c)) = res {
                    c
                } else {
                    0
                };
                Some(ExecutorStepResult::Completed(code))
            }
        }
    }
}

impl KernelDriver for NativeDriver {
    fn dispatch(&self, task: KernelTask) {
        match task {
            KernelTask::Main(t) => {
                let _ = self.main_queue_tx.send(KernelTask::Main(t));
            }
            KernelTask::Any(t) => {
                self.active_workers.fetch_add(1, Ordering::SeqCst);
                let _ = self.worker_queue_tx.send(t);
            }
        }
    }

    fn dispatch_front(&self, task: KernelTask) {
        // Crossbeam's unbounded doesn't natively support LIFO/push_front.
        // For the MVP, we route it directly as a normal dispatch.
        self.dispatch(task);
    }

    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {
        *self.exit_callback.lock().unwrap() = Some(callback);
    }

    fn complete(&self, result: Result<i32, ExecutionFailure>) {
        if let Some(cb) = self.exit_callback.lock().unwrap().take() {
            cb(result);
        }
    }

    fn run(&self) {
        loop {
            match self.step() {
                ExecutorStepResult::Running => continue,
                ExecutorStepResult::Blocked { timeout } => {
                    // Se não há tarefas na Main, bloqueamos a Main esperando um evento chegar
                    let recv_result = if let Some(t) = timeout {
                        self.main_queue_rx.recv_timeout(t).ok()
                    } else {
                        self.main_queue_rx.recv().ok()
                    };

                    if let Some(task) = recv_result {
                        self.dispatch(task); // Re-injeta para ser pego pelo `step()` na proxima iteração
                    }
                }
                ExecutorStepResult::Completed(_) => break,
            }
        }
    }

    fn step(&self) -> ExecutorStepResult {
        // O `step` do driver tenta processar tarefas apenas da Main (já que Any está em background)
        if let Ok(task) = self.main_queue_rx.try_recv() {
            if let KernelTask::Main(t) = task
                && let Some(res) = Self::run_main_task(t)
            {
                return res;
            }
            return ExecutorStepResult::Running;
        }

        // Se a Main está vazia, mas há workers rodando ou tarefas na fila, reporta Running.
        if self.active_workers.load(Ordering::SeqCst) > 0 {
            return ExecutorStepResult::Running;
        }

        // Se a Main e Workers estão vazios, reporta Blocked.
        ExecutorStepResult::Blocked { timeout: None }
    }
}

impl ExecutionDriver for NativeDriver {
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
        limits: &LimitsMetadata,
    ) -> Result<(), galfus_runtime::driver::EventDeliveryError> {
        self.event_bridge.configure_limit(limits.max_event_queue)
    }
}

impl Default for NativeDriver {
    fn default() -> Self {
        Self::new()
    }
}
