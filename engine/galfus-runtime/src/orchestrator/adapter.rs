use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, MessageInjector, RunnableTask,
    ThreadResult,
};
#[cfg(test)]
use galfus_contract::{KernelTask, TaskAffinity};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn restore_adapter_module(
    bindings: &Mutex<galfus_contract::AdapterBindings>,
    proxy_module: &str,
    module: Box<dyn galfus_contract::AdapterModuleBinding>,
) {
    match bindings.lock() {
        Ok(mut bindings) => {
            let _ = bindings.restore_module(proxy_module, module);
        }
        Err(poisoned) => {
            // No adapter code runs while this registry is locked. Recover the table so the
            // detached module is never lost merely because an unrelated callback poisoned it.
            let mut guard = poisoned.into_inner();
            let _ = guard.restore_module(proxy_module, module);
            drop(guard);
            bindings.clear_poison();
        }
    }
}

pub(crate) struct ProviderDispatchTask {
    pub(crate) providers: Arc<std::sync::Mutex<galfus_contract::Providers>>,
    pub(crate) thread_id: galfus_core::ThreadId,
    pub(crate) request_lease: galfus_core::RequestLease,
    pub(crate) alias: String,
    pub(crate) name: String,
    pub(crate) args: crate::orchestrator::future_registry::ProviderArguments,
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
                let _ = self.injector.inject_system_response(
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
            let _ = self.injector.inject_system_response(
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
        restore_adapter_module(&self.bindings, &self.module, module);
        ThreadResult::Discarded
    }
    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        // Bound adapters are main-thread-only. Adapters may use the injector
        // from their own workers, but dispatch itself never leaves this lane.
        None
    }
}

impl ProviderDispatchTask {
    #[cfg(test)]
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
        let host_arc = match self.providers.lock() {
            Ok(providers) => providers.get_host(&self.alias),
            Err(_) => {
                let _ = self.injector.inject_system_response(
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
        let Some(host_arc) = host_arc else {
            let _ = self.injector.inject_system_response(
                self.thread_id,
                self.request_lease,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::MissingProvider,
                    "HostProvider missing",
                )),
            );
            return ThreadResult::Discarded;
        };

        let mut host = match host_arc.lock() {
            Ok(host) => host,
            Err(_) => {
                let _ = self.injector.inject_system_response(
                    self.thread_id,
                    self.request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::InternalRuntimeFailure,
                        "provider lock is poisoned",
                    )),
                );
                return ThreadResult::Discarded;
            }
        };
        match self.args {
            crate::orchestrator::future_registry::ProviderArguments::Boundary(args) => {
                host.dispatch(
                    self.thread_id,
                    self.request_lease,
                    &self.name,
                    &args,
                    self.injector.clone(),
                );
            }
            crate::orchestrator::future_registry::ProviderArguments::Surface(args) => {
                if !host.dispatch_surface(
                    self.thread_id,
                    self.request_lease,
                    &self.name,
                    &args,
                    self.injector.clone(),
                ) {
                    let _ = self.injector.inject_system_response(
                        self.thread_id,
                        self.request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "provider does not implement the declared surface contract",
                        )),
                    );
                }
            }
        }
        ThreadResult::Discarded
    }

    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        Some(self)
    }
}
