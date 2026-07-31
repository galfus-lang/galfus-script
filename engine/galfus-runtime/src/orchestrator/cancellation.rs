use super::Orchestrator;
use crate::orchestrator::pending::PendingOperation;
use std::sync::atomic::Ordering;

impl Orchestrator {
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
                        host.cancel(thread_id.raw() as usize, pending.request_id);
                    }
                }
                PendingOperation::Adapter { module, symbol } => {
                    if let Some(adapters) = &self.adapters {
                        adapters.lock().unwrap().cancel(
                            &module,
                            &symbol,
                            thread_id.raw() as usize,
                            pending.request_id,
                        );
                    }
                }
                PendingOperation::Future => {}
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
