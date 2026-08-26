use super::*;

use crate::orchestrator::pending::{PendingContinuation, PendingKey, PendingOperation};
use crate::task::execution_stack;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};
use std::sync::{Arc, atomic::AtomicBool};

impl Orchestrator {
    pub(super) fn handle_future_dropped(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        future_id: galfus_core::FutureId,
    ) {
        self.future_metrics.dropped += 1;
        self.remove_mailbox_future_wait(thread_id, future_id);
        let disposition = match self.future_registry.discard(thread_id, future_id) {
            Ok(disposition) => disposition,
            Err(error) => {
                self.failure = Some(error.with_stack(execution_stack(&thread)));
                self.kernel.cancel(thread_id);
                return;
            }
        };
        if let crate::orchestrator::future_registry::DiscardDisposition::Running(activation) =
            disposition
        {
            self.cancel_future_activation(thread_id, future_id, activation);
        }
        self.free_future_id(future_id);
        self.resume_or_fail_front(thread_id, thread, continuation, galfus_vm::VmValue::Null);
    }

    pub(super) fn handle_future_wait(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        future_id: galfus_core::FutureId,
        module_id: galfus_core::ModuleId,
        return_type: galfus_bytecode::instruction::TypeIdx,
    ) {
        self.future_metrics.awaited += 1;
        let aggregate_registration = self.aggregate_registration.take();
        let waiter = crate::orchestrator::future_registry::Waiter {
            continuation: PendingContinuation {
                thread_id,
                continuation,
                module_id,
                return_type,
                stack: execution_stack(&thread),
                operation: aggregate_registration.map_or(
                    PendingOperation::Future,
                    |(coordinator_id, index)| PendingOperation::AggregateMember {
                        coordinator_id,
                        index,
                    },
                ),
                active: Arc::new(AtomicBool::new(true)),
            },
        };
        let disposition = match self
            .future_registry
            .add_waiter(thread_id, future_id, waiter)
        {
            Ok(disposition) => disposition,
            Err(error) => {
                self.failure = Some(error.with_stack(execution_stack(&thread)));
                self.kernel.cancel(thread_id);
                return;
            }
        };

        if let crate::orchestrator::future_registry::WaitDisposition::Resolved { waiter, result } =
            disposition
        {
            if aggregate_registration.is_none() && !self.block_or_fail(thread_id, thread) {
                return;
            }
            self.resume_pending(
                thread_id,
                waiter.continuation,
                result,
                PendingKey::Future(future_id),
            );
            return;
        }
        if matches!(
            disposition,
            crate::orchestrator::future_registry::WaitDisposition::Discarded
        ) {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::InvalidContinuation,
                    "discarded future cannot be awaited",
                )
                .with_thread_id(thread_id)
                .with_future_id(future_id)
                .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return;
        }

        let activation = match self
            .future_registry
            .take_activation_for_start(thread_id, future_id)
        {
            Ok(activation) => activation,
            Err(error) => {
                self.failure = Some(error.with_stack(execution_stack(&thread)));
                self.kernel.cancel(thread_id);
                return;
            }
        };

        if let Some(activation) = activation {
            if let Some(t) = self.start_activation(
                thread_id,
                thread,
                future_id,
                activation,
                aggregate_registration,
            ) {
                thread = t;
            } else {
                return;
            }
        }
        if aggregate_registration.is_none() && !self.block_or_fail(thread_id, thread) {}
    }

    pub(super) fn block_or_fail(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
    ) -> bool {
        let stack = execution_stack(&thread);
        match self.kernel.block(thread_id, thread, None) {
            Ok(()) => true,
            Err(error) => {
                self.failure = Some(error.with_thread_id(thread_id).with_stack(stack));
                self.kernel.cancel(thread_id);
                false
            }
        }
    }
}
