use super::*;

use crate::event::RuntimeEvent;
use crate::task::RuntimeTask;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind, KernelTask};

impl Orchestrator {
    #[cfg(test)]
    pub(crate) fn submit_event(&mut self, event: RuntimeEvent) {
        let sequence = self
            .pending_events
            .last_key_value()
            .map(|(sequence, _)| sequence.next().expect("event sequence space exhausted"))
            .unwrap_or(self.next_event_sequence);
        self.quota
            .lock()
            .unwrap()
            .try_reserve_event_queue(1)
            .unwrap();
        self.pending_events.insert(sequence, event);
    }

    /// Dispatches all currently runnable threads from the VirtualKernel to the driver.
    pub(crate) fn dispatch_runnables(&mut self) {
        let mut remaining = self
            .driver
            .as_ref()
            .map_or(1, |driver| driver.available_task_capacity());
        while remaining != 0 {
            let Some((thread_id, is_front)) = self.kernel.next_runnable_detailed() else {
                return;
            };
            let Some(thread) = self.kernel.take_thread(thread_id) else {
                continue;
            };
            self.kernel.mark_running(thread_id);
            #[cfg(feature = "metrics")]
            {
                self.future_metrics.dispatched_threads += 1;
                if is_front {
                    self.future_metrics.front_dispatched_threads += 1;
                }
            }

            if let Err(e) = self.quota.lock().unwrap().try_reserve_kernel_tasks(1) {
                self.failure = Some(
                    ExecutionFailure::new(e, "kernel tasks limit exceeded")
                        .with_thread_id(thread_id),
                );
                self.kernel.cancel(thread_id);
                return;
            }

            let task = Box::new(crate::task::QuotaTask::new(
                RuntimeTask::new(
                    thread_id,
                    thread,
                    self.vm.as_ref().unwrap().clone(),
                    self.event_sink
                        .as_ref()
                        .expect("event sink is configured before execution")
                        .clone(),
                    self.future_workers.get(&thread_id).copied(),
                ),
                self.quota.clone(),
            ));

            let kernel_task = KernelTask::Any(task);
            if is_front {
                self.driver.as_ref().unwrap().dispatch_front(kernel_task);
            } else {
                self.driver.as_ref().unwrap().dispatch(kernel_task);
            }
            remaining = remaining.saturating_sub(1);
        }
    }

    /// Processes all pending events in the queue without blocking.
    pub(crate) fn process_events(&mut self) {
        let events = self
            .driver
            .as_ref()
            .map(|driver| driver.drain_events())
            .unwrap_or_default();
        for (sequence, event) in events {
            if sequence < self.next_event_sequence {
                continue;
            }
            if let Err(e) = self.quota.lock().unwrap().try_reserve_event_queue(1) {
                self.failure = Some(ExecutionFailure::new(e, "event queue quota exceeded"));
                return;
            }
            if self.pending_events.insert(sequence, event).is_some() {
                self.failure = Some(ExecutionFailure::new(
                    ExecutionFailureKind::InvalidContinuation,
                    format!("duplicate external event sequence {}", sequence.0),
                ));
                return;
            }
        }

        while let Some(event) = self.pending_events.remove(&self.next_event_sequence) {
            self.quota.lock().unwrap().release_event_queue(1);
            let sequence = self.next_event_sequence;
            self.next_event_sequence = self
                .next_event_sequence
                .next()
                .expect("event sequence space exhausted");
            self.active_event_sequence = Some(sequence);
            self.process_event(event);
            if self.failure.is_none() {
                self.finish_pending_aggregates();
            }
            self.active_event_sequence = None;
            if self.failure.is_some() {
                return;
            }
        }
    }

    pub(super) fn process_event(&mut self, event: RuntimeEvent) {
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.runtime_events += 1;
        }
        match event {
            RuntimeEvent::ThreadSpawned { mut thread } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.thread_spawned_events += 1;
                }
                self.flush_thread_handle_drops(&mut thread);
                let id = match self.kernel.spawn(thread, None) {
                    Ok(id) => id,
                    Err(error) => {
                        self.failure = Some(error);
                        self.cancel_and_teardown_all_threads();
                        return;
                    }
                };
                let thread = self
                    .kernel
                    .take_thread(id)
                    .expect("spawned thread is registered");
                if let Err(e) = self.kernel.enqueue_runnable(id, thread) {
                    self.failure = Some(
                        ExecutionFailure::new(e, "runnable threads limit exceeded")
                            .with_thread_id(id),
                    );
                    self.cancel_and_teardown_thread(id);
                }
            }
            RuntimeEvent::Exited {
                thread_id,
                mut thread,
                result,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.thread_exited_events += 1;
                }
                self.cancel_thread_futures(thread_id);
                self.teardown_thread_handles(&mut thread);
                self.kernel.mark_exited(thread_id, thread, result.clone());
                if let Some(waiters) = self.thread_exit_waits.remove(&thread_id) {
                    for (owner_thread_id, future_lease) in waiters {
                        self.process_event(RuntimeEvent::FutureCompleted {
                            thread_id: owner_thread_id,
                            future_lease,
                            result: result.clone(),
                        });
                    }
                }
            }
            RuntimeEvent::Initialized {
                thread_id,
                mut thread,
                module_id,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.initialized_events += 1;
                }
                self.flush_thread_handle_drops(&mut thread);
                self.advance_startup(thread_id, thread, module_id)
            }
            RuntimeEvent::Failed { thread_id, error } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.thread_failed_events += 1;
                }
                self.failure = Some(error.with_thread_id(thread_id));
                self.cancel_pending_continuations(thread_id);
                self.cancel_thread_futures(thread_id);
                self.startup_plans.remove(&thread_id);
                self.cancel_and_teardown_thread(thread_id);
            }
            RuntimeEvent::EffectCompleted {
                thread_id,
                request_lease,
                result,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.effect_completed_events += 1;
                }
                if request_lease.generation
                    == self
                        .request_generations
                        .get(&request_lease.id.raw())
                        .copied()
                        .unwrap_or(0)
                {
                    self.complete_pending(thread_id, PendingKey::Request(request_lease.id), result)
                } else {
                    self.completion_metrics.late_after_cancel += 1;
                }
            }
            RuntimeEvent::FutureCompleted {
                thread_id,
                future_lease,
                result,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.future_completed_events += 1;
                }
                if future_lease.generation
                    == self
                        .future_generations
                        .get(&future_lease.id.raw())
                        .copied()
                        .unwrap_or(0)
                {
                    self.complete_future(thread_id, future_lease.id, result)
                } else {
                    self.completion_metrics.late_after_cancel += 1;
                }
            }
            RuntimeEvent::FutureWorkerCompleted {
                worker_thread_id,
                owner_thread_id,
                future_lease,
                mut thread,
                result,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.future_worker_completed_events += 1;
                }
                self.future_workers.remove(&worker_thread_id);
                self.teardown_thread_handles(&mut thread);
                self.kernel
                    .mark_exited(worker_thread_id, thread, result.clone());
                if future_lease.generation
                    == self
                        .future_generations
                        .get(&future_lease.id.raw())
                        .copied()
                        .unwrap_or(0)
                {
                    self.complete_future(owner_thread_id, future_lease.id, result);
                }
            }
            RuntimeEvent::Tick { delta_ms } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.tick_events += 1;
                }
                let woke_up = self.kernel.tick(delta_ms);
                for (id, result) in woke_up {
                    if let Err(e) = result {
                        self.failure = Some(
                            ExecutionFailure::new(e, "runnable threads limit exceeded")
                                .with_thread_id(id),
                        );
                        self.cancel_and_teardown_thread(id);
                    }
                }
                self.expire_mailbox_future_waits(delta_ms);
                self.expire_timer_future_waits(delta_ms);
            }
            RuntimeEvent::CancelExecution => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.cancellation_events += 1;
                }
                self.shutdown();
                self.failure = Some(galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::Cancelled,
                    "execution cancelled",
                ));
            }
            RuntimeEvent::Syscall { thread_id, .. } if self.shutting_down => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.syscall_events += 1;
                }
                self.cancel_and_teardown_thread(thread_id);
            }
            RuntimeEvent::Syscall {
                thread_id,
                mut thread,
                effect,
                continuation,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.syscall_events += 1;
                }
                self.flush_thread_handle_drops(&mut thread);
                self.handle_effect(thread_id, thread, effect, continuation);
            }
            RuntimeEvent::Yielded {
                thread_id,
                mut thread,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.yields += 1;
                    self.future_metrics.yield_events += 1;
                }
                self.flush_thread_handle_drops(&mut thread);
                if let Err(e) = self.kernel.enqueue_runnable(thread_id, thread) {
                    self.failure = Some(
                        ExecutionFailure::new(e, "runnable threads limit exceeded")
                            .with_thread_id(thread_id),
                    );
                    self.cancel_and_teardown_thread(thread_id);
                }
            }
            RuntimeEvent::CancelThread { thread_id } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.cancellation_events += 1;
                }
                self.cancel_pending_continuations(thread_id);
                self.cancel_thread_futures(thread_id);
                self.startup_plans.remove(&thread_id);
                self.cancel_and_teardown_thread(thread_id);
            }
        }
    }
}
