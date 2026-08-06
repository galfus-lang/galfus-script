use super::Orchestrator;
use crate::orchestrator::future_registry::Activation;
use crate::orchestrator::pending::PendingOperation;
use std::sync::atomic::Ordering;

impl Orchestrator {
    pub(super) fn cancel_thread_futures(&mut self, thread_id: crate::registry::ThreadId) {
        for (future_id, activation) in self.future_registry.discard_all_for_owner(thread_id) {
            self.cancel_future_activation(thread_id, future_id, activation);
        }
    }

    pub(super) fn cancel_all_futures(&mut self) {
        for (thread_id, future_id, activation) in self.future_registry.discard_all() {
            self.cancel_future_activation(thread_id, future_id, activation);
        }
    }

    pub(super) fn cancel_future_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: u64,
        activation: Activation,
    ) {
        match activation {
            Activation::Provider { .. } => {
                let Some(vm) = self.vm.as_ref() else {
                    return;
                };
                let Some(providers) = vm.providers() else {
                    return;
                };
                if let Some(host) = providers.lock().unwrap().host_mut() {
                    let _outcome = host.cancel(thread_id.raw() as usize, future_id);
                }
            }
            Activation::Adapter {
                proxy_module,
                symbol,
                ..
            } => {
                if let Some(bindings) = &self.adapter_bindings {
                    let _outcome = bindings.lock().unwrap().cancel(
                        &proxy_module,
                        &symbol,
                        thread_id.raw() as usize,
                        future_id,
                    );
                }
            }
            Activation::GalfusFunction { .. } => {
                let workers = self
                    .future_workers
                    .iter()
                    .filter_map(|(&worker_id, &(owner_id, id))| {
                        (owner_id == thread_id && id == future_id).then_some(worker_id)
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
            match pending.operation {
                PendingOperation::Provider => {
                    let Some(vm) = self.vm.as_ref() else {
                        continue;
                    };
                    let Some(providers) = vm.providers() else {
                        continue;
                    };
                    let mut providers = providers.lock().unwrap();
                    if let Some(host) = providers.host_mut() {
                        let _outcome = host.cancel(thread_id.raw() as usize, pending.request_id);
                    }
                }
                PendingOperation::Adapter { module, symbol } => {
                    if let Some(bindings) = &self.adapter_bindings {
                        let _outcome = bindings.lock().unwrap().cancel(
                            &module,
                            &symbol,
                            thread_id.raw() as usize,
                            pending.request_id,
                        );
                    }
                }
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
}
