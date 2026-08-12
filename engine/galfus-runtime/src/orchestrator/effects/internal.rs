use super::*;

use crate::task::execution_stack;
use galfus_bytecode::instruction::FuncIdx;
use galfus_contract::{BoundaryValue, ExecutionFailure, ExecutionFailureKind};
use galfus_core::ModuleId;

impl Orchestrator {
    pub(super) fn start_internal_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        future_id: galfus_core::FutureId,
        operation: String,
        args: Vec<galfus_contract::BoundaryValue>,
        aggregate_registration: Option<(galfus_core::CoordinatorId, usize)>,
    ) -> Option<galfus_vm::thread::VmThreadState> {
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
                        BoundaryValue::Bytes(key) => std::str::from_utf8(key).ok(),
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
                    Ok(BoundaryValue::I32(code)) => Some(BoundaryValue::I32(code)),
                    _ => None,
                })
                .unwrap_or(BoundaryValue::Null))),
            "__internal_thread_send" => {
                let success = match (thread_arg(0), args.get(1)) {
                    (Some(target_id), Some(BoundaryValue::Bytes(data))) => {
                        if let Some(mailbox) = self.kernel.get_mailbox(target_id) {
                            let message_bytes = data.len();
                            let mut ok = false;
                            if let Some(target_quota) = self.kernel.get_thread_quota(target_id) {
                                let mut quota = target_quota.lock().unwrap();
                                if quota.try_reserve_mailbox_messages(1).is_ok() {
                                    if quota.try_reserve_mailbox_bytes(message_bytes).is_ok() {
                                        ok = true;
                                    } else {
                                        quota.release_mailbox_messages(1);
                                    }
                                }
                            }
                            if ok {
                                mailbox.lock().unwrap().push_back(
                                    crate::registry::MailboxMessage {
                                        sender_id: thread_id,
                                        data: data.clone(),
                                    },
                                );
                                if let Err(e) = self.kernel.unblock(target_id) {
                                    self.failure = Some(
                                        galfus_contract::ExecutionFailure::new(
                                            e,
                                            "runnable threads limit exceeded",
                                        )
                                        .with_thread_id(target_id),
                                    );
                                    self.kernel.cancel(target_id);
                                }
                                self.complete_mailbox_future_waits(target_id);
                                true
                            } else {
                                false
                            }
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
                .map(|message| {
                    if let Some(target_quota) = self.kernel.get_thread_quota(thread_id) {
                        let mut tq = target_quota.lock().unwrap();
                        tq.release_mailbox_messages(1);
                        tq.release_mailbox_bytes(message.data.len());
                    }
                    BoundaryValue::Bytes(message.data)
                })
                .unwrap_or(BoundaryValue::Null))),
            "__internal_thread_wait" => match thread_arg(0).and_then(|id| self.kernel.state(id)) {
                Some(state) if !state.is_exited() => {
                    if let Some(target_id) = thread_arg(0) {
                        self.register_thread_exit_future(target_id, thread_id, future_id);
                    }
                    None
                }
                Some(state) => Some(Ok(state
                    .exit_reason()
                    .and_then(Result::ok)
                    .unwrap_or(BoundaryValue::Null))),
                None => Some(Ok(BoundaryValue::Null)),
            },
            "__internal_thread_receive" => {
                let sender_id = thread_arg(0);
                let timeout_ms = args.get(1).and_then(|value| match value {
                    BoundaryValue::I32(ms) if *ms >= 0 => Some(*ms as u64),
                    BoundaryValue::I64(ms) if *ms >= 0 => Some(*ms as u64),
                    _ => None,
                });
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
                    if let Some(ref m) = msg {
                        if let Some(target_quota) = self.kernel.get_thread_quota(thread_id) {
                            let mut tq = target_quota.lock().unwrap();
                            tq.release_mailbox_messages(1);
                            tq.release_mailbox_bytes(m.data.len());
                        }
                    }
                    msg
                });
                match message {
                    Some(message) => Some(Ok(BoundaryValue::Bytes(message.data))),
                    None => {
                        self.register_mailbox_future_wait(
                            thread_id, thread_id, future_id, sender_id, timeout_ms,
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
                Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(f.sqrt()))))
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
                Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(f.sin()))))
            }
            "__internal_math_cos" => {
                let f = float_arg(0).unwrap_or(0.0);
                Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(f.cos()))))
            }
            "__internal_math_tan" => {
                let f = float_arg(0).unwrap_or(0.0);
                Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(f.tan()))))
            }
            "__internal_math_log" => {
                let f = float_arg(0).unwrap_or(0.0);
                Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(f.ln()))))
            }
            "__internal_math_log2" => {
                let f = float_arg(0).unwrap_or(0.0);
                Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(f.log2()))))
            }
            "__internal_math_log10" => {
                let f = float_arg(0).unwrap_or(0.0);
                Some(Ok(BoundaryValue::F64(galfus_core::normalize_f64(
                    f.log10(),
                ))))
            }
            "__internal_thread_create" => {
                let key = args.get(1).and_then(|value| match value {
                    BoundaryValue::Bytes(key) => String::from_utf8(key.clone()).ok(),
                    BoundaryValue::Null => None,
                    _ => None,
                });
                let id = match args.first() {
                    Some(BoundaryValue::Function {
                        module_id,
                        func_idx,
                    }) => {
                        let mut new_thread = galfus_vm::thread::VmThreadState::new(
                            self.quota.clone(),
                            std::sync::Arc::new(std::sync::Mutex::new(
                                galfus_vm::quota::ThreadQuota::new(
                                    self.quota.lock().unwrap().limits().clone(),
                                ),
                            )),
                        );
                        new_thread.entry_func = Some(galfus_vm::VmValue::Function {
                            module_id: ModuleId::new(*module_id),
                            func_idx: FuncIdx(*func_idx),
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
                Some(Ok(BoundaryValue::I64(id)))
            }
            "__internal_thread_sleep" => {
                let ms = args
                    .get(0)
                    .and_then(|v| match v {
                        BoundaryValue::I32(m) if *m >= 0 => Some(*m as u64),
                        BoundaryValue::I64(m) if *m >= 0 => Some(*m as u64),
                        _ => None,
                    })
                    .unwrap_or(0);
                self.register_timer_future_wait(thread_id, future_id, ms);
                None
            }
            "__internal_thread_spawn" => {
                let success = thread_arg(0).is_some_and(|target_id| {
                    let Some(mut target_thread) = self.kernel.take_created_thread(target_id) else {
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
                                Some(BoundaryValue::Null) | None => Ok(galfus_vm::VmValue::Null),
                                Some(BoundaryValue::Array { values, .. }) => {
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
                                            let arrays = values
                                                .iter()
                                                .map(|value| {
                                                    match value {
                                                BoundaryValue::Bytes(bytes) => Ok(
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
                                                ),
                                                _ => Err(()),
                                            }
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
                Some(Ok(BoundaryValue::Bool(success)))
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
        if let Some(result) = immediate {
            if aggregate_registration.is_none() {
                if !self.block_or_fail(thread_id, thread) {
                    return None;
                }
            }
            self.complete_future(thread_id, future_id, result);
            return None;
        }
        Some(thread)
    }
}
