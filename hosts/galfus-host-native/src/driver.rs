#[cfg(test)]
mod tests;

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

const DEFAULT_MAX_WORKERS: usize = 4;
const WORKER_STACK_SIZE: usize = 512 * 1024;
const NATIVE_TASK_BUDGET: usize = 100_000;

pub struct NativeDriver {
    main_queue_tx: Sender<KernelTask>,
    main_queue_rx: Receiver<KernelTask>,

    worker_queue_tx: Sender<Box<dyn RunnableTask + Send>>,

    worker_queue_rx: Receiver<Box<dyn RunnableTask + Send>>,
    worker_count: Arc<AtomicUsize>,
    max_workers: usize,

    event_bridge: Arc<NativeEventBridge>,
    active_workers: Arc<AtomicUsize>,

    exit_callback: Mutex<Option<Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>>>,
}

impl NativeDriver {
    pub fn new() -> Self {
        let max_workers = std::thread::available_parallelism()
            .map(|count| count.get().min(DEFAULT_MAX_WORKERS))
            .unwrap_or(DEFAULT_MAX_WORKERS);
        Self::with_max_workers(max_workers)
    }

    pub fn with_max_workers(max_workers: usize) -> Self {
        assert!(max_workers > 0, "NativeDriver requires at least one worker");
        let (main_tx, main_rx) = unbounded();
        let (worker_tx, worker_rx) = unbounded::<Box<dyn RunnableTask + Send>>();
        let active_workers = Arc::new(AtomicUsize::new(0));

        Self {
            main_queue_tx: main_tx,
            main_queue_rx: main_rx,
            worker_queue_tx: worker_tx,
            worker_queue_rx: worker_rx,
            worker_count: Arc::new(AtomicUsize::new(0)),
            max_workers,
            event_bridge: Arc::new(NativeEventBridge::new()),
            active_workers,
            exit_callback: Mutex::new(None),
        }
    }

    fn run_main_task(task: Box<dyn RunnableTask>) -> Option<ExecutorStepResult> {
        let result = task.run(NATIVE_TASK_BUDGET);

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

    fn ensure_worker_capacity(&self, required_workers: usize) -> bool {
        loop {
            let current = self.worker_count.load(Ordering::Acquire);
            if current >= required_workers.min(self.max_workers) {
                return true;
            }
            if self
                .worker_count
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }

            let receiver = self.worker_queue_rx.clone();
            let active_workers = self.active_workers.clone();
            let event_bridge = self.event_bridge.clone();
            let worker_count = self.worker_count.clone();
            let spawn_result = thread::Builder::new()
                .name("galfus-runtime-worker".to_string())
                .stack_size(WORKER_STACK_SIZE)
                .spawn(move || {
                    while let Ok(task) = receiver.recv() {
                        let _ = task.run(NATIVE_TASK_BUDGET);
                        active_workers.fetch_sub(1, Ordering::Release);
                        event_bridge.notify_waiters();
                    }
                    worker_count.fetch_sub(1, Ordering::Release);
                });
            if spawn_result.is_err() {
                self.worker_count.fetch_sub(1, Ordering::Release);
                return self.worker_count.load(Ordering::Acquire) > 0;
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
                let active_workers = self.active_workers.fetch_add(1, Ordering::AcqRel) + 1;
                if self.ensure_worker_capacity(active_workers) {
                    let _ = self.worker_queue_tx.send(t);
                } else {
                    self.active_workers.fetch_sub(1, Ordering::Release);
                    let _ = t.run(NATIVE_TASK_BUDGET);
                }
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

        // Wait for a worker event instead of consuming a CPU core while it runs.
        if self.active_workers.load(Ordering::SeqCst) > 0 {
            let active_workers = self.active_workers.clone();
            self.event_bridge
                .wait_for_event_or(move || active_workers.load(Ordering::Acquire) == 0);
            if self.event_bridge.has_pending() {
                return ExecutorStepResult::Running;
            }
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

    fn available_task_capacity(&self) -> usize {
        self.max_workers
            .saturating_sub(self.active_workers.load(Ordering::Acquire))
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
