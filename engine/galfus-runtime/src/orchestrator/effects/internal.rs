use super::*;

use crate::event::FutureValue;
use crate::task::execution_stack;
use galfus_bytecode::instruction::TypeIdx;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};
use galfus_core::ModuleId;

fn internal_thread_arg(
    args: &[galfus_vm::VmValue],
    index: usize,
) -> Option<crate::registry::ThreadId> {
    match args.get(index) {
        Some(galfus_vm::VmValue::Int64(id)) if *id > 0 => {
            u32::try_from(*id).ok().map(crate::registry::ThreadId::new)
        }
        _ => None,
    }
}

fn internal_timeout_arg(args: &[galfus_vm::VmValue], index: usize) -> Option<u64> {
    match args.get(index) {
        Some(galfus_vm::VmValue::Int32(value)) if *value >= 0 => Some(*value as u64),
        Some(galfus_vm::VmValue::Int64(value)) if *value >= 0 => Some(*value as u64),
        _ => None,
    }
}

fn internal_float_arg(args: &[galfus_vm::VmValue], index: usize) -> Option<f64> {
    match args.get(index) {
        Some(galfus_vm::VmValue::Float64(value)) => Some(*value),
        Some(galfus_vm::VmValue::Float32(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Int64(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Int32(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Int16(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Int8(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Uint64(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Uint32(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Uint16(value)) => Some(*value as f64),
        Some(galfus_vm::VmValue::Uint8(value)) => Some(*value as f64),
        _ => None,
    }
}

fn internal_bytes_arg(
    heap: &galfus_vm::thread::PrivateHeap,
    value: Option<&galfus_vm::VmValue>,
) -> Option<Vec<u8>> {
    let Some(galfus_vm::VmValue::Object(reference)) = value else {
        return None;
    };
    let Ok(galfus_vm::HeapObject::Array { elements, .. }) = heap.get_object(*reference) else {
        return None;
    };
    elements
        .iter()
        .map(|value| match value {
            galfus_vm::VmValue::Uint8(value) => Some(*value),
            _ => None,
        })
        .collect()
}

impl Orchestrator {
    pub(super) fn try_complete_internal_await(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread_heap: &mut galfus_vm::thread::PrivateHeap,
        module_id: ModuleId,
        return_type: TypeIdx,
        operation: &str,
        args: &[galfus_vm::VmValue],
    ) -> Option<Result<galfus_vm::VmValue, ExecutionFailure>> {
        match operation {
            "__internal_thread_wait" => {
                match internal_thread_arg(args, 0).and_then(|id| self.kernel.state(id)) {
                    Some(state) if !state.is_exited() => None,
                    Some(state) => Some(Ok(state
                        .exit_reason()
                        .and_then(Result::ok)
                        .map(galfus_vm::VmValue::Int32)
                        .unwrap_or(galfus_vm::VmValue::Null))),
                    None => Some(Ok(galfus_vm::VmValue::Null)),
                }
            }
            "__internal_thread_receive" => {
                let sender_id = internal_thread_arg(args, 0);
                self.kernel
                    .get_mailbox(thread_id)
                    .and_then(|mailbox| {
                        let mut mailbox = mailbox.lock().unwrap();
                        let index = sender_id.map_or_else(
                            || (!mailbox.is_empty()).then_some(0),
                            |sender_id| {
                                mailbox
                                    .iter()
                                    .position(|message| message.sender_id == sender_id)
                            },
                        )?;
                        mailbox.remove(index)
                    })
                    .map(|message| {
                        if let Some(quota) = self.kernel.get_thread_quota(thread_id) {
                            quota.release_mailbox_messages(1);
                            quota.release_mailbox_bytes(message.data.len());
                        }
                        crate::task::encode_future_value_into_thread_heap(
                            thread_heap,
                            FutureValue::Bytes(message.data),
                            return_type,
                            module_id,
                            &self
                                .vm
                                .as_ref()
                                .expect("VM is ready")
                                .graph
                                .get(module_id)
                                .unwrap()
                                .module,
                        )
                        .map_err(|error| {
                            ExecutionFailure::new(
                                ExecutionFailureKind::BoundaryCodecFailure,
                                format!("invalid byte array encoding: {error:?}"),
                            )
                        })
                    })
            }
            "__internal_thread_sleep" => {
                let ms = internal_timeout_arg(args, 0).unwrap_or(0);
                (ms == 0).then_some(Ok(galfus_vm::VmValue::Null))
            }
            _ => None,
        }
    }

    fn send_internal_thread_message(
        &mut self,
        sender_id: crate::registry::ThreadId,
        target_id: Option<crate::registry::ThreadId>,
        data: Option<Vec<u8>>,
    ) -> bool {
        let (Some(target_id), Some(data)) = (target_id, data) else {
            return false;
        };
        let Some(mailbox) = self.kernel.get_mailbox(target_id) else {
            return false;
        };
        let message_bytes = data.len();
        let Some(quota) = self.kernel.get_thread_quota(target_id) else {
            return false;
        };
        if quota.try_reserve_mailbox_messages(1).is_err() {
            return false;
        }
        if quota.try_reserve_mailbox_bytes(message_bytes).is_err() {
            quota.release_mailbox_messages(1);
            return false;
        }
        mailbox
            .lock()
            .unwrap()
            .push_back(crate::registry::MailboxMessage { sender_id, data });
        if let Err(error) = self.kernel.unblock(target_id) {
            self.failure = Some(
                ExecutionFailure::new(error, "runnable threads limit exceeded")
                    .with_thread_id(target_id),
            );
            self.kernel.cancel(target_id);
        }
        self.complete_mailbox_future_waits(target_id, sender_id);
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_internal_thread_call(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        module_id: ModuleId,
        operation: Box<str>,
        args: Vec<galfus_vm::VmValue>,
        return_type: TypeIdx,
    ) {
        let result: Result<galfus_vm::VmValue, ExecutionFailure> = match operation.as_ref() {
            "__internal_thread_get" => Ok(galfus_vm::VmValue::Int64(
                internal_bytes_arg(&thread.heap, args.first())
                    .and_then(|key| std::str::from_utf8(&key).ok().map(|s| s.to_string()))
                    .and_then(|key| self.kernel.lookup_key(&key))
                    .map(|id| id.raw() as i64)
                    .unwrap_or(-1),
            )),
            "__internal_thread_is_running" => Ok(galfus_vm::VmValue::Bool(
                internal_thread_arg(&args, 0).is_some_and(|id| self.kernel.is_running(id)),
            )),
            "__internal_thread_is_exited" => Ok(galfus_vm::VmValue::Bool(
                internal_thread_arg(&args, 0).is_some_and(|id| self.kernel.is_exited(id)),
            )),
            "__internal_thread_exit_reason" => Ok(internal_thread_arg(&args, 0)
                .and_then(|id| self.kernel.state(id))
                .and_then(|state| state.exit_reason())
                .and_then(Result::ok)
                .map(galfus_vm::VmValue::Int32)
                .unwrap_or(galfus_vm::VmValue::Null)),
            "__internal_thread_has_messages" => Ok(galfus_vm::VmValue::Bool(
                self.kernel
                    .get_mailbox(thread_id)
                    .is_some_and(|mailbox| !mailbox.lock().unwrap().is_empty()),
            )),
            "__internal_thread_get_message" | "__internal_thread_try_receive" => {
                let sender_id = (operation.as_ref() == "__internal_thread_try_receive")
                    .then(|| internal_thread_arg(&args, 0))
                    .flatten();
                let message = self.kernel.get_mailbox(thread_id).and_then(|mailbox| {
                    let mut mailbox = mailbox.lock().unwrap();
                    let index = sender_id.map_or_else(
                        || (!mailbox.is_empty()).then_some(0),
                        |sender_id| {
                            mailbox
                                .iter()
                                .position(|message| message.sender_id == sender_id)
                        },
                    )?;
                    mailbox.remove(index)
                });

                match message {
                    Some(message) => {
                        if let Some(quota) = self.kernel.get_thread_quota(thread_id) {
                            quota.release_mailbox_messages(1);
                            quota.release_mailbox_bytes(message.data.len());
                        }
                        crate::task::encode_future_value_into_thread_heap(
                            &mut thread.heap,
                            FutureValue::Bytes(message.data),
                            return_type,
                            module_id,
                            &self
                                .vm
                                .as_ref()
                                .unwrap()
                                .graph
                                .get(module_id)
                                .unwrap()
                                .module,
                        )
                        .map_err(|error| {
                            ExecutionFailure::new(
                                ExecutionFailureKind::BoundaryCodecFailure,
                                format!("invalid internal result: {error:?}"),
                            )
                        })
                    }
                    None => Ok(galfus_vm::VmValue::Null),
                }
            }
            "__internal_thread_send" => {
                Ok(galfus_vm::VmValue::Bool(self.send_internal_thread_message(
                    thread_id,
                    internal_thread_arg(&args, 0),
                    internal_bytes_arg(&thread.heap, args.get(1)),
                )))
            }
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::InvalidBytecode,
                format!("unknown synchronous internal operation: {operation}"),
            )),
        };

        for arg in args {
            if let galfus_vm::VmValue::Object(reference) = arg {
                let _ = thread.heap.release_anchor(reference);
            }
        }

        let value = match result {
            Ok(value) => value,
            Err(error) => {
                self.failure = Some(
                    error
                        .with_thread_id(thread_id)
                        .with_stack(execution_stack(&thread)),
                );
                self.kernel.cancel(thread_id);
                return;
            }
        };

        #[cfg(feature = "metrics")]
        {
            self.future_metrics.internal_immediate += 1;
        }
        self.resume_or_fail_front(thread_id, thread, continuation, value);
    }

    pub(super) fn start_internal_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        future_id: galfus_core::FutureId,
        operation: String,
        args: Vec<galfus_vm::VmValue>,
        aggregate_registration: Option<(galfus_core::CoordinatorId, usize)>,
    ) -> Option<galfus_vm::thread::VmThreadState> {
        let immediate = match operation.as_str() {
            "__internal_thread_get" => {
                let id = internal_bytes_arg(&thread.heap, args.first())
                    .and_then(|key| std::str::from_utf8(&key).ok().map(|s| s.to_string()))
                    .and_then(|key| self.kernel.lookup_key(&key))
                    .map(|id| id.raw() as i64)
                    .unwrap_or(-1);
                Some(Ok(FutureValue::I64(id)))
            }
            "__internal_thread_is_running" => Some(Ok(FutureValue::Bool(
                internal_thread_arg(&args, 0).is_some_and(|id| self.kernel.is_running(id)),
            ))),
            "__internal_thread_is_exited" => Some(Ok(FutureValue::Bool(
                internal_thread_arg(&args, 0).is_some_and(|id| self.kernel.is_exited(id)),
            ))),
            "__internal_thread_exit_reason" => Some(Ok(internal_thread_arg(&args, 0)
                .and_then(|id| self.kernel.state(id))
                .and_then(|state| state.exit_reason())
                .and_then(Result::ok)
                .map(FutureValue::I32)
                .unwrap_or(FutureValue::Null))),
            "__internal_thread_send" => {
                Some(Ok(FutureValue::Bool(self.send_internal_thread_message(
                    thread_id,
                    internal_thread_arg(&args, 0),
                    internal_bytes_arg(&thread.heap, args.get(1)),
                ))))
            }
            "__internal_thread_has_messages" => Some(Ok(FutureValue::Bool(
                self.kernel
                    .get_mailbox(thread_id)
                    .is_some_and(|mailbox| !mailbox.lock().unwrap().is_empty()),
            ))),
            "__internal_thread_get_message" => Some(Ok(self
                .kernel
                .get_mailbox(thread_id)
                .and_then(|mailbox| mailbox.lock().unwrap().pop_front())
                .map(|message| {
                    if let Some(target_quota) = self.kernel.get_thread_quota(thread_id) {
                        let tq = target_quota;
                        tq.release_mailbox_messages(1);
                        tq.release_mailbox_bytes(message.data.len());
                    }
                    FutureValue::Bytes(message.data)
                })
                .unwrap_or(FutureValue::Null))),
            "__internal_thread_wait" => {
                match internal_thread_arg(&args, 0).and_then(|id| self.kernel.state(id)) {
                    Some(state) if !state.is_exited() => {
                        if let Some(target_id) = internal_thread_arg(&args, 0) {
                            self.register_thread_exit_future(target_id, thread_id, future_id);
                        }
                        None
                    }
                    Some(state) => Some(Ok(state
                        .exit_reason()
                        .and_then(Result::ok)
                        .map(FutureValue::I32)
                        .unwrap_or(FutureValue::Null))),
                    None => Some(Ok(FutureValue::Null)),
                }
            }
            "__internal_thread_receive" => {
                let sender_id = internal_thread_arg(&args, 0);
                let timeout_ms = internal_timeout_arg(&args, 1);
                let message = self.kernel.get_mailbox(thread_id).and_then(|mailbox| {
                    let mut mailbox = mailbox.lock().unwrap();
                    let index = sender_id.map_or_else(
                        || (!mailbox.is_empty()).then_some(0),
                        |sender_id| {
                            mailbox
                                .iter()
                                .position(|message| message.sender_id == sender_id)
                        },
                    )?;
                    let msg = mailbox.remove(index);
                    if let Some(ref m) = msg
                        && let Some(target_quota) = self.kernel.get_thread_quota(thread_id)
                    {
                        let tq = target_quota;
                        tq.release_mailbox_messages(1);
                        tq.release_mailbox_bytes(m.data.len());
                    }
                    msg
                });
                match message {
                    Some(message) => Some(Ok(FutureValue::Bytes(message.data))),
                    None => {
                        self.register_mailbox_future_wait(
                            thread_id, thread_id, future_id, sender_id, timeout_ms,
                        );
                        None
                    }
                }
            }
            "__internal_math_is_nan" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::Bool(f.is_nan())))
            }
            "__internal_math_is_finite" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::Bool(f.is_finite())))
            }
            "__internal_math_is_infinite" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::Bool(f.is_infinite())))
            }
            "__internal_math_sqrt" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(f.sqrt()))))
            }
            "__internal_math_hypot" => {
                let x = internal_float_arg(&args, 0).unwrap_or(0.0);
                let y = internal_float_arg(&args, 1).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(x.hypot(y)))))
            }
            "__internal_math_sin" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(f.sin()))))
            }
            "__internal_math_cos" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(f.cos()))))
            }
            "__internal_math_tan" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(f.tan()))))
            }
            "__internal_math_log" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(f.ln()))))
            }
            "__internal_math_log2" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(f.log2()))))
            }
            "__internal_math_log10" => {
                let f = internal_float_arg(&args, 0).unwrap_or(0.0);
                Some(Ok(FutureValue::F64(galfus_core::normalize_f64(f.log10()))))
            }
            "__internal_thread_create" => {
                let key = internal_bytes_arg(&thread.heap, args.get(1))
                    .and_then(|key| String::from_utf8(key).ok());
                let id = match args.first() {
                    Some(galfus_vm::VmValue::Function {
                        module_id,
                        func_idx,
                    }) => {
                        let mut new_thread = galfus_vm::thread::VmThreadState::new(
                            self.quota.clone(),
                            std::sync::Arc::new(galfus_vm::quota::ThreadQuota::new(
                                self.quota.lock().unwrap().limits().clone(),
                            )),
                        );
                        new_thread.entry_func = Some(galfus_vm::VmValue::Function {
                            module_id: *module_id,
                            func_idx: *func_idx,
                        });
                        match self.kernel.spawn(new_thread, key) {
                            Ok(id) => id.raw() as i64,
                            Err(e) => {
                                self.failure = Some(
                                    galfus_contract::ExecutionFailure::new(
                                        galfus_contract::ExecutionFailureKind::BoundaryCodecFailure,
                                        e.to_string(),
                                    )
                                    .with_thread_id(thread_id)
                                    .with_stack(crate::task::execution_stack(&thread)),
                                );
                                self.kernel.cancel(thread_id);
                                return None;
                            }
                        }
                    }
                    _ => -1,
                };
                Some(Ok(FutureValue::I64(id)))
            }
            "__internal_thread_sleep" => {
                let ms = internal_timeout_arg(&args, 0).unwrap_or(0);
                self.register_timer_future_wait(thread_id, future_id, ms);
                None
            }
            "__internal_thread_spawn" => {
                let success = internal_thread_arg(&args, 0).is_some_and(|target_id| {
                    let Some(mut target_thread) = self.kernel.take_created_thread(target_id) else {
                        return false;
                    };
                    let prepared = match target_thread.entry_func {
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
                                Some(galfus_vm::VmValue::Null) | None => Ok(galfus_vm::VmValue::Null),
                                Some(galfus_vm::VmValue::Object(reference)) => {
                                    if let Ok(galfus_vm::HeapObject::Array { elements, .. }) = thread.heap.get_object(*reference) {
                                        let byte_type = module
                                            .types
                                            .iter()
                                            .position(|ty| {
                                                matches!(ty, galfus_bytecode::BytecodeType::Uint8)
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
                                                let arrays = elements
                                                    .iter()
                                                    .map(|value| {
                                                        let bytes = internal_bytes_arg(&thread.heap, Some(value)).ok_or(())?;
                                                        Ok(
                                                            galfus_vm::VmValue::Object(
                                                                target_thread.heap.alloc(
                                                                    galfus_vm::HeapObject::Array {
                                                                        module_id: target_thread.call_stack.last().expect("spawned thread has an entry frame").module_id,
                                                                        element_ty: byte_type,
                                                                        elements: bytes
                                                                            .iter()
                                                                            .copied()
                                                                            .map(galfus_vm::VmValue::Uint8)
                                                                            .collect(),
                                                                    },
                                                                ).map_err(|_| ())?,
                                                            ),
                                                        )
                                                    })
                                                    .collect::<Result<Vec<_>, _>>()?;
                                                Ok(galfus_vm::VmValue::Object(
                                                    target_thread
                                                        .heap
                                                        .alloc(galfus_vm::HeapObject::Array {
                                                            module_id: target_thread.call_stack.last().expect("spawned thread has an entry frame").module_id,
                                                            element_ty: bytes_type,
                                                            elements: arrays,
                                                        })
                                                        .map_err(|_| ())?,
                                                ))
                                            },
                                        )
                                    } else {
                                        Err(())
                                    }
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
                        if let Err(e) = self.kernel.mark_spawned(target_id) {
                            self.kernel.park_running(target_id, target_thread);
                            self.failure = Some(
                                galfus_contract::ExecutionFailure::new(
                                    e,
                                    "failed to mark thread as spawned",
                                )
                                .with_thread_id(thread_id),
                            );
                            false
                        } else {
                            if let Err(e) = self.kernel.enqueue_runnable(target_id, target_thread) {
                                self.failure = Some(
                                    galfus_contract::ExecutionFailure::new(
                                        e,
                                        "runnable threads limit exceeded",
                                    )
                                    .with_thread_id(thread_id),
                                );
                                false
                            } else {
                                self.kernel.mark_running(target_id);
                                true
                            }
                        }
                    } else {
                        self.kernel.park_running(target_id, target_thread);
                        false
                    }
                });
                Some(Ok(FutureValue::Bool(success)))
            }
            _ => {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::InvalidBytecode,
                        format!("unknown internal future activation: {operation}"),
                    )
                    .with_thread_id(thread_id)
                    .with_future_id(future_id)
                    .with_stack(execution_stack(&thread)),
                );
                self.kernel.cancel(thread_id);
                return None;
            }
        };

        for arg in args {
            if let galfus_vm::VmValue::Object(reference) = arg {
                let _ = thread.heap.release_anchor(reference);
            }
        }

        if let Some(result) = immediate {
            #[cfg(feature = "metrics")]
            {
                self.future_metrics.internal_immediate += 1;
            }
            if aggregate_registration.is_none() && !self.block_or_fail(thread_id, thread) {
                return None;
            }
            self.complete_future(thread_id, future_id, result);
            return None;
        }
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.internal_suspended += 1;
        }
        Some(thread)
    }
}
