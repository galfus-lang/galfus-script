use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, KernelTask, MessageInjector,
    RunnableTask, TaskAffinity, ThreadResult,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct ProviderDispatchTask {
    pub(crate) providers: Arc<std::sync::Mutex<galfus_contract::Providers>>,
    pub(crate) thread_id: galfus_core::ThreadId,
    pub(crate) request_lease: galfus_core::RequestLease,
    pub(crate) name: String,
    pub(crate) args: Vec<BoundaryValue>,
    pub(crate) injector: Arc<dyn MessageInjector>,
    pub(crate) active: Arc<AtomicBool>,
}

pub(crate) struct AdapterDispatchTask {
    pub(crate) bindings: Arc<std::sync::Mutex<galfus_contract::AdapterBindings>>,
    pub(crate) thread_id: galfus_core::ThreadId,
    pub(crate) request_lease: galfus_core::RequestLease,
    pub(crate) module: String,
    pub(crate) symbol: String,
    pub(crate) args: Vec<BoundaryValue>,
    pub(crate) injector: Arc<dyn MessageInjector>,
    pub(crate) active: Arc<AtomicBool>,
}

impl RunnableTask for AdapterDispatchTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        if !self.active.load(Ordering::Acquire) {
            return ThreadResult::Discarded;
        }
        let module = match self.bindings.lock() {
            Ok(mut bindings) => bindings.take_module(&self.module),
            Err(_) => {
                self.injector.inject_system_response(
                    self.thread_id,
                    self.request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::InternalRuntimeFailure,
                        "adapter registry lock is poisoned",
                    )),
                );
                return ThreadResult::Discarded;
            }
        };
        let Some(mut module) = module else {
            self.injector.inject_system_response(
                self.thread_id,
                self.request_lease,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::MissingAdapter,
                    "adapter symbol missing",
                )),
            );
            return ThreadResult::Discarded;
        };
        module.dispatch(
            self.symbol.as_str(),
            self.thread_id,
            self.request_lease,
            &self.args,
            self.injector.clone(),
        );
        if let Ok(mut bindings) = self.bindings.lock() {
            let _ = bindings.restore_module(&self.module, module);
        }
        ThreadResult::Discarded
    }
    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        // Bound adapters are main-thread-only. Adapters may use the injector
        // from their own workers, but dispatch itself never leaves this lane.
        None
    }
}

impl ProviderDispatchTask {
    pub(crate) fn into_kernel_task(self, affinity: TaskAffinity) -> KernelTask {
        match affinity {
            TaskAffinity::Main => KernelTask::Main(Box::new(self)),
            TaskAffinity::Any => KernelTask::Any(Box::new(self)),
        }
    }
}

impl RunnableTask for ProviderDispatchTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        if !self.active.load(Ordering::Acquire) {
            return ThreadResult::Discarded;
        }
        let host = match self.providers.lock() {
            Ok(mut providers) => providers.take_host(),
            Err(_) => {
                self.injector.inject_system_response(
                    self.thread_id,
                    self.request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::InternalRuntimeFailure,
                        "provider registry lock is poisoned",
                    )),
                );
                return ThreadResult::Discarded;
            }
        };
        let Some(mut host) = host else {
            self.injector.inject_system_response(
                self.thread_id,
                self.request_lease,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::MissingProvider,
                    "HostProvider missing while dispatching request",
                )
                .with_request_lease(self.request_lease)),
            );
            return ThreadResult::Discarded;
        };
        host.dispatch(
            self.thread_id,
            self.request_lease,
            self.name.as_str(),
            self.args.as_slice(),
            self.injector.clone(),
        );
        if let Ok(mut providers) = self.providers.lock() {
            providers.restore_host(host);
        }
        ThreadResult::Discarded
    }

    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        Some(self)
    }
}
