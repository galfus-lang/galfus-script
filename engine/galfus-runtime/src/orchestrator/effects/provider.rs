use super::*;

use crate::execution::FutureCompletionInjector;
use crate::orchestrator::adapter::ProviderDispatchTask;
use crate::task::execution_stack;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind, KernelTask};
use std::sync::Arc;

impl Orchestrator {
    pub(super) fn start_provider_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        future_id: galfus_core::FutureId,
        alias: String,
        name: String,
        args: crate::orchestrator::future_registry::ProviderArguments,
    ) -> Option<galfus_vm::thread::VmThreadState> {
        let vm = self.vm.as_ref().expect("VM is configured before execution");
        let Some(providers) = vm.providers() else {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::MissingProvider,
                    "HostProvider missing",
                )
                .with_thread_id(thread_id)
                .with_future_id(future_id)
                .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return None;
        };
        let (affinity, surface_result) = {
            let host_arc = match providers.lock() {
                Ok(providers) => providers.get_host(&alias),
                Err(_) => {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::InternalRuntimeFailure,
                            "provider registry lock is poisoned",
                        )
                        .with_thread_id(thread_id)
                        .with_future_id(future_id)
                        .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return None;
                }
            };
            let Some(host_arc) = host_arc else {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::MissingProvider,
                        "HostProvider missing",
                    )
                    .with_thread_id(thread_id)
                    .with_future_id(future_id)
                    .with_stack(execution_stack(&thread)),
                );
                self.kernel.cancel(thread_id);
                return None;
            };
            let host = match host_arc.lock() {
                Ok(host) => host,
                Err(_) => {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::InternalRuntimeFailure,
                            "provider lock is poisoned",
                        )
                        .with_thread_id(thread_id)
                        .with_future_id(future_id)
                        .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return None;
                }
            };
            let surface_result = host
                .descriptor()
                .modules
                .into_iter()
                .find_map(|module| module.surface_contract(name.as_str()).cloned())
                .map(|contract| contract.result);
            (host.affinity(name.as_str()), surface_result)
        };
        let request_lease = self.allocate_request_lease(thread_id, future_id, &thread)?;
        if let Err(error) =
            self.future_registry
                .assign_request_id(thread_id, future_id, request_lease.id)
        {
            self.failure = Some(error.with_stack(execution_stack(&thread)));
            self.kernel.cancel(thread_id);
            return None;
        }
        let task = ProviderDispatchTask {
            providers,
            thread_id,
            request_lease,
            alias,
            name,
            args,
            injector: Arc::new(FutureCompletionInjector::new(
                self.event_sink
                    .as_ref()
                    .expect("event sink is configured before execution")
                    .clone(),
                thread_id,
                request_lease,
                galfus_core::FutureLease::new(
                    future_id,
                    self.future_generations
                        .get(&future_id.raw())
                        .copied()
                        .unwrap_or(0),
                ),
                surface_result,
            )),
            active: self
                .future_registry
                .active_flag(thread_id, future_id)
                .expect("active future has a registry record"),
        };
        if let Err(e) = self.quota.lock().unwrap().try_reserve_kernel_tasks(1) {
            self.failure = Some(
                ExecutionFailure::new(e, "kernel tasks limit exceeded")
                    .with_thread_id(thread_id)
                    .with_future_id(future_id),
            );
            self.kernel.cancel(thread_id);
            return None;
        }
        let quota_task = crate::task::QuotaTask::new(task, self.quota.clone());
        let kernel_task = match affinity {
            galfus_contract::TaskAffinity::Main => KernelTask::Main(Box::new(quota_task)),
            galfus_contract::TaskAffinity::Any => KernelTask::Any(Box::new(quota_task)),
        };
        self.driver
            .as_ref()
            .expect("driver is configured before execution")
            .dispatch(kernel_task);
        Some(thread)
    }
}
