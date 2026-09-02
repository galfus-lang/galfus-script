use super::*;

use crate::execution::FutureCompletionInjector;
use crate::orchestrator::adapter::AdapterDispatchTask;
use crate::task::execution_stack;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind, KernelTask};
use std::sync::Arc;

impl Orchestrator {
    pub(super) fn handle_adapter_handle_dropped(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        binding_id: galfus_core::BindingId,
        type_id: galfus_core::OpaqueTypeId,
        id: galfus_core::HandleId,
    ) {
        if let Err(error) = self.release_adapter_handle(binding_id, type_id, id) {
            self.failure = Some(
                ExecutionFailure::new(ExecutionFailureKind::AdapterCallFailure, error.to_string())
                    .with_thread_id(thread_id)
                    .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return;
        }
        self.resume_or_fail_front(thread_id, thread, continuation, galfus_vm::VmValue::Null);
    }

    pub(super) fn start_adapter_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        future_id: galfus_core::FutureId,
        proxy_module: String,
        symbol: String,
        args: Vec<galfus_contract::BoundaryValue>,
    ) -> Option<galfus_vm::thread::VmThreadState> {
        let Some(bindings) = self.adapter_bindings.clone() else {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::MissingAdapter,
                    "adapter registry missing",
                )
                .with_thread_id(thread_id)
                .with_future_id(future_id)
                .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return None;
        };
        let has_module = match bindings.lock() {
            Ok(bindings) => bindings.has_module(&proxy_module),
            Err(_) => {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::InternalRuntimeFailure,
                        "adapter registry lock is poisoned",
                    )
                    .with_thread_id(thread_id)
                    .with_future_id(future_id)
                    .with_stack(execution_stack(&thread)),
                );
                self.kernel.cancel(thread_id);
                return None;
            }
        };
        if !has_module {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::MissingAdapter,
                    "adapter symbol missing",
                )
                .with_thread_id(thread_id)
                .with_future_id(future_id)
                .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return None;
        }
        let request_lease = self.allocate_request_lease(thread_id, future_id, &thread)?;
        if let Err(error) =
            self.future_registry
                .assign_request_id(thread_id, future_id, request_lease.id)
        {
            self.failure = Some(error.with_stack(execution_stack(&thread)));
            self.kernel.cancel(thread_id);
            return None;
        }
        let task = AdapterDispatchTask {
            bindings,
            thread_id,
            request_lease,
            module: proxy_module,
            symbol,
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
                None,
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
        let task = Box::new(crate::task::QuotaTask::new(task, self.quota.clone()));
        self.driver
            .as_ref()
            .expect("driver is configured before execution")
            .dispatch(KernelTask::Main(task));
        Some(thread)
    }
}
