use super::Orchestrator;
use crate::event::RuntimeEvent;
use crate::execution::FutureCompletionInjector;
use crate::orchestrator::external::{AdapterDispatchTask, ProviderDispatchTask};
use crate::orchestrator::pending::{PendingContinuation, PendingKey, PendingOperation};
use crate::task::execution_stack;
use galfus_bytecode::instruction::{FuncIdx, TypeIdx};
use galfus_contract::{BoundaryValue, ExecutionFailure, ExecutionFailureKind, KernelTask};
use galfus_core::ModuleId;
use std::sync::{Arc, atomic::AtomicBool};

impl Orchestrator {
    pub(super) fn handle_effect(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        effect: galfus_vm::VmEffect,
        continuation: galfus_vm::Continuation,
    ) {
        if matches!(
            &effect,
            galfus_vm::VmEffect::ProviderCall { .. }
                | galfus_vm::VmEffect::SendMsg { .. }
                | galfus_vm::VmEffect::ReceiveFilter { .. }
                | galfus_vm::VmEffect::CreateThread { .. }
                | galfus_vm::VmEffect::StartThread { .. }
                | galfus_vm::VmEffect::GetThread { .. }
                | galfus_vm::VmEffect::ThreadIsRunning { .. }
                | galfus_vm::VmEffect::ThreadIsExited { .. }
                | galfus_vm::VmEffect::ThreadExitReason { .. }
                | galfus_vm::VmEffect::MailboxHasMessages
                | galfus_vm::VmEffect::MailboxGetMessage
                | galfus_vm::VmEffect::WaitThread { .. }
        ) {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::InvalidBytecode,
                    "legacy immediate boundary effect; use an async Future activation",
                )
                .with_thread_id(thread_id.raw())
                .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return;
        }

        match effect {
            galfus_vm::VmEffect::ProviderCall {
                module_id,
                name,
                args,
                arg_types,
                return_type,
            } => {
                let stack = execution_stack(&thread);
                let vm = self.vm.as_ref().expect("VM is configured before execution");
                let module = &vm
                    .graph
                    .get(module_id)
                    .expect("provider call module is loaded")
                    .module;
                let args = args
                    .into_iter()
                    .zip(arg_types)
                    .map(|(value, ty)| {
                        crate::task::decode_from_thread_heap(&thread.heap, value, ty, module)
                    })
                    .collect::<Result<Vec<_>, _>>();
                let args = match args {
                    Ok(args) => args,
                    Err(error) => {
                        self.failure = Some(
                            galfus_contract::ExecutionFailure::new(
                                galfus_contract::ExecutionFailureKind::BoundaryCodecFailure,
                                format!("invalid provider argument: {error:?}"),
                            )
                            .with_thread_id(thread_id.raw())
                            .with_module_id(module_id.raw().into())
                            .with_stack(stack.clone()),
                        );
                        self.kernel.cancel(thread_id);
                        return;
                    }
                };
                let Some(providers) = vm.providers() else {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::MissingProvider,
                            "HostProvider missing",
                        )
                        .with_thread_id(thread_id.raw())
                        .with_module_id(module_id.raw().into())
                        .with_stack(stack.clone()),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                };
                let affinity = {
                    let mut providers = providers.lock().unwrap();
                    let Some(host) = providers.host_mut() else {
                        self.failure = Some(
                            ExecutionFailure::new(
                                ExecutionFailureKind::MissingProvider,
                                "HostProvider missing",
                            )
                            .with_thread_id(thread_id.raw())
                            .with_module_id(module_id.raw().into())
                            .with_stack(stack.clone()),
                        );
                        self.kernel.cancel(thread_id);
                        return;
                    };
                    host.affinity(name.as_str())
                };
                let request_id = self.next_request_id;
                self.next_request_id += 1;
                let active = Arc::new(AtomicBool::new(true));
                self.pending_continuations.insert(
                    PendingKey::Request(request_id),
                    PendingContinuation {
                        thread_id,
                        continuation,
                        module_id,
                        return_type,
                        request_id,
                        stack,
                        operation: PendingOperation::Provider,
                        active: active.clone(),
                    },
                );
                let injector = Arc::new(crate::ExecutionHandle::new(self.sink.clone()));
                self.kernel.block(thread_id, thread, None);
                let task = ProviderDispatchTask {
                    providers,
                    thread_id: thread_id.raw() as usize,
                    request_id,
                    name,
                    args,
                    injector,
                    active,
                };
                self.driver
                    .as_ref()
                    .expect("driver is configured before execution")
                    .dispatch(task.into_kernel_task(affinity));
                return;
            }
            galfus_vm::VmEffect::SendMsg { target, bytes } => {
                if target == 0 {
                    self.sink.send(RuntimeEvent::Failed {
                        thread_id,
                        error: galfus_contract::ExecutionFailure::new(
                            galfus_contract::ExecutionFailureKind::InvalidBytecode,
                            "host calls must use the ProviderCall VM effect",
                        )
                        .with_thread_id(thread_id.raw())
                        .with_stack(execution_stack(&thread)),
                    });
                    return;
                }
                let target_id = crate::registry::ThreadId::from_raw(target);
                let mut success = false;
                if let Some(target_id) = target_id {
                    if let Some(mailbox) = self.kernel.get_mailbox(target_id) {
                        mailbox
                            .lock()
                            .unwrap()
                            .push_back(crate::registry::MailboxMessage {
                                sender_id: thread_id.raw(),
                                data: bytes,
                            });
                        self.kernel.unblock(target_id);
                        success = true;
                    }
                }
                self.resume_or_fail(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Bool(success),
                );
            }
            galfus_vm::VmEffect::AdapterCall {
                module_id,
                adapter,
                symbol,
                args,
                arg_types,
                return_type,
            } => {
                let stack = execution_stack(&thread);
                let Some(bindings) = self.external_bindings.clone() else {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::MissingAdapter,
                            "adapter registry missing",
                        )
                        .with_thread_id(thread_id.raw())
                        .with_stack(stack.clone()),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                };
                let module = &self
                    .vm
                    .as_ref()
                    .unwrap()
                    .graph
                    .get(module_id)
                    .unwrap()
                    .module;
                let args = args
                    .into_iter()
                    .zip(arg_types)
                    .map(|(value, ty)| {
                        crate::task::decode_from_thread_heap(&thread.heap, value, ty, module)
                    })
                    .collect::<Result<Vec<_>, _>>();
                let Ok(args) = args else {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::BoundaryCodecFailure,
                            "invalid adapter argument",
                        )
                        .with_thread_id(thread_id.raw())
                        .with_stack(stack.clone()),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                };
                if bindings.lock().unwrap().get_mut(&adapter).is_none() {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::MissingAdapter,
                            "adapter symbol missing",
                        )
                        .with_thread_id(thread_id.raw())
                        .with_stack(stack.clone()),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                }
                let request_id = self.next_request_id;
                self.next_request_id += 1;
                let active = Arc::new(AtomicBool::new(true));
                self.pending_continuations.insert(
                    PendingKey::Request(request_id),
                    PendingContinuation {
                        thread_id,
                        continuation,
                        module_id,
                        return_type,
                        request_id,
                        stack,
                        operation: PendingOperation::Adapter {
                            module: adapter.clone(),
                            symbol: symbol.clone(),
                        },
                        active: active.clone(),
                    },
                );
                self.kernel.block(thread_id, thread, None);
                let task = AdapterDispatchTask {
                    bindings,
                    thread_id: thread_id.raw() as usize,
                    request_id,
                    module: adapter,
                    symbol,
                    args,
                    injector: Arc::new(crate::ExecutionHandle::new(self.sink.clone())),
                    active,
                };
                self.driver
                    .as_ref()
                    .unwrap()
                    .dispatch(KernelTask::Main(Box::new(task)));
                return;
            }
            galfus_vm::VmEffect::TimerWait { delay_ms } => {
                self.kernel.block(thread_id, thread, Some(delay_ms));
            }
            galfus_vm::VmEffect::FutureDropped { future_id } => {
                self.remove_mailbox_future_wait(thread_id, future_id);
                let disposition = match self.future_registry.discard(thread_id, future_id) {
                    Ok(disposition) => disposition,
                    Err(error) => {
                        self.failure = Some(error.with_stack(execution_stack(&thread)));
                        self.kernel.cancel(thread_id);
                        return;
                    }
                };
                if let crate::orchestrator::future_registry::DiscardDisposition::Running(
                    activation,
                ) = disposition
                {
                    self.cancel_future_activation(thread_id, future_id, activation);
                }
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Null,
                );
            }
            galfus_vm::VmEffect::FutureWait {
                future_id,
                module_id,
                return_type,
            } => {
                let aggregate_registration = self.aggregate_registration.take();
                let waiter = crate::orchestrator::future_registry::Waiter {
                    continuation: PendingContinuation {
                        thread_id,
                        continuation,
                        module_id,
                        return_type,
                        request_id: future_id,
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

                if let crate::orchestrator::future_registry::WaitDisposition::Resolved {
                    waiter,
                    result,
                } = disposition
                {
                    if aggregate_registration.is_none() {
                        self.kernel.block(thread_id, thread, None);
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
                        .with_thread_id(thread_id.raw())
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
                    match activation {
                        crate::orchestrator::future_registry::Activation::GalfusFunction {
                            module_id: act_module_id,
                            func_idx,
                            args,
                            arg_types,
                        } => {
                            let mut worker_thread = galfus_vm::thread::VmThreadState::new();
                            let module = &self
                                .vm
                                .as_ref()
                                .unwrap()
                                .graph
                                .get(act_module_id)
                                .unwrap()
                                .module;

                            let mut vm_args = Vec::with_capacity(args.len());
                            for (boundary, expected_ty) in
                                args.into_iter().zip(arg_types.into_iter())
                            {
                                let vm_val = match crate::task::encode_into_thread_heap(
                                    &mut worker_thread.heap,
                                    boundary,
                                    expected_ty,
                                    act_module_id,
                                    module,
                                ) {
                                    Ok(value) => value,
                                    Err(error) => {
                                        self.failure = Some(
                                            ExecutionFailure::new(
                                                ExecutionFailureKind::BoundaryCodecFailure,
                                                format!(
                                                    "invalid future worker argument: {error:?}"
                                                ),
                                            )
                                            .with_thread_id(thread_id.raw())
                                            .with_module_id(act_module_id.raw().into())
                                            .with_stack(execution_stack(&thread)),
                                        );
                                        self.kernel.cancel(thread_id);
                                        return;
                                    }
                                };
                                vm_args.push(vm_val);
                            }
                            if let Err(error) = self
                                .vm
                                .as_ref()
                                .expect("VM is configured before execution")
                                .prepare_function(
                                    &mut worker_thread,
                                    act_module_id,
                                    func_idx,
                                    vm_args,
                                )
                            {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::VmPanic,
                                        error.to_string(),
                                    )
                                    .with_thread_id(thread_id.raw())
                                    .with_module_id(act_module_id.raw().into())
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            }

                            let worker_id = self.kernel.spawn(worker_thread, None);
                            self.future_workers
                                .insert(worker_id, (thread_id, future_id));
                            let spawned_thread = self.kernel.take_thread(worker_id).unwrap();
                            self.kernel.enqueue_runnable(worker_id, spawned_thread);
                        }
                        crate::orchestrator::future_registry::Activation::Provider {
                            name,
                            args,
                            arg_types: _,
                        } => {
                            let vm = self.vm.as_ref().expect("VM is configured before execution");
                            let Some(providers) = vm.providers() else {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::MissingProvider,
                                        "HostProvider missing",
                                    )
                                    .with_thread_id(thread_id.raw())
                                    .with_future_id(future_id)
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            };
                            let affinity = {
                                let mut providers = providers.lock().unwrap();
                                let Some(host) = providers.host_mut() else {
                                    self.failure = Some(
                                        ExecutionFailure::new(
                                            ExecutionFailureKind::MissingProvider,
                                            "HostProvider missing",
                                        )
                                        .with_thread_id(thread_id.raw())
                                        .with_future_id(future_id)
                                        .with_stack(execution_stack(&thread)),
                                    );
                                    self.kernel.cancel(thread_id);
                                    return;
                                };
                                host.affinity(name.as_str())
                            };
                            let task = ProviderDispatchTask {
                                providers,
                                thread_id: thread_id.raw() as usize,
                                request_id: future_id,
                                name,
                                args,
                                injector: Arc::new(FutureCompletionInjector::new(
                                    self.sink.clone(),
                                    thread_id,
                                    future_id,
                                )),
                                active: self
                                    .future_registry
                                    .active_flag(thread_id, future_id)
                                    .expect("active future has a registry record"),
                            };
                            self.driver
                                .as_ref()
                                .expect("driver is configured before execution")
                                .dispatch(task.into_kernel_task(affinity));
                        }
                        crate::orchestrator::future_registry::Activation::Adapter {
                            proxy_module,
                            symbol,
                            args,
                            arg_types: _,
                        } => {
                            let Some(bindings) = self.external_bindings.clone() else {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::MissingAdapter,
                                        "adapter registry missing",
                                    )
                                    .with_thread_id(thread_id.raw())
                                    .with_future_id(future_id)
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            };
                            if bindings.lock().unwrap().get_mut(&proxy_module).is_none() {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::MissingAdapter,
                                        "adapter symbol missing",
                                    )
                                    .with_thread_id(thread_id.raw())
                                    .with_future_id(future_id)
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            }
                            let task = AdapterDispatchTask {
                                bindings,
                                thread_id: thread_id.raw() as usize,
                                request_id: future_id,
                                module: proxy_module,
                                symbol,
                                args,
                                injector: Arc::new(FutureCompletionInjector::new(
                                    self.sink.clone(),
                                    thread_id,
                                    future_id,
                                )),
                                active: self
                                    .future_registry
                                    .active_flag(thread_id, future_id)
                                    .expect("active future has a registry record"),
                            };
                            self.driver
                                .as_ref()
                                .expect("driver is configured before execution")
                                .dispatch(KernelTask::Main(Box::new(task)));
                        }
                        crate::orchestrator::future_registry::Activation::Internal {
                            operation,
                            args,
                            arg_types: _,
                        } => {
                            let thread_arg = |index: usize| {
                                args.get(index).and_then(|value| match value {
                                    BoundaryValue::I64(id) if *id > 0 => {
                                        crate::registry::ThreadId::from_raw(*id as u64)
                                    }
                                    _ => None,
                                })
                            };
                            let immediate = match operation.as_str() {
                                "__internal_thread_get" => {
                                    let id = args
                                        .first()
                                        .and_then(|value| match value {
                                            BoundaryValue::Bytes(key) => {
                                                std::str::from_utf8(key).ok()
                                            }
                                            _ => None,
                                        })
                                        .and_then(|key| self.kernel.lookup_key(key))
                                        .map(|id| id.raw() as i64)
                                        .unwrap_or(-1);
                                    Some(Ok(BoundaryValue::I64(id)))
                                }
                                "__internal_thread_is_running" => Some(Ok(BoundaryValue::Bool(
                                    thread_arg(0)
                                        .and_then(|id| self.kernel.state(id))
                                        .is_some_and(|state| state.is_running()),
                                ))),
                                "__internal_thread_is_exited" => Some(Ok(BoundaryValue::Bool(
                                    thread_arg(0)
                                        .and_then(|id| self.kernel.state(id))
                                        .is_some_and(|state| state.is_exited()),
                                ))),
                                "__internal_thread_exit_reason" => Some(Ok(thread_arg(0)
                                    .and_then(|id| self.kernel.state(id))
                                    .and_then(|state| state.exit_reason())
                                    .and_then(|result| match result {
                                        Ok(BoundaryValue::I32(code)) => {
                                            Some(BoundaryValue::I32(code))
                                        }
                                        _ => None,
                                    })
                                    .unwrap_or(BoundaryValue::Null))),
                                "__internal_thread_send" => {
                                    let success = match (thread_arg(0), args.get(1)) {
                                        (Some(target_id), Some(BoundaryValue::Bytes(data))) => {
                                            if let Some(mailbox) =
                                                self.kernel.get_mailbox(target_id)
                                            {
                                                mailbox.lock().unwrap().push_back(
                                                    crate::registry::MailboxMessage {
                                                        sender_id: thread_id.raw(),
                                                        data: data.clone(),
                                                    },
                                                );
                                                self.kernel.unblock(target_id);
                                                self.complete_mailbox_future_waits(target_id);
                                                true
                                            } else {
                                                false
                                            }
                                        }
                                        _ => false,
                                    };
                                    Some(Ok(BoundaryValue::Bool(success)))
                                }
                                "__internal_thread_has_messages" => Some(Ok(BoundaryValue::Bool(
                                    self.kernel
                                        .get_mailbox(thread_id)
                                        .is_some_and(|mailbox| !mailbox.lock().unwrap().is_empty()),
                                ))),
                                "__internal_thread_get_message" => Some(Ok(self
                                    .kernel
                                    .get_mailbox(thread_id)
                                    .and_then(|mailbox| mailbox.lock().unwrap().pop_front())
                                    .map(|message| BoundaryValue::Bytes(message.data))
                                    .unwrap_or(BoundaryValue::Null))),
                                "__internal_thread_wait" => {
                                    match thread_arg(0).and_then(|id| self.kernel.state(id)) {
                                        Some(state) if !state.is_exited() => {
                                            if let Some(target_id) = thread_arg(0) {
                                                self.register_thread_exit_future(
                                                    target_id, thread_id, future_id,
                                                );
                                            }
                                            None
                                        }
                                        Some(state) => Some(Ok(state
                                            .exit_reason()
                                            .and_then(Result::ok)
                                            .unwrap_or(BoundaryValue::Null))),
                                        None => Some(Ok(BoundaryValue::Null)),
                                    }
                                }
                                "__internal_thread_receive" => {
                                    let sender_id = thread_arg(0).map(|id| id.raw());
                                    let timeout_ms = args.get(1).and_then(|value| match value {
                                        BoundaryValue::I32(ms) if *ms >= 0 => Some(*ms as u64),
                                        BoundaryValue::I64(ms) if *ms >= 0 => Some(*ms as u64),
                                        _ => None,
                                    });
                                    let message =
                                        self.kernel.get_mailbox(thread_id).and_then(|mailbox| {
                                            let mut mailbox = mailbox.lock().unwrap();
                                            let index = sender_id.map_or_else(
                                                || (!mailbox.is_empty()).then_some(0),
                                                |sender_id| {
                                                    mailbox.iter().position(|message| {
                                                        message.sender_id == sender_id
                                                    })
                                                },
                                            )?;
                                            mailbox.remove(index)
                                        });
                                    match message {
                                        Some(message) => {
                                            Some(Ok(BoundaryValue::Bytes(message.data)))
                                        }
                                        None => {
                                            self.register_mailbox_future_wait(
                                                thread_id, thread_id, future_id, sender_id,
                                                timeout_ms,
                                            );
                                            None
                                        }
                                    }
                                }
                                "__internal_thread_create" => {
                                    let key = args.get(1).and_then(|value| match value {
                                        BoundaryValue::Bytes(key) => {
                                            String::from_utf8(key.clone()).ok()
                                        }
                                        BoundaryValue::Null => None,
                                        _ => None,
                                    });
                                    let id = match args.first() {
                                        Some(BoundaryValue::Function {
                                            module_id,
                                            func_idx,
                                        }) => {
                                            let mut new_thread =
                                                galfus_vm::thread::VmThreadState::new();
                                            new_thread.entry_func =
                                                Some(galfus_vm::VmValue::Function {
                                                    module_id: ModuleId::new(*module_id),
                                                    func_idx: FuncIdx(*func_idx),
                                                });
                                            self.kernel.spawn(new_thread, key).raw() as i64
                                        }
                                        _ => -1,
                                    };
                                    Some(Ok(BoundaryValue::I64(id)))
                                }
                                "__internal_thread_spawn" => {
                                    let success = thread_arg(0).is_some_and(|target_id| {
                                        let Some(mut target_thread) =
                                            self.kernel.take_created_thread(target_id)
                                        else {
                                            return false;
                                        };
                                        let prepared = match target_thread.entry_func.clone() {
                                            Some(galfus_vm::VmValue::Function {
                                                module_id,
                                                func_idx,
                                            }) => {
                                                let module = &self
                                                    .vm
                                                    .as_ref()
                                                    .expect("VM is configured before execution")
                                                    .graph
                                                    .get(module_id)
                                                    .expect("thread entry module is loaded")
                                                    .module;
                                                let argument = match args.get(1) {
                                                    Some(BoundaryValue::Null) | None => {
                                                        Ok(galfus_vm::VmValue::Null)
                                                    }
                                                    Some(BoundaryValue::Array { values, .. }) => {
                                                        let byte_type = module
                                                            .types
                                                            .iter()
                                                            .position(|ty| {
                                                                matches!(
                                                                    ty,
                                                                    galfus_bytecode::BytecodeType::Uint8
                                                                )
                                                            })
                                                            .map(|index| TypeIdx(index as u16));
                                                        let bytes_type = byte_type.and_then(|byte_type| {
                                                            module
                                                                .types
                                                                .iter()
                                                                .position(|ty| {
                                                                    matches!(
                                                                        ty,
                                                                        galfus_bytecode::BytecodeType::Array(element)
                                                                            if *element == byte_type
                                                                    )
                                                                })
                                                                .map(|index| TypeIdx(index as u16))
                                                        });
                                                        byte_type.zip(bytes_type).ok_or(()).and_then(
                                                            |(byte_type, bytes_type)| {
                                                                let arrays = values
                                                                    .iter()
                                                                    .map(|value| match value {
                                                                        BoundaryValue::Bytes(bytes) => Ok(
                                                                            galfus_vm::VmValue::Object(
                                                                                target_thread.heap.alloc(
                                                                                    galfus_vm::HeapObject::Array {
                                                                                        element_ty: byte_type,
                                                                                        elements: bytes
                                                                                            .iter()
                                                                                            .copied()
                                                                                            .map(galfus_vm::VmValue::Uint8)
                                                                                            .collect(),
                                                                                    },
                                                                                ),
                                                                            ),
                                                                        ),
                                                                        _ => Err(()),
                                                                    })
                                                                    .collect::<Result<Vec<_>, _>>()?;
                                                                Ok(galfus_vm::VmValue::Object(
                                                                    target_thread.heap.alloc(
                                                                        galfus_vm::HeapObject::Array {
                                                                            element_ty: bytes_type,
                                                                            elements: arrays,
                                                                        },
                                                                    ),
                                                                ))
                                                            },
                                                        )
                                                    }
                                                    _ => Err(()),
                                                };
                                                argument.map_err(|_| ()).and_then(|argument| {
                                                    self.vm
                                                        .as_ref()
                                                        .expect("VM is configured before execution")
                                                        .prepare_function(
                                                            &mut target_thread,
                                                            module_id,
                                                            func_idx,
                                                            vec![argument],
                                                        )
                                                        .map_err(|_| ())
                                                })
                                            }
                                            _ => Err(()),
                                        };
                                        if prepared.is_ok() {
                                            self.kernel.enqueue_runnable(target_id, target_thread);
                                            true
                                        } else {
                                            self.kernel.park_running(target_id, target_thread);
                                            false
                                        }
                                    });
                                    Some(Ok(BoundaryValue::Bool(success)))
                                }
                                _ => {
                                    self.failure = Some(
                                        ExecutionFailure::new(
                                            ExecutionFailureKind::InvalidBytecode,
                                            format!(
                                                "unknown internal future activation: {operation}"
                                            ),
                                        )
                                        .with_thread_id(thread_id.raw())
                                        .with_future_id(future_id)
                                        .with_stack(execution_stack(&thread)),
                                    );
                                    self.kernel.cancel(thread_id);
                                    return;
                                }
                            };
                            if let Some(result) = immediate {
                                if aggregate_registration.is_none() {
                                    self.kernel.block(thread_id, thread, None);
                                }
                                self.complete_future(thread_id, future_id, result);
                                return;
                            }
                        }
                    }
                }

                if aggregate_registration.is_none() {
                    self.kernel.block(thread_id, thread, None);
                }
            }
            galfus_vm::VmEffect::ReceiveFilter {
                sender_id: _,
                timeout,
            } => {
                if let Some(ms) = timeout {
                    self.kernel.block(thread_id, thread, Some(ms));
                } else {
                    self.kernel.block(thread_id, thread, None);
                }
            }
            galfus_vm::VmEffect::CreateThread { func, key } => {
                let mut new_thread = galfus_vm::thread::VmThreadState::new();
                new_thread.entry_func = Some(func.clone());
                let new_id = self
                    .kernel
                    .spawn(new_thread, crate::task::thread_key(&thread, key));
                self.resolve_intrinsic_future(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Int64(new_id.raw() as i64),
                );
            }
            galfus_vm::VmEffect::StartThread {
                thread_id: target_id,
                arg,
            } => {
                let target_id = crate::registry::ThreadId::from_raw(target_id);
                let mut success = false;

                if let Some(target_id) = target_id {
                    let target_thread = self.kernel.take_created_thread(target_id);
                    if let Some(mut target_thread) = target_thread {
                        let prepared = match target_thread.entry_func.clone() {
                            Some(galfus_vm::VmValue::Function {
                                module_id,
                                func_idx,
                            }) => self.vm.as_ref().unwrap().prepare_function(
                                &mut target_thread,
                                module_id,
                                func_idx,
                                vec![arg],
                            ),
                            _ => Err(galfus_vm::VmPanic {
                                error: galfus_vm::VmError::FunctionOutOfBounds {
                                    index: galfus_bytecode::instruction::FuncIdx(0),
                                },
                                stack_trace: vec![],
                            }),
                        };
                        if prepared.is_ok() {
                            self.kernel.enqueue_runnable(target_id, target_thread);
                            success = true;
                        } else {
                            self.kernel.park_running(target_id, target_thread);
                        }
                    }
                }

                self.resolve_intrinsic_future(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Bool(success),
                );
            }
            galfus_vm::VmEffect::GetThread { key } => {
                let val = crate::task::thread_key(&thread, key)
                    .and_then(|k| self.kernel.lookup_key(k.as_str()))
                    .map(|id| galfus_vm::VmValue::Int64(id.raw() as i64))
                    .unwrap_or(galfus_vm::VmValue::Int64(-1));
                self.resolve_intrinsic_future(thread_id, thread, continuation, val);
            }
            galfus_vm::VmEffect::ThreadIsRunning {
                thread_id: target_id,
            } => {
                let running = crate::registry::ThreadId::from_raw(target_id)
                    .and_then(|id| self.kernel.state(id))
                    .is_some_and(|state| state.is_running());
                self.resolve_intrinsic_future(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Bool(running),
                );
            }
            galfus_vm::VmEffect::ThreadIsExited {
                thread_id: target_id,
            } => {
                let exited = crate::registry::ThreadId::from_raw(target_id)
                    .and_then(|id| self.kernel.state(id))
                    .is_some_and(|state| state.is_exited());
                self.resolve_intrinsic_future(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Bool(exited),
                );
            }
            galfus_vm::VmEffect::ThreadExitReason {
                thread_id: target_id,
            } => {
                let reason = crate::registry::ThreadId::from_raw(target_id)
                    .and_then(|id| self.kernel.state(id))
                    .and_then(|state| state.exit_reason())
                    .and_then(|result| match result {
                        Ok(galfus_contract::BoundaryValue::I32(code)) => {
                            Some(galfus_vm::VmValue::Int32(code))
                        }
                        _ => None,
                    })
                    .unwrap_or(galfus_vm::VmValue::Null);
                self.resolve_intrinsic_future(thread_id, thread, continuation, reason);
            }
            galfus_vm::VmEffect::Blocked => {
                self.kernel.enqueue_runnable(thread_id, thread);
            }
            galfus_vm::VmEffect::MailboxHasMessages => {
                let has_messages = self
                    .kernel
                    .get_mailbox(thread_id)
                    .is_some_and(|mailbox| !mailbox.lock().unwrap().is_empty());
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Bool(has_messages),
                );
            }
            galfus_vm::VmEffect::MailboxGetMessage => {
                let message = self
                    .kernel
                    .get_mailbox(thread_id)
                    .and_then(|mailbox| mailbox.lock().unwrap().pop_front());
                let value = match message {
                    Some(msg) => {
                        let elements = msg
                            .data
                            .into_iter()
                            .map(galfus_vm::VmValue::Uint8)
                            .collect();
                        let array = thread.heap.alloc(galfus_vm::HeapObject::Array {
                            element_ty: galfus_bytecode::instruction::TypeIdx(0),
                            elements,
                        });
                        let tuple = thread.heap.alloc(galfus_vm::HeapObject::Tuple {
                            elements: vec![
                                galfus_vm::VmValue::Int64(msg.sender_id as i64),
                                galfus_vm::VmValue::Object(array),
                            ],
                        });
                        galfus_vm::VmValue::Object(tuple)
                    }
                    None => galfus_vm::VmValue::Null,
                };
                self.resume_or_fail_front(thread_id, thread, continuation, value);
            }
            galfus_vm::VmEffect::WaitThread {
                thread_id: target_raw,
            } => {
                let future_id = self.next_request_id;
                self.next_request_id += 1;
                let maybe_target = crate::registry::ThreadId::from_raw(target_raw);
                let exit_code = maybe_target
                    .and_then(|target| self.kernel.state(target))
                    .and_then(|state| state.exit_reason());
                if let Some(result) = exit_code {
                    if let Err(error) = self
                        .future_registry
                        .insert_resolved(thread_id, future_id, result)
                    {
                        self.failure = Some(error.with_stack(execution_stack(&thread)));
                        self.kernel.cancel(thread_id);
                        return;
                    }
                } else {
                    match maybe_target {
                        Some(target_id) => {
                            if let Err(error) = self.future_registry.insert_created(
                                thread_id,
                                future_id,
                                None,
                                None,
                                crate::orchestrator::future_registry::Activation::Internal {
                                    operation: "thread-exit".to_string(),
                                    args: vec![],
                                    arg_types: vec![],
                                },
                            ) {
                                self.failure = Some(error.with_stack(execution_stack(&thread)));
                                self.kernel.cancel(thread_id);
                                return;
                            }
                            if let Err(error) = self
                                .future_registry
                                .take_activation_for_start(thread_id, future_id)
                            {
                                self.failure = Some(error.with_stack(execution_stack(&thread)));
                                self.kernel.cancel(thread_id);
                                return;
                            }
                            self.register_thread_exit_future(target_id, thread_id, future_id);
                        }
                        None => {
                            if let Err(error) = self.future_registry.insert_resolved(
                                thread_id,
                                future_id,
                                Ok(galfus_contract::BoundaryValue::Null),
                            ) {
                                self.failure = Some(error.with_stack(execution_stack(&thread)));
                                self.kernel.cancel(thread_id);
                                return;
                            }
                        }
                    }
                }
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Future(future_id),
                );
            }
            galfus_vm::VmEffect::CreateFuture {
                module_id,
                target_module_id,
                func_idx,
                args,
                arg_types,
                return_type,
            } => {
                let future_id = self.next_request_id;
                self.next_request_id += 1;

                let module = &self
                    .vm
                    .as_ref()
                    .unwrap()
                    .graph
                    .get(module_id)
                    .unwrap()
                    .module;
                let mut encoded_args = Vec::with_capacity(args.len());
                for (arg, ty) in args.into_iter().zip(arg_types.iter()) {
                    match crate::task::decode_from_thread_heap(
                        &thread.heap,
                        arg.clone(),
                        *ty,
                        module,
                    ) {
                        Ok(value) => encoded_args.push(value),
                        Err(_) if matches!(arg, galfus_vm::VmValue::Function { .. }) => {
                            let galfus_vm::VmValue::Function {
                                module_id,
                                func_idx,
                            } = arg
                            else {
                                unreachable!();
                            };
                            encoded_args.push(BoundaryValue::Function {
                                module_id: module_id.raw(),
                                func_idx: func_idx.raw(),
                            });
                            continue;
                        }
                        Err(error) => {
                            self.failure = Some(
                                ExecutionFailure::new(
                                    ExecutionFailureKind::BoundaryCodecFailure,
                                    format!("invalid future argument: {error:?}"),
                                )
                                .with_thread_id(thread_id.raw())
                                .with_module_id(module_id.raw().into())
                                .with_stack(execution_stack(&thread)),
                            );
                            self.kernel.cancel(thread_id);
                            return;
                        }
                    };
                }

                let activation =
                    self.future_activation(target_module_id, func_idx, encoded_args, arg_types);
                if let Err(error) = self.future_registry.insert_created(
                    thread_id,
                    future_id,
                    Some(return_type),
                    Some(module_id),
                    activation,
                ) {
                    self.failure = Some(error.with_stack(execution_stack(&thread)));
                    self.kernel.cancel(thread_id);
                    return;
                }

                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Future(future_id),
                );
            }
            galfus_vm::VmEffect::CreateIndirectFuture {
                module_id,
                func,
                args,
                arg_types,
                return_type,
            } => {
                let future_id = self.next_request_id;
                self.next_request_id += 1;
                let galfus_vm::VmValue::Function {
                    module_id: target_module_id,
                    func_idx,
                } = func
                else {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::InvalidContinuation,
                            "indirect async call requires a function value",
                        )
                        .with_thread_id(thread_id.raw())
                        .with_module_id(module_id.raw().into())
                        .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                };
                let target_module = &self
                    .vm
                    .as_ref()
                    .unwrap()
                    .graph
                    .get(target_module_id)
                    .unwrap()
                    .module;
                let mut encoded_args = Vec::with_capacity(args.len());
                for (arg, ty) in args.into_iter().zip(arg_types.iter()) {
                    match crate::task::decode_from_thread_heap(
                        &thread.heap,
                        arg,
                        *ty,
                        target_module,
                    ) {
                        Ok(value) => encoded_args.push(value),
                        Err(error) => {
                            self.failure = Some(
                                ExecutionFailure::new(
                                    ExecutionFailureKind::BoundaryCodecFailure,
                                    format!("invalid indirect future argument: {error:?}"),
                                )
                                .with_thread_id(thread_id.raw())
                                .with_module_id(module_id.raw().into())
                                .with_stack(execution_stack(&thread)),
                            );
                            self.kernel.cancel(thread_id);
                            return;
                        }
                    }
                }
                let activation =
                    self.future_activation(target_module_id, func_idx, encoded_args, arg_types);
                if let Err(error) = self.future_registry.insert_created(
                    thread_id,
                    future_id,
                    Some(return_type),
                    Some(module_id),
                    activation,
                ) {
                    self.failure = Some(error.with_stack(execution_stack(&thread)));
                    self.kernel.cancel(thread_id);
                    return;
                }
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Future(future_id),
                );
            }
            galfus_vm::VmEffect::FutureWaitAll {
                future_ids,
                module_id,
                return_type,
            } => {
                self.begin_aggregate_wait(
                    thread_id,
                    thread,
                    continuation,
                    module_id,
                    return_type,
                    future_ids,
                    crate::orchestrator::AggregateMode::All,
                );
            }
            galfus_vm::VmEffect::FutureWaitRace {
                future_ids,
                module_id,
                return_type,
            } => {
                self.begin_aggregate_wait(
                    thread_id,
                    thread,
                    continuation,
                    module_id,
                    return_type,
                    future_ids,
                    crate::orchestrator::AggregateMode::Race,
                );
            }
        }
    }

    fn begin_aggregate_wait(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        module_id: ModuleId,
        return_type: TypeIdx,
        future_ids: Vec<u64>,
        mode: crate::orchestrator::AggregateMode,
    ) {
        let coordinator_id = self.next_request_id;
        self.next_request_id += 1;
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
                    request_id: coordinator_id,
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
                    .with_thread_id(thread_id.raw())
                    .with_future_id(future_id)
                    .with_stack(execution_stack(&thread)),
                );
                self.aggregate_coordinators.remove(&coordinator_id);
                self.kernel.cancel(thread_id);
                return;
            };
            self.aggregate_registration = Some((coordinator_id, index));
            self.handle_effect(
                thread_id,
                galfus_vm::thread::VmThreadState::new(),
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
                self.kernel.cancel(thread_id);
                return;
            }
        }

        if let Some(coordinator) = self.aggregate_coordinators.get_mut(&coordinator_id) {
            coordinator.armed = true;
        }
        self.kernel.block(thread_id, thread, None);
        self.finish_aggregate_if_ready(coordinator_id);
    }

    fn future_activation(
        &self,
        target_module_id: ModuleId,
        func_idx: FuncIdx,
        args: Vec<BoundaryValue>,
        arg_types: Vec<TypeIdx>,
    ) -> crate::orchestrator::future_registry::Activation {
        let target = &self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .graph
            .get(target_module_id)
            .expect("future target module is loaded")
            .module;
        let function_name = target.functions[func_idx.raw() as usize].name.clone();
        let is_bodyless = target.functions[func_idx.raw() as usize]
            .instructions
            .is_empty();

        if let Some(name) = function_name.strip_prefix("__provider_") {
            crate::orchestrator::future_registry::Activation::Provider {
                name: name.to_string(),
                args,
                arg_types,
            }
        } else if function_name.starts_with("__internal_") {
            crate::orchestrator::future_registry::Activation::Internal {
                operation: function_name,
                args,
                arg_types,
            }
        } else if is_bodyless {
            crate::orchestrator::future_registry::Activation::Adapter {
                proxy_module: target.name.clone(),
                symbol: function_name,
                args,
                arg_types,
            }
        } else {
            crate::orchestrator::future_registry::Activation::GalfusFunction {
                module_id: target_module_id,
                func_idx,
                args,
                arg_types,
            }
        }
    }

    /// Resolves an intrinsic VM effect (e.g. CreateThread, StartThread) through a ready future.
    fn resolve_intrinsic_future(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        value: galfus_vm::VmValue,
    ) {
        let future_id = self.next_request_id;
        self.next_request_id += 1;
        let boundary = crate::task::vm_value_to_boundary(value);
        if let Err(error) = self
            .future_registry
            .insert_resolved(thread_id, future_id, Ok(boundary))
        {
            self.failure = Some(error);
            self.kernel.cancel(thread_id);
            return;
        }
        self.resume_or_fail_front(
            thread_id,
            thread,
            continuation,
            galfus_vm::VmValue::Future(future_id),
        );
    }
}
