pub(crate) mod activation;
pub(crate) mod adapter;
pub(crate) mod aggregate;
pub(crate) mod future_creation;
pub(crate) mod future_wait;
pub(crate) mod internal;
pub(crate) mod provider;

use super::*;
use crate::orchestrator::pending::{PendingContinuation, PendingOperation};
use galfus_bytecode::instruction::TypeIdx;
use galfus_core::ModuleId;
use std::sync::{Arc, atomic::AtomicBool};

impl Orchestrator {
    pub(super) fn handle_effect(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        effect: galfus_vm::VmEffect,
        continuation: galfus_vm::Continuation,
    ) {
        match effect {
            galfus_vm::VmEffect::FutureDropped { future_id } => {
                self.handle_future_dropped(thread_id, thread, continuation, future_id)
            }
            galfus_vm::VmEffect::AdapterHandleDropped {
                binding_id,
                type_id,
                id,
            } => self.handle_adapter_handle_dropped(
                thread_id,
                thread,
                continuation,
                binding_id,
                type_id,
                id,
            ),
            galfus_vm::VmEffect::FutureWait {
                future_id,
                module_id,
                return_type,
            } => self.handle_future_wait(
                thread_id,
                thread,
                continuation,
                future_id,
                module_id,
                return_type,
            ),
            galfus_vm::VmEffect::CreateFuture {
                module_id,
                target_module_id,
                func_idx,
                args,
                arg_types,
                return_type,
            } => self.handle_create_future(
                thread_id,
                thread,
                continuation,
                module_id,
                target_module_id,
                func_idx,
                args,
                arg_types,
                return_type,
            ),
            galfus_vm::VmEffect::CreateAwaitFuture {
                module_id,
                operation,
                args,
                ref arg_types,
                return_type,
            } => self.handle_create_await_future(
                thread_id,
                thread,
                continuation,
                module_id,
                operation,
                args,
                arg_types,
                return_type,
            ),
            galfus_vm::VmEffect::InternalThreadCall {
                module_id,
                operation,
                args,
                ref arg_types,
                return_type,
            } => self.handle_internal_thread_call(
                thread_id,
                thread,
                continuation,
                module_id,
                operation,
                args,
                arg_types,
                return_type,
            ),
            galfus_vm::VmEffect::CreateIndirectFuture {
                module_id,
                func,
                args,
                arg_types,
                return_type,
            } => self.handle_create_indirect_future(
                thread_id,
                thread,
                continuation,
                module_id,
                func,
                args,
                arg_types,
                return_type,
            ),
            galfus_vm::VmEffect::FutureWaitAll {
                future_ids,
                module_id,
                return_type,
            } => self.begin_aggregate_wait(
                thread_id,
                thread,
                continuation,
                module_id,
                return_type,
                future_ids,
                crate::orchestrator::AggregateMode::All,
            ),
            galfus_vm::VmEffect::FutureWaitRace {
                future_ids,
                module_id,
                return_type,
            } => self.begin_aggregate_wait(
                thread_id,
                thread,
                continuation,
                module_id,
                return_type,
                future_ids,
                crate::orchestrator::AggregateMode::Race,
            ),
        }
    }
}
