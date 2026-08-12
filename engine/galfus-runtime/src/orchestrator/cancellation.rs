use super::Orchestrator;
use crate::orchestrator::future_registry::Activation;
use crate::orchestrator::pending::PendingOperation;
use galfus_contract::AdapterBindingsCloseReport;
use std::sync::atomic::Ordering;

impl Orchestrator {
    pub(super) fn cancel_thread_futures(&mut self, thread_id: crate::registry::ThreadId) {
        for (future_id, activation) in self.future_registry.discard_all_for_owner(thread_id) {
            if let Some(activation) = activation {
                self.cancel_future_activation(thread_id, future_id, activation);
            }
            self.free_future_id(future_id);
        }
    }

    pub(super) fn cancel_all_futures(&mut self) {
        for (thread_id, future_id, activation) in self.future_registry.discard_all() {
            if let Some(activation) = activation {
                self.cancel_future_activation(thread_id, future_id, activation);
            }
            self.free_future_id(future_id);
        }
    }

    pub(super) fn cancel_future_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        activation: Activation,
    ) {
        match activation {
            Activation::Provider {
                request_id: Some(request_id),
                ..
            } => {
                let Some(vm) = self.vm.as_ref() else {
                    return;
                };
                let Some(providers) = vm.providers() else {
                    return;
                };
                let host = match providers.lock() {
                    Ok(mut providers) => providers.take_host(),
                    Err(_) => None,
                };
                if let Some(mut host) = host {
                    let generation = self
                        .request_generations
                        .get(&request_id.raw())
                        .copied()
                        .unwrap_or(0);
                    let outcome = host.cancel(
                        thread_id,
                        galfus_core::RequestLease::new(request_id, generation),
                    );
                    self.cancellation_report.record(outcome);
                    if let Ok(mut providers) = providers.lock() {
                        providers.restore_host(host);
                    }
                }
                self.free_request_id(request_id);
            }
            Activation::Provider {
                request_id: None, ..
            } => {}
            Activation::Adapter {
                proxy_module,
                symbol,
                request_id: Some(request_id),
                ..
            } => {
                if let Some(bindings) = &self.adapter_bindings {
                    let generation = self
                        .request_generations
                        .get(&request_id.raw())
                        .copied()
                        .unwrap_or(0);
                    let module = match bindings.lock() {
                        Ok(mut bindings) => bindings.take_module(&proxy_module),
                        Err(_) => None,
                    };
                    if let Some(mut module) = module {
                        let outcome = module.cancel(
                            &symbol,
                            thread_id,
                            galfus_core::RequestLease::new(request_id, generation),
                        );
                        self.cancellation_report.record(outcome);
                        if let Ok(mut bindings) = bindings.lock() {
                            let _ = bindings.restore_module(&proxy_module, module);
                        }
                    }
                }
                self.free_request_id(request_id);
            }
            Activation::Adapter {
                request_id: None, ..
            } => {}
            Activation::GalfusFunction { .. } => {
                let workers = self
                    .future_workers
                    .iter()
                    .filter_map(|(&worker_id, &(owner_id, future_lease))| {
                        (owner_id == thread_id && future_lease.id == future_id).then_some(worker_id)
                    })
                    .collect::<Vec<_>>();
                for worker_id in workers {
                    self.future_workers.remove(&worker_id);
                    self.cancel_and_teardown_thread(worker_id);
                }
            }
            Activation::Internal { .. } => {}
        }
    }

    pub(super) fn cancel_pending_continuations(&mut self, thread_id: crate::registry::ThreadId) {
        let mut request_ids = self
            .pending_continuations
            .iter()
            .filter_map(|(&key, pending)| (pending.thread_id == thread_id).then_some(key))
            .collect::<Vec<_>>();
        request_ids.sort_unstable();
        for key in request_ids {
            let Some(pending) = self.pending_continuations.remove(&key) else {
                continue;
            };
            pending.active.store(false, Ordering::Release);
            if let super::pending::PendingKey::Request(request_id) = key {
                self.free_request_id(request_id);
            }
            match pending.operation {
                PendingOperation::Future | PendingOperation::AggregateMember { .. } => {}
            }
        }
    }

    pub(super) fn cancel_all_pending_continuations(&mut self) {
        let mut thread_ids = self
            .pending_continuations
            .values()
            .map(|pending| pending.thread_id)
            .collect::<Vec<_>>();
        thread_ids.sort_unstable_by_key(|thread_id| thread_id.raw());
        thread_ids.dedup();
        for thread_id in thread_ids {
            self.cancel_pending_continuations(thread_id);
        }
    }

    pub(crate) fn shutdown(&mut self) -> AdapterBindingsCloseReport {
        if self.shutting_down {
            return self.shutdown_report.clone().unwrap_or_default();
        }
        self.shutting_down = true;
        self.cancel_all_pending_continuations();
        self.cancel_all_futures();
        self.startup_plans.clear();
        self.thread_exit_waits.clear();
        self.mailbox_future_waits.clear();
        self.quota.lock().unwrap().release_event_queue(self.pending_events.len());
        self.pending_events.clear();
        self.pending_aggregate_finishes.clear();
        for coordinator_id in self.aggregate_coordinators.drain().map(|(id, _)| id) {
            self.coordinator_id_manager.free(coordinator_id);
        }
        self.cancel_and_teardown_all_threads();
        let report = self.close_adapter_bindings();
        self.shutdown_report = Some(report.clone());
        report
    }
}
