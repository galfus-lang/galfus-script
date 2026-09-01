use super::*;

use crate::orchestrator::pending::{PendingContinuation, PendingKey, PendingOperation};
use crate::task::execution_stack;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};
use std::sync::{Arc, atomic::AtomicBool};

impl Orchestrator {
    pub(super) fn handle_future_dropped(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        future_ids: Vec<galfus_core::FutureId>,
    ) {
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.dropped += future_ids.len();
        }
        for future_id in future_ids {
            self.remove_mailbox_future_wait(thread_id, future_id);
            let disposition = match self.future_registry.discard(thread_id, future_id) {
                Ok(disposition) => disposition,
                Err(error) => {
                    self.failure = Some(error.with_stack(execution_stack(&thread)));
                    self.kernel.cancel(thread_id);
                    return;
                }
            };
            match disposition {
                crate::orchestrator::future_registry::DiscardDisposition::Running(activation) => {
                    self.cancel_future_activation(thread_id, future_id, activation);
                    self.free_future_id(future_id);
                }
                crate::orchestrator::future_registry::DiscardDisposition::Created(activation) => {
                    if let crate::orchestrator::future_registry::Activation::GalfusFunction {
                        args,
                        ..
                    } = activation
                    {
                        for arg in args {
                            if let galfus_vm::VmValue::Object(obj_ref) = arg {
                                let _ = thread.heap.release_anchor(obj_ref);
                            }
                        }
                    }
                    self.free_future_id(future_id);
                }
                crate::orchestrator::future_registry::DiscardDisposition::Terminal => {
                    self.free_future_id(future_id);
                }
                crate::orchestrator::future_registry::DiscardDisposition::Retained => {}
            }
        }
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
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.awaited += 1;
        }
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

        if aggregate_registration.is_none() {
            let inline_activation = match self
                .future_registry
                .take_inline_galfus_activation(thread_id, future_id)
            {
                Ok(activation) => activation,
                Err(error) => {
                    self.failure = Some(error.with_stack(execution_stack(&thread)));
                    self.kernel.cancel(thread_id);
                    return;
                }
            };

            if let Some(crate::orchestrator::future_registry::Activation::GalfusFunction {
                module_id,
                func_idx,
                args,
            }) = inline_activation
            {
                // Execute inline!
                let target_func = match self.vm.as_ref().unwrap().get_function(module_id, func_idx)
                {
                    Ok(f) => f,
                    Err(error) => {
                        self.failure = Some(
                            ExecutionFailure::new(ExecutionFailureKind::VmPanic, error.to_string())
                                .with_thread_id(thread_id)
                                .with_stack(execution_stack(&thread)),
                        );
                        self.kernel.cancel(thread_id);
                        return;
                    }
                };
                let register_count = target_func.param_count as usize
                    + target_func.local_count as usize
                    + target_func.temp_count as usize;
                let cached_instructions = target_func.instructions.as_slice() as *const _;
                if let Err(error) = thread.push_frame(
                    module_id,
                    func_idx,
                    0,
                    waiter.continuation.continuation.dest(),
                    register_count,
                    cached_instructions,
                ) {
                    self.failure = Some(
                        ExecutionFailure::new(ExecutionFailureKind::VmPanic, error.to_string())
                            .with_thread_id(thread_id)
                            .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                }
                let register_base = thread.call_stack.last().unwrap().register_base;
                for (i, val) in args.into_iter().enumerate() {
                    // No need to retain_anchor_val, because it was already retained when future was created, and we just move ownership to the registers.
                    thread.registers[register_base + i] = val;
                }

                // Unpark thread to continue executing
                if let Err(e) = self.kernel.enqueue_runnable(thread_id, thread) {
                    self.failure = Some(
                        ExecutionFailure::new(e, "runnable threads limit exceeded")
                            .with_thread_id(thread_id),
                    );
                    self.kernel.cancel(thread_id);
                }
                return;
            }
        }
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
            Ok(()) => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.blocked_threads += 1;
                }
                true
            }
            Err(error) => {
                self.failure = Some(error.with_thread_id(thread_id).with_stack(stack));
                self.kernel.cancel(thread_id);
                false
            }
        }
    }
}
