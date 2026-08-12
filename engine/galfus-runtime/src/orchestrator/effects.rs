use super::Orchestrator;
use crate::execution::FutureCompletionInjector;
use crate::orchestrator::adapter::{AdapterDispatchTask, ProviderDispatchTask};
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
        thread: galfus_vm::thread::VmThreadState,
        effect: galfus_vm::VmEffect,
        continuation: galfus_vm::Continuation,
    ) {
        match effect {
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
                self.future_id_manager.free(future_id);
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Null,
                );
            }
            galfus_vm::VmEffect::AdapterHandleDropped {
                binding_id,
                type_id,
                id,
            } => {
                if let Err(error) = self.release_adapter_handle(binding_id, type_id, id) {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::AdapterCallFailure,
                            error.to_string(),
                        )
                        .with_thread_id(thread_id)
                        .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return;
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
                        if !self.block_or_fail(thread_id, thread) {
                            return;
                        }
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
                    match activation {
                        crate::orchestrator::future_registry::Activation::GalfusFunction {
                            module_id: act_module_id,
                            func_idx,
                            args,
                            arg_types,
                        } => {
                            let mut worker_thread = galfus_vm::thread::VmThreadState::new(self.quota.clone());
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
                                            .with_thread_id(thread_id)
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
                                    .with_thread_id(thread_id)
                                    .with_module_id(act_module_id.raw().into())
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            }

                            let worker_id = match self.kernel.spawn(worker_thread, None) {
                                Ok(worker_id) => worker_id,
                                Err(error) => {
                                    self.failure = Some(
                                        error
                                            .with_thread_id(thread_id)
                                            .with_future_id(future_id)
                                            .with_stack(execution_stack(&thread)),
                                    );
                                    self.kernel.cancel(thread_id);
                                    return;
                                }
                            };
                            self.future_workers.insert(
                                worker_id,
                                (
                                    thread_id,
                                    galfus_core::FutureLease::new(
                                        future_id,
                                        self.future_generations
                                            .get(&future_id.raw())
                                            .copied()
                                            .unwrap_or(0),
                                    ),
                                ),
                            );
                            let spawned_thread = self.kernel.take_thread(worker_id).unwrap();
                            self.kernel.enqueue_runnable(worker_id, spawned_thread);
                        }
                        crate::orchestrator::future_registry::Activation::Provider {
                            name,
                            args,
                            ..
                        } => {
                            let vm = self.vm.as_ref().expect("VM is configured before execution");
                            let Some(providers) = vm.providers() else {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::MissingProvider,
                                        "HostProvider missing",
                                    )
                                    .with_thread_id(thread_id)
                                    .with_future_id(future_id)
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            };
                            let affinity = {
                                let host = match providers.lock() {
                                    Ok(mut providers) => providers.take_host(),
                                    Err(_) => {
                                        self.failure = Some(
                                            ExecutionFailure::new(
                                                ExecutionFailureKind::InternalRuntimeFailure,
                                                "provider registry lock is poisoned",
                                            )
                                            .with_thread_id(thread_id)
                                            .with_future_id(future_id)
                                            .with_stack(execution_stack(&thread)),
                                        );
                                        self.kernel.cancel(thread_id);
                                        return;
                                    }
                                };
                                let Some(host) = host else {
                                    self.failure = Some(
                                        ExecutionFailure::new(
                                            ExecutionFailureKind::MissingProvider,
                                            "HostProvider missing",
                                        )
                                        .with_thread_id(thread_id)
                                        .with_future_id(future_id)
                                        .with_stack(execution_stack(&thread)),
                                    );
                                    self.kernel.cancel(thread_id);
                                    return;
                                };
                                let affinity = host.affinity(name.as_str());
                                if let Ok(mut providers) = providers.lock() {
                                    providers.restore_host(host);
                                }
                                affinity
                            };
                            let request_lease =
                                match self.allocate_request_lease(thread_id, future_id, &thread) {
                                    Some(lease) => lease,
                                    None => return,
                                };
                            if let Err(error) = self.future_registry.assign_request_id(
                                thread_id,
                                future_id,
                                request_lease.id,
                            ) {
                                self.failure = Some(error.with_stack(execution_stack(&thread)));
                                self.kernel.cancel(thread_id);
                                return;
                            }
                            let task = ProviderDispatchTask {
                                providers,
                                thread_id,
                                request_lease,
                                name,
                                args,
                                injector: Arc::new(FutureCompletionInjector::new(
                                    self.event_sink
                                        .as_ref()
                                        .expect("event sink is configured before execution")
                                        .clone(),
                                    thread_id,
                                    request_lease,
                                    galfus_core::FutureLease::new(
                                        future_id,
                                        self.future_generations
                                            .get(&future_id.raw())
                                            .copied()
                                            .unwrap_or(0),
                                    ),
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
                            ..
                        } => {
                            let Some(bindings) = self.adapter_bindings.clone() else {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::MissingAdapter,
                                        "adapter registry missing",
                                    )
                                    .with_thread_id(thread_id)
                                    .with_future_id(future_id)
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            };
                            let has_module = match bindings.lock() {
                                Ok(bindings) => bindings.has_module(&proxy_module),
                                Err(_) => {
                                    self.failure = Some(
                                        ExecutionFailure::new(
                                            ExecutionFailureKind::InternalRuntimeFailure,
                                            "adapter registry lock is poisoned",
                                        )
                                        .with_thread_id(thread_id)
                                        .with_future_id(future_id)
                                        .with_stack(execution_stack(&thread)),
                                    );
                                    self.kernel.cancel(thread_id);
                                    return;
                                }
                            };
                            if !has_module {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::MissingAdapter,
                                        "adapter symbol missing",
                                    )
                                    .with_thread_id(thread_id)
                                    .with_future_id(future_id)
                                    .with_stack(execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return;
                            }
                            let request_lease =
                                match self.allocate_request_lease(thread_id, future_id, &thread) {
                                    Some(lease) => lease,
                                    None => return,
                                };
                            if let Err(error) = self.future_registry.assign_request_id(
                                thread_id,
                                future_id,
                                request_lease.id,
                            ) {
                                self.failure = Some(error.with_stack(execution_stack(&thread)));
                                self.kernel.cancel(thread_id);
                                return;
                            }
                            let task = AdapterDispatchTask {
                                bindings,
                                thread_id,
                                request_lease,
                                module: proxy_module,
                                symbol,
                                args,
                                injector: Arc::new(FutureCompletionInjector::new(
                                    self.event_sink
                                        .as_ref()
                                        .expect("event sink is configured before execution")
                                        .clone(),
                                    thread_id,
                                    request_lease,
                                    galfus_core::FutureLease::new(
                                        future_id,
                                        self.future_generations
                                            .get(&future_id.raw())
                                            .copied()
                                            .unwrap_or(0),
                                    ),
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
                        } => {
                            let float_arg = |idx: usize| -> Option<f64> {
                                args.get(idx).and_then(|val| match val {
                                    BoundaryValue::F64(f) => Some(*f),
                                    BoundaryValue::F32(f) => Some(*f as f64),
                                    BoundaryValue::I64(i) => Some(*i as f64),
                                    BoundaryValue::I32(i) => Some(*i as f64),
                                    BoundaryValue::I16(i) => Some(*i as f64),
                                    BoundaryValue::I8(i) => Some(*i as f64),
                                    BoundaryValue::U64(i) => Some(*i as f64),
                                    BoundaryValue::U32(i) => Some(*i as f64),
                                    BoundaryValue::U16(i) => Some(*i as f64),
                                    BoundaryValue::U8(i) => Some(*i as f64),
                                    _ => None,
                                })
                            };
                            let thread_arg = |index: usize| {
                                args.get(index).and_then(|value| match value {
                                    BoundaryValue::I64(id) if *id > 0 => {
                                        u32::try_from(*id).ok().map(crate::registry::ThreadId::new)
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
                                    thread_arg(0).is_some_and(|id| self.kernel.is_running(id)),
                                ))),
                                "__internal_thread_is_exited" => Some(Ok(BoundaryValue::Bool(
                                    thread_arg(0).is_some_and(|id| self.kernel.is_exited(id)),
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
                                                        sender_id: thread_id,
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
                                    let sender_id = thread_arg(0);
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
                                "__internal_math_is_nan" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::Bool(f.is_nan())))
                                }
                                "__internal_math_is_finite" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::Bool(f.is_finite())))
                                }
                                "__internal_math_is_infinite" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::Bool(f.is_infinite())))
                                }
                                "__internal_math_sqrt" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                                        f.sqrt(),
                                    ))))
                                }
                                "__internal_math_hypot" => {
                                    let x = float_arg(0).unwrap_or(0.0);
                                    let y = float_arg(1).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                                        x.hypot(y),
                                    ))))
                                }
                                "__internal_math_sin" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                                        f.sin(),
                                    ))))
                                }
                                "__internal_math_cos" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                                        f.cos(),
                                    ))))
                                }
                                "__internal_math_tan" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                                        f.tan(),
                                    ))))
                                }
                                "__internal_math_log" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(f.ln()))))
                                }
                                "__internal_math_log2" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                                        f.log2(),
                                    ))))
                                }
                                "__internal_math_log10" => {
                                    let f = float_arg(0).unwrap_or(0.0);
                                    Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                                        f.log10(),
                                    ))))
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
                                                galfus_vm::thread::VmThreadState::new(self.quota.clone());
                                            new_thread.entry_func =
                                                Some(galfus_vm::VmValue::Function {
                                                    module_id: ModuleId::new(*module_id),
                                                    func_idx: FuncIdx(*func_idx),
                                                });
                                            match self.kernel.spawn(new_thread, key) {
                                                Ok(id) => id.raw() as i64,
                                                Err(e) => {
                                                    self.failure = Some(galfus_contract::ExecutionFailure::new(galfus_contract::ExecutionFailureKind::BoundaryCodecFailure, e.to_string()).with_thread_id(thread_id).with_stack(crate::task::execution_stack(&thread)));
                                                    self.kernel.cancel(thread_id);
                                                    return;
                                                }
                                            }
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
                                                                                ).map_err(|_| ())?,
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
                                                                    ).map_err(|_| ())?,
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
                                            self.kernel.mark_running(target_id);
                                            self.kernel.mark_spawned(target_id);
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
                                        .with_thread_id(thread_id)
                                        .with_future_id(future_id)
                                        .with_stack(execution_stack(&thread)),
                                    );
                                    self.kernel.cancel(thread_id);
                                    return;
                                }
                            };
                            if let Some(result) = immediate {
                                if aggregate_registration.is_none() {
                                    if !self.block_or_fail(thread_id, thread) {
                                        return;
                                    }
                                }
                                self.complete_future(thread_id, future_id, result);
                                return;
                            }
                        }
                    }
                }

                if aggregate_registration.is_none() {
                    if !self.block_or_fail(thread_id, thread) {
                        return;
                    }
                }
            }
            galfus_vm::VmEffect::CreateFuture {
                module_id,
                target_module_id,
                func_idx,
                args,
                arg_types,
                return_type,
            } => {
                let Some(future_lease) = self.allocate_future_lease(thread_id, &thread) else {
                    return;
                };
                let future_id = future_lease.id;

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
                                .with_thread_id(thread_id)
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
                let Some(future_lease) = self.allocate_future_lease(thread_id, &thread) else {
                    return;
                };
                let future_id = future_lease.id;
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
                        .with_thread_id(thread_id)
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
                                .with_thread_id(thread_id)
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
        future_ids: Vec<galfus_core::FutureId>,
        mode: crate::orchestrator::AggregateMode,
    ) {
        let Some(coordinator_id) = self.allocate_coordinator_id(thread_id, &thread) else {
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
                self.kernel.cancel(thread_id);
                return;
            };
            self.aggregate_registration = Some((coordinator_id, index));
            self.handle_effect(
                thread_id,
                galfus_vm::thread::VmThreadState::new(self.quota.clone()),
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
        if !self.block_or_fail(thread_id, thread) {
            return;
        }
        self.finish_aggregate_if_ready(coordinator_id);
    }

    fn block_or_fail(
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

    pub(super) fn allocate_request_lease(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        thread: &galfus_vm::thread::VmThreadState,
    ) -> Option<galfus_core::RequestLease> {
        if let Some(id) = self.request_id_manager.try_allocate() {
            let gen_val = self
                .request_generations
                .entry(id.raw())
                .and_modify(|g| *g = g.wrapping_add(1))
                .or_insert(1);
            Some(galfus_core::RequestLease::new(id, *gen_val))
        } else {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::IdSpaceExhausted,
                    "request id space exhausted",
                )
                .with_thread_id(thread_id)
                .with_future_id(future_id)
                .with_stack(execution_stack(thread)),
            );
            self.kernel.cancel(thread_id);
            None
        }
    }

    pub(super) fn allocate_future_lease(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: &galfus_vm::thread::VmThreadState,
    ) -> Option<galfus_core::FutureLease> {
        if let Some(id) = self.future_id_manager.try_allocate() {
            let gen_val = self
                .future_generations
                .entry(id.raw())
                .and_modify(|g| *g = g.wrapping_add(1))
                .or_insert(1);
            Some(galfus_core::FutureLease::new(id, *gen_val))
        } else {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::IdSpaceExhausted,
                    "future id space exhausted",
                )
                .with_thread_id(thread_id)
                .with_stack(execution_stack(thread)),
            );
            self.kernel.cancel(thread_id);
            None
        }
    }

    pub(super) fn allocate_coordinator_id(
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
        let adapter_identity = target.functions[func_idx.raw() as usize]
            .adapter_proxy_metadata
            .as_ref()
            .map(|meta| (meta.proxy_module.clone(), meta.symbol.clone()));

        if let Some(name) = function_name.strip_prefix("__provider_") {
            crate::orchestrator::future_registry::Activation::Provider {
                name: name.to_string(),
                args,
                request_id: None,
            }
        } else if function_name.starts_with("__internal_") {
            crate::orchestrator::future_registry::Activation::Internal {
                operation: function_name,
                args,
            }
        } else if let Some((proxy_module, symbol)) = adapter_identity {
            crate::orchestrator::future_registry::Activation::Adapter {
                proxy_module,
                symbol,
                args,
                request_id: None,
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
}
