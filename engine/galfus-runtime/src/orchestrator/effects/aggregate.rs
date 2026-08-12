use super::*;

use crate::task::execution_stack;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};

impl Orchestrator {
    pub(super) fn begin_aggregate_wait(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        module_id: ModuleId,
        return_type: TypeIdx,
        future_ids: Vec<galfus_core::FutureId>,
        mode: crate::orchestrator::AggregateMode,
    ) {
        if let Err(error) = self.quota.lock().unwrap().try_reserve_pending_states(1) {
            self.failure = Some(
                ExecutionFailure::new(error, "pending states limit exceeded")
                    .with_thread_id(thread_id)
                    .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return;
        }
        let Some(coordinator_id) = self.allocate_coordinator_id(thread_id, &thread) else {
            self.quota.lock().unwrap().release_pending_states(1);
            return;
        };
        self.aggregate_coordinators.insert(
            coordinator_id,
            crate::orchestrator::AggregateCoordinator {
                mode,
                future_ids: future_ids.clone(),
                pending: PendingContinuation {
                    thread_id,
                    continuation,
                    module_id,
                    return_type,
                    stack: execution_stack(&thread),
                    operation: PendingOperation::Future,
                    active: Arc::new(AtomicBool::new(true)),
                },
                results: vec![None; future_ids.len()],
                winner: None,
                armed: false,
            },
        );

        for (index, future_id) in future_ids.into_iter().enumerate() {
            let Some((_, member_return_type)) =
                self.future_registry.payload_schema(thread_id, future_id)
            else {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::InvalidContinuation,
                        "aggregate member has no payload schema",
                    )
                    .with_thread_id(thread_id)
                    .with_future_id(future_id)
                    .with_stack(execution_stack(&thread)),
                );
                self.aggregate_coordinators.remove(&coordinator_id);
                self.quota.lock().unwrap().release_pending_states(1);
                self.kernel.cancel(thread_id);
                return;
            };
            self.aggregate_registration = Some((coordinator_id, index));
            let thread_quota = std::sync::Arc::new(std::sync::Mutex::new(
                galfus_vm::quota::ThreadQuota::new(self.quota.lock().unwrap().limits().clone()),
            ));
            self.handle_effect(
                thread_id,
                galfus_vm::thread::VmThreadState::new(self.quota.clone(), thread_quota),
                galfus_vm::VmEffect::FutureWait {
                    future_id,
                    module_id,
                    return_type: member_return_type,
                },
                galfus_vm::Continuation::for_provider(
                    galfus_bytecode::instruction::Reg(0),
                    module_id,
                    member_return_type,
                ),
            );
            self.aggregate_registration = None;
            if self.failure.is_some() {
                self.aggregate_coordinators.remove(&coordinator_id);
                self.quota.lock().unwrap().release_pending_states(1);
                self.kernel.cancel(thread_id);
                return;
            }
        }

        if let Some(coordinator) = self.aggregate_coordinators.get_mut(&coordinator_id) {
            coordinator.armed = true;
        }
        if !self.block_or_fail(thread_id, thread) {
            return;
        }
        self.finish_aggregate_if_ready(coordinator_id);
    }

    pub(crate) fn allocate_coordinator_id(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: &galfus_vm::thread::VmThreadState,
    ) -> Option<galfus_core::CoordinatorId> {
        if let Some(id) = self.coordinator_id_manager.try_allocate() {
            Some(id)
        } else {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::IdSpaceExhausted,
                    "aggregate coordinator id space exhausted",
                )
                .with_thread_id(thread_id)
                .with_stack(execution_stack(thread)),
            );
            self.kernel.cancel(thread_id);
            None
        }
    }
}
