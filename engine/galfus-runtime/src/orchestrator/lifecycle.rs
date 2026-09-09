use super::*;

use crate::task::{execution_stack, with_execution_stack};
use galfus_contract::ExecutionFailure;

impl Orchestrator {
    pub(crate) fn free_future_id(&mut self, future_id: galfus_core::FutureId) {
        self.future_id_manager.free(future_id);
        self.quota.lock().unwrap().release_futures(1);
    }

    pub(crate) fn free_request_id(&mut self, request_id: galfus_core::RequestId) {
        self.request_id_manager.free(request_id);
        self.quota.lock().unwrap().release_pending_requests(1);
    }

    pub(super) fn resume_or_fail_front(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        value: galfus_vm::VmValue,
    ) {
        let result = self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .resume(thread_id, &mut thread, continuation, value);
        match result {
            Ok(()) => {
                if let Err(e) = self.kernel.enqueue_runnable_front(thread_id, thread) {
                    self.failure = Some(
                        ExecutionFailure::new(e, "runnable threads limit exceeded")
                            .with_thread_id(thread_id),
                    );
                    self.cancel_and_teardown_thread(thread_id);
                } else {
                    self.dispatch_runnables();
                }
            }
            Err(error) => {
                self.failure = Some(with_execution_stack(
                    error.with_thread_id(thread_id),
                    execution_stack(&thread),
                ));
                self.cancel_and_teardown_thread(thread_id);
            }
        }
    }

    pub(crate) fn cancel_and_teardown_thread(&mut self, thread_id: crate::registry::ThreadId) {
        if let Some(mut thread) = self.kernel.take_thread(thread_id) {
            self.teardown_thread_handles(&mut thread);
        }
        self.kernel.cancel(thread_id);
    }

    pub(crate) fn cancel_and_teardown_all_threads(&mut self) {
        let thread_ids = self
            .kernel
            .debug_states()
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for thread_id in thread_ids {
            self.cancel_and_teardown_thread(thread_id);
        }
    }

    pub(crate) fn step(&mut self, _budget: usize) -> galfus_contract::ThreadResult {
        self.process_events();
        self.dispatch_runnables();

        if self.failure.is_some() {
            return galfus_contract::ThreadResult::Discarded;
        }

        if self.kernel.active_count() == 0 {
            let result = self
                .root_thread_id
                .and_then(|id| self.kernel.state(id))
                .and_then(|state| state.exit_reason());

            return match result {
                Some(Ok(value)) => galfus_contract::ThreadResult::Completed(Ok(value)),
                Some(Err(error)) => galfus_contract::ThreadResult::Completed(Err(error)),
                None => galfus_contract::ThreadResult::Completed(Ok(0)),
            };
        }

        galfus_contract::ThreadResult::Discarded
    }

    pub(crate) fn debug_states(
        &self,
    ) -> Vec<(crate::registry::ThreadId, crate::registry::ThreadState)> {
        self.kernel.debug_states()
    }
}
