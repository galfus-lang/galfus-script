#[cfg(test)]
mod tests;

use super::{ModuleLoadWaiter, Orchestrator};

use crate::module_resolver::ModuleLoadRequest;
use crate::task::{ModuleLoadTask, QuotaTask};
use galfus_bytecode::ModuleResolveError;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind, KernelTask};
use galfus_core::ModuleId;
use std::sync::Arc;

impl Orchestrator {
    pub(super) fn handle_module_load(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        module_id: ModuleId,
    ) {
        let Some(resolver) = self.module_resolver.clone() else {
            self.fail_module_load_waiters(
                module_id,
                ModuleResolveError::UnknownModule {
                    context: galfus_bytecode::ModuleResolveContext::new(module_id),
                },
                vec![ModuleLoadWaiter {
                    thread_id,
                    thread,
                    continuation,
                }],
            );
            return;
        };
        let waiter = ModuleLoadWaiter {
            thread_id,
            thread,
            continuation,
        };
        match resolver.request_module_load(module_id) {
            Ok(ModuleLoadRequest::Ready(_)) => {
                self.refresh_vm_modules(&resolver);
                self.resume_module_waiter(waiter);
            }
            Ok(ModuleLoadRequest::Failed(error)) | Err(error) => {
                self.fail_module_load_waiters(module_id, error, vec![waiter]);
            }
            Ok(ModuleLoadRequest::Loading) => {
                self.module_load_waiters
                    .entry(module_id)
                    .or_default()
                    .push(waiter);
            }
            Ok(ModuleLoadRequest::Start) => {
                self.module_load_waiters
                    .entry(module_id)
                    .or_default()
                    .push(waiter);
                self.dispatch_module_load(resolver, module_id);
            }
        }
    }

    pub(super) fn complete_module_load(
        &mut self,
        module_id: ModuleId,
        result: Result<Arc<galfus_bytecode::BytecodeNode>, ModuleResolveError>,
    ) {
        let waiters = self
            .module_load_waiters
            .remove(&module_id)
            .unwrap_or_default();
        match result {
            Ok(_) => {
                if let Some(resolver) = self.module_resolver.clone() {
                    self.refresh_vm_modules(&resolver);
                    for waiter in waiters {
                        self.resume_module_waiter(waiter);
                    }
                } else {
                    self.fail_module_load_waiters(
                        module_id,
                        ModuleResolveError::UnknownModule {
                            context: galfus_bytecode::ModuleResolveContext::new(module_id),
                        },
                        waiters,
                    );
                }
            }
            Err(error) => self.fail_module_load_waiters(module_id, error, waiters),
        }
    }

    fn dispatch_module_load(
        &mut self,
        resolver: Arc<crate::module_resolver::ModuleResolver>,
        module_id: ModuleId,
    ) {
        let reservation = self.quota.lock().unwrap().try_reserve_kernel_tasks(1);
        if let Err(error) = reservation {
            let waiters = self
                .module_load_waiters
                .remove(&module_id)
                .unwrap_or_default();
            let failure = ExecutionFailure::new(error, "kernel tasks limit exceeded")
                .with_module_id(module_id.raw().into());
            self.failure = Some(waiters.first().map_or(failure.clone(), |waiter| {
                failure.with_thread_id(waiter.thread_id)
            }));
            for waiter in waiters {
                self.cancel_and_teardown_thread(waiter.thread_id);
            }
            return;
        }
        let task = ModuleLoadTask::new(
            resolver,
            module_id,
            self.event_sink
                .as_ref()
                .expect("event sink is configured before module loading")
                .clone(),
        );
        self.driver
            .as_ref()
            .expect("driver is configured before module loading")
            .dispatch(KernelTask::Any(Box::new(QuotaTask::new(
                task,
                self.quota.clone(),
            ))));
    }

    fn refresh_vm_modules(&mut self, resolver: &crate::module_resolver::ModuleResolver) {
        let current_vm = self
            .vm
            .as_ref()
            .expect("VM is configured before module loading");
        self.vm = Some(Arc::new(
            current_vm.with_ready_modules(resolver.loaded_modules()),
        ));
    }

    fn resume_module_waiter(&mut self, waiter: ModuleLoadWaiter) {
        self.resume_or_fail_front(
            waiter.thread_id,
            waiter.thread,
            waiter.continuation,
            galfus_vm::VmValue::Null,
        );
    }

    fn fail_module_load_waiters(
        &mut self,
        module_id: ModuleId,
        error: ModuleResolveError,
        waiters: Vec<ModuleLoadWaiter>,
    ) {
        let failure = ExecutionFailure::new(
            ExecutionFailureKind::InternalRuntimeFailure,
            error.to_string(),
        )
        .with_module_id(module_id.raw().into());
        if let Some(waiter) = waiters.first() {
            self.failure = Some(failure.clone().with_thread_id(waiter.thread_id));
        } else {
            self.failure = Some(failure);
        }
        for waiter in waiters {
            self.cancel_and_teardown_thread(waiter.thread_id);
        }
    }
}
