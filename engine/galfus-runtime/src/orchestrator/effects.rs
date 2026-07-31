use super::Orchestrator;
use crate::event::RuntimeEvent;
use crate::orchestrator::external::{AdapterDispatchTask, ProviderDispatchTask};
use crate::orchestrator::pending::{PendingContinuation, PendingKey, PendingOperation};
use crate::task::execution_stack;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind, KernelTask, TaskAffinity};
use std::sync::{Arc, atomic::AtomicBool};

impl Orchestrator {
    pub(super) fn handle_effect(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        effect: galfus_vm::VmEffect,
        continuation: galfus_vm::Continuation,
    ) {
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
                let Some(adapters) = self.adapters.clone() else {
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
                let affinity = adapters
                    .lock()
                    .unwrap()
                    .get_mut(&adapter, &symbol)
                    .map(|adapter| adapter.affinity());
                let Some(affinity) = affinity else {
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
                        operation: PendingOperation::Adapter {
                            module: adapter.clone(),
                            symbol: symbol.clone(),
                        },
                        active: active.clone(),
                    },
                );
                self.kernel.block(thread_id, thread, None);
                let task = AdapterDispatchTask {
                    adapters,
                    thread_id: thread_id.raw() as usize,
                    request_id,
                    module: adapter,
                    symbol,
                    args,
                    injector: Arc::new(crate::ExecutionHandle::new(self.sink.clone())),
                    active,
                };
                self.driver.as_ref().unwrap().dispatch(match affinity {
                    TaskAffinity::Main => KernelTask::Main(Box::new(task)),
                    TaskAffinity::Any => KernelTask::Any(Box::new(task)),
                });
                return;
            }
            galfus_vm::VmEffect::TimerWait { delay_ms } => {
                self.kernel.block(thread_id, thread, Some(delay_ms));
            }
            galfus_vm::VmEffect::FutureWait {
                future_id,
                module_id,
                return_type,
            } => {
                let key = PendingKey::Future(future_id);
                if self.pending_continuations.contains_key(&key) {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::InvalidContinuation,
                            "future is already awaited",
                        )
                        .with_thread_id(thread_id.raw())
                        .with_future_id(future_id)
                        .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                }
                self.pending_continuations.insert(
                    key,
                    PendingContinuation {
                        thread_id,
                        continuation,
                        module_id,
                        return_type,
                        request_id: future_id,
                        stack: execution_stack(&thread),
                        operation: PendingOperation::Future,
                        active: Arc::new(AtomicBool::new(true)),
                    },
                );
                self.kernel.block(thread_id, thread, None);
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
                self.resume_or_fail(
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
                    let target_thread = self.kernel.take_thread(target_id).filter(|_| {
                        self.kernel.state(target_id) == Some(crate::registry::ThreadState::Created)
                    });
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

                self.resume_or_fail(
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
                self.resume_or_fail_front(thread_id, thread, continuation, val);
            }
            galfus_vm::VmEffect::ThreadIsRunning {
                thread_id: target_id,
            } => {
                let running = crate::registry::ThreadId::from_raw(target_id)
                    .and_then(|target_id| {
                        self.kernel.take_thread(target_id).map(|t| {
                            let state = self
                                .kernel
                                .state(target_id)
                                .unwrap_or(crate::registry::ThreadState::Exited(0));
                            self.kernel.park_running(target_id, t);
                            state
                        })
                    })
                    .is_some_and(|state| state.is_running());
                self.resume_or_fail_front(
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
                    .and_then(|target_id| {
                        self.kernel.take_thread(target_id).map(|t| {
                            let state = self
                                .kernel
                                .state(target_id)
                                .unwrap_or(crate::registry::ThreadState::Exited(0));
                            self.kernel.park_running(target_id, t);
                            state
                        })
                    })
                    .is_some_and(|state| state.is_exited());
                self.resume_or_fail_front(
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
                    .and_then(|target_id| {
                        self.kernel.take_thread(target_id).map(|t| {
                            let state = self
                                .kernel
                                .state(target_id)
                                .unwrap_or(crate::registry::ThreadState::Exited(0));
                            self.kernel.park_running(target_id, t);
                            state
                        })
                    })
                    .and_then(|state| state.exit_reason())
                    .map(galfus_vm::VmValue::Int32)
                    .unwrap_or(galfus_vm::VmValue::Null);
                self.resume_or_fail_front(thread_id, thread, continuation, reason);
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
                let maybe_target = crate::registry::ThreadId::from_raw(target_raw);
                let exit_code = maybe_target
                    .and_then(|target| self.kernel.state(target))
                    .and_then(|state| state.exit_reason());
                if let Some(code) = exit_code {
                    // Target already exited — resume immediately with the exit code.
                    self.resume_or_fail_front(
                        thread_id,
                        thread,
                        continuation,
                        galfus_vm::VmValue::Int32(code),
                    );
                } else {
                    // Target still running — block the caller and register it as a waiter.
                    match maybe_target {
                        Some(target_id) => {
                            self.kernel.block(thread_id, thread, None);
                            self.kernel
                                .register_waiter(target_id, thread_id, continuation);
                        }
                        None => {
                            // Invalid thread id — resume with null (no exit code).
                            self.resume_or_fail_front(
                                thread_id,
                                thread,
                                continuation,
                                galfus_vm::VmValue::Null,
                            );
                        }
                    }
                }
            }
            galfus_vm::VmEffect::CreateFuture {
                module_id: _,
                func_idx: _,
                args: _,
            } => {
                let future_id = self.next_request_id;
                self.next_request_id += 1;
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Uint64(future_id),
                );
            }
            galfus_vm::VmEffect::FutureWaitAll {
                future_ids: _,
                module_id: _,
            } => {
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Null,
                );
            }
            galfus_vm::VmEffect::FutureWaitRace {
                future_ids: _,
                module_id: _,
            } => {
                self.resume_or_fail_front(
                    thread_id,
                    thread,
                    continuation,
                    galfus_vm::VmValue::Null,
                );
            }
        }
    }
}
