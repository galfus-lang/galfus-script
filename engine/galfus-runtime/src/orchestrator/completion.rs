use super::*;

use crate::orchestrator::adapter_handles::stamp_adapter_handles;
use crate::orchestrator::pending::{
    LateCompletion, MAX_LATE_COMPLETIONS, PendingContinuation, PendingKey, PendingOperation,
};
use crate::task::with_execution_stack;
use galfus_contract::{BoundaryValue, ExecutionFailure, ExecutionFailureKind};

impl Orchestrator {
    pub(super) fn record_late_completion(
        &mut self,
        thread_id: crate::registry::ThreadId,
        key: crate::orchestrator::pending::PendingKey,
    ) {
        if self.late_completions.len() == MAX_LATE_COMPLETIONS {
            self.late_completions.pop_front();
        }
        self.late_completions
            .push_back(LateCompletion { thread_id, key });
    }

    #[cfg(test)]
    pub(super) fn late_completion_count(&self) -> usize {
        self.late_completions.len()
    }

    pub(super) fn complete_pending(
        &mut self,
        thread_id: crate::registry::ThreadId,
        key: PendingKey,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let Some(pending) = self.pending_continuations.remove(&key) else {
            self.completion_metrics.unknown_request += 1;
            self.record_late_completion(thread_id, key);
            return;
        };
        if pending.thread_id != thread_id {
            self.pending_continuations.insert(key, pending);
            self.completion_metrics.unknown_request += 1;
            self.record_late_completion(thread_id, key);
            return;
        }
        self.resume_pending(thread_id, pending, result, key);
        self.completion_metrics.accepted += 1;
        if let PendingKey::Request(request_id) = key {
            self.free_request_id(request_id);
        }
    }

    pub(super) fn resume_pending(
        &mut self,
        thread_id: crate::registry::ThreadId,
        pending: PendingContinuation,
        result: Result<BoundaryValue, ExecutionFailure>,
        key: PendingKey,
    ) {
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.resumed_continuations += 1;
        }
        if let PendingOperation::AggregateMember {
            coordinator_id,
            index,
        } = pending.operation
        {
            self.complete_aggregate_member(coordinator_id, index, result);
            return;
        }
        let _was_unblocked = match self.kernel.unblock(thread_id) {
            Ok(was_unblocked) => was_unblocked,
            Err(e) => {
                self.failure = Some(
                    ExecutionFailure::new(e, "runnable threads limit exceeded")
                        .with_thread_id(thread_id),
                );
                self.cancel_and_teardown_thread(thread_id);
                return;
            }
        };
        #[cfg(feature = "metrics")]
        if _was_unblocked {
            self.future_metrics.unblocked_threads += 1;
        }
        let Some(mut thread) = self.kernel.take_thread(thread_id) else {
            return;
        };
        let vm = self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .clone();
        let with_pending_id = |failure: ExecutionFailure| match key {
            PendingKey::Request(request_id) => {
                failure.with_request_lease(galfus_core::RequestLease::new(
                    request_id,
                    self.request_generations
                        .get(&request_id.raw())
                        .copied()
                        .unwrap_or(0),
                ))
            }
            PendingKey::Future(future_id) => failure.with_future_id(future_id),
            PendingKey::Coordinator(_) => failure,
        };
        match result {
            Ok(value) => {
                let module = &vm
                    .graph
                    .get(pending.module_id)
                    .expect("asynchronous call module is loaded")
                    .module;
                let value = match crate::task::encode_into_thread_heap(
                    &mut thread.heap,
                    value,
                    pending.return_type,
                    pending.module_id,
                    module,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.failure = Some(
                            with_pending_id(ExecutionFailure::new(
                                ExecutionFailureKind::BoundaryCodecFailure,
                                format!("invalid asynchronous result: {error:?}"),
                            ))
                            .with_thread_id(thread_id)
                            .with_module_id(pending.module_id.raw().into())
                            .with_stack(pending.stack.clone()),
                        );
                        self.cancel_and_teardown_thread(thread_id);
                        return;
                    }
                };
                self.resume_or_fail_front(thread_id, thread, pending.continuation, value);
            }
            Err(error) => {
                let error = with_execution_stack(
                    with_pending_id(error)
                        .with_thread_id(thread_id)
                        .with_module_id(pending.module_id.raw().into()),
                    pending.stack,
                );
                self.failure = Some(match thread.initializing_module() {
                    Some(initializing_module_id) => ExecutionFailure::new(
                        ExecutionFailureKind::InitializationFailure,
                        "module initializer asynchronous request failed",
                    )
                    .with_thread_id(thread_id)
                    .with_module_id(initializing_module_id.raw().into())
                    .with_cause(error),
                    None => error,
                });
                self.cancel_and_teardown_thread(thread_id);
            }
        }
    }

    pub(super) fn complete_future(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        mut result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let adapter_proxy_module = self
            .future_registry
            .adapter_proxy_module(thread_id, future_id);
        let request_id = self.future_registry.request_id(thread_id, future_id);

        let binding_id = adapter_proxy_module.as_deref().and_then(|proxy_module| {
            self.adapter_bindings
                .as_ref()
                .and_then(|bindings| bindings.lock().ok()?.binding_id(proxy_module))
        });
        if result.as_mut().is_ok_and(|value| {
            !stamp_adapter_handles(value, adapter_proxy_module.as_deref(), binding_id)
        }) {
            result = Err(ExecutionFailure::new(
                ExecutionFailureKind::BoundaryCodecFailure,
                "adapter returned a handle for a different binding or opaque type namespace",
            ));
        }

        let is_direct_await = self.future_registry.is_direct_await(thread_id, future_id);

        if !is_direct_await
            && let (Some((payload_module_id, payload_type)), Ok(value)) = (
                self.future_registry.payload_schema(thread_id, future_id),
                &result,
            )
        {
            let module = &self
                .vm
                .as_ref()
                .expect("VM is configured before execution")
                .graph
                .get(payload_module_id)
                .expect("future payload module is loaded")
                .module;
            let thread_quota = std::sync::Arc::new(galfus_vm::quota::ThreadQuota::new(
                self.quota.lock().unwrap().limits().clone(),
            ));
            let mut payload_heap = galfus_vm::thread::PrivateHeap::new(thread_quota);
            if let Err(error) = crate::task::encode_into_thread_heap(
                &mut payload_heap,
                value.clone(),
                payload_type,
                payload_module_id,
                module,
            ) {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::BoundaryCodecFailure,
                        format!("invalid future worker result: {error:?}"),
                    )
                    .with_thread_id(thread_id)
                    .with_future_id(future_id)
                    .with_module_id(payload_module_id.raw().into()),
                );
                self.cancel_and_teardown_thread(thread_id);
                return;
            }
        }
        let waiters = match self
            .future_registry
            .complete(thread_id, future_id, result.clone())
        {
            Ok(waiters) => waiters,
            Err(error) => {
                if error.kind == ExecutionFailureKind::DuplicateCompletion {
                    self.completion_metrics.duplicate += 1;
                    self.record_late_completion(thread_id, PendingKey::Future(future_id));
                    return;
                }
                self.completion_metrics.unknown_request += 1;
                return;
            }
        };
        if let (Some(proxy_module), Ok(value)) = (adapter_proxy_module, &result)
            && let Err(error) = self.register_adapter_handles(&proxy_module, value)
        {
            self.failure = Some(error.with_thread_id(thread_id).with_future_id(future_id));
            self.kernel.cancel(thread_id);
            return;
        }
        for waiter in waiters {
            let waiter_thread_id = waiter.continuation.thread_id;
            self.resume_pending(
                waiter_thread_id,
                waiter.continuation,
                result.clone(),
                PendingKey::Future(future_id),
            );
        }
        if is_direct_await {
            let _ = self.future_registry.discard(thread_id, future_id);
            self.free_future_id(future_id);
        }
        if let Some(request_id) = request_id {
            self.free_request_id(request_id);
        }
        self.completion_metrics.accepted += 1;
    }
}
