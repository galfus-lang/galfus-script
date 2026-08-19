#[cfg(test)]
mod tests;

use crate::driver::RuntimeEventSink;
use crate::registry;
use galfus_contract::{
    ExecutionFailure, ExecutionFailureKind, ExecutionFrame, RunnableTask, ThreadResult,
};
use galfus_vm::VirtualMachine;

use std::sync::Arc;

pub(crate) fn decode_from_thread_heap(
    heap: &galfus_vm::thread::PrivateHeap,
    value: galfus_vm::VmValue,
    expected: galfus_bytecode::instruction::TypeIdx,
    module: &galfus_bytecode::BytecodeModule,
) -> Result<galfus_contract::BoundaryValue, galfus_contract::BoundaryCodecError> {
    use galfus_bytecode::BytecodeType;
    use galfus_contract::{BoundaryCodecError, BoundaryValue};

    let expected_type = module
        .types
        .get(expected.raw() as usize)
        .ok_or(BoundaryCodecError::UnsupportedType)?;
    let found = format!("{value:?}");
    let mismatch = || BoundaryCodecError::TypeMismatch {
        expected: format!("{expected_type:?}"),
        found: found.clone(),
    };
    match (expected_type, value) {
        (BytecodeType::Null, galfus_vm::VmValue::Null) => Ok(BoundaryValue::Null),
        (BytecodeType::Bool, galfus_vm::VmValue::Bool(value)) => Ok(BoundaryValue::Bool(value)),
        (BytecodeType::Int8, galfus_vm::VmValue::Int8(value)) => Ok(BoundaryValue::I8(value)),
        (BytecodeType::Int16, galfus_vm::VmValue::Int16(value)) => Ok(BoundaryValue::I16(value)),
        (BytecodeType::Int32, galfus_vm::VmValue::Int32(value)) => Ok(BoundaryValue::I32(value)),
        (BytecodeType::Int64, galfus_vm::VmValue::Int64(value)) => Ok(BoundaryValue::I64(value)),
        (BytecodeType::Uint8, galfus_vm::VmValue::Uint8(value)) => Ok(BoundaryValue::U8(value)),
        (BytecodeType::Uint16, galfus_vm::VmValue::Uint16(value)) => Ok(BoundaryValue::U16(value)),
        (BytecodeType::Uint32, galfus_vm::VmValue::Uint32(value)) => Ok(BoundaryValue::U32(value)),
        (BytecodeType::Uint64, galfus_vm::VmValue::Uint64(value)) => Ok(BoundaryValue::U64(value)),
        (BytecodeType::Float32, galfus_vm::VmValue::Float32(value)) => {
            Ok(BoundaryValue::F32(galfus_core::normalize_f32(value)))
        }
        (BytecodeType::Float64, galfus_vm::VmValue::Float64(value)) => {
            Ok(BoundaryValue::F64(galfus_core::normalize_f64(value)))
        }
        (
            BytecodeType::Function { .. },
            galfus_vm::VmValue::Function {
                module_id,
                func_idx,
            },
        ) => Ok(BoundaryValue::Function {
            module_id: module_id.raw(),
            func_idx: func_idx.raw(),
        }),
        (BytecodeType::AdapterHandle(type_id), galfus_vm::VmValue::Object(reference)) => {
            match heap.get_object(reference) {
                Ok(galfus_vm::HeapObject::AdapterHandle {
                    binding_id,
                    type_id: actual,
                    id,
                }) if actual == type_id => Ok(BoundaryValue::Handle {
                    type_id: type_id.clone(),
                    binding_id: Some(*binding_id),
                    id: *id,
                }),
                _ => Err(mismatch()),
            }
        }
        (BytecodeType::Array(element_type), galfus_vm::VmValue::Object(reference)) => {
            let galfus_vm::HeapObject::Array { elements, .. } = heap
                .get_object(reference)
                .map_err(|_| BoundaryCodecError::UnsupportedType)?
            else {
                return Err(mismatch());
            };
            if let Some(BytecodeType::Uint8) = module.types.get(element_type.raw() as usize) {
                let bytes: Result<Vec<u8>, _> = elements
                    .iter()
                    .map(|element| {
                        if let galfus_vm::VmValue::Uint8(b) = element {
                            Ok(*b)
                        } else {
                            Err(mismatch())
                        }
                    })
                    .collect();
                return Ok(BoundaryValue::Bytes(bytes?));
            }
            let values = elements
                .iter()
                .cloned()
                .map(|element| decode_from_thread_heap(heap, element, *element_type, module))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BoundaryValue::Array {
                element_type: module.boundary_type(*element_type)?,
                values,
            })
        }
        (BytecodeType::Nullable(_), galfus_vm::VmValue::Null) => Ok(BoundaryValue::Null),
        (BytecodeType::Nullable(inner), value) => {
            decode_from_thread_heap(heap, value, *inner, module)
        }
        (BytecodeType::Tuple(element_types), galfus_vm::VmValue::Object(reference)) => {
            let galfus_vm::HeapObject::Tuple { elements } = heap
                .get_object(reference)
                .map_err(|_| BoundaryCodecError::UnsupportedType)?
            else {
                return Err(mismatch());
            };
            if elements.len() != element_types.len() {
                return Err(mismatch());
            }
            let values = elements
                .iter()
                .cloned()
                .zip(element_types)
                .map(|(element, ty)| decode_from_thread_heap(heap, element, *ty, module))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BoundaryValue::Tuple(values))
        }
        (BytecodeType::Choice(layout_idx), galfus_vm::VmValue::Object(reference)) => {
            let galfus_vm::HeapObject::Choice {
                layout_idx: actual_layout,
                variant_idx,
                payload,
                ..
            } = heap
                .get_object(reference)
                .map_err(|_| BoundaryCodecError::UnsupportedType)?
            else {
                return Err(mismatch());
            };
            if actual_layout != layout_idx {
                return Err(mismatch());
            }
            let variant = module
                .choice_layouts
                .get(layout_idx.raw() as usize)
                .and_then(|layout| layout.variants.get(*variant_idx as usize))
                .ok_or(BoundaryCodecError::UnsupportedType)?;
            let payload = variant
                .payload_ty
                .map(|payload_type| {
                    decode_from_thread_heap(heap, *payload, payload_type, module)
                        .map(Box::new)
                })
                .transpose()?;
            Ok(BoundaryValue::Choice {
                variant: *variant_idx as u32,
                payload,
            })
        }
        _ => Err(mismatch()),
    }
}

pub(crate) fn encode_into_thread_heap(
    heap: &mut galfus_vm::thread::PrivateHeap,
    value: galfus_contract::BoundaryValue,
    expected: galfus_bytecode::instruction::TypeIdx,
    module_id: galfus_core::ModuleId,
    module: &galfus_bytecode::BytecodeModule,
) -> Result<galfus_vm::VmValue, galfus_contract::BoundaryCodecError> {
    use galfus_bytecode::BytecodeType;
    use galfus_contract::{BoundaryCodecError, BoundaryValue};

    let expected_type = module
        .types
        .get(expected.raw() as usize)
        .ok_or(BoundaryCodecError::UnsupportedType)?;
    let found = format!("{value:?}");
    let mismatch = || BoundaryCodecError::TypeMismatch {
        expected: format!("{expected_type:?}"),
        found: found.clone(),
    };
    match (expected_type, value) {
        (BytecodeType::Null, BoundaryValue::Null) => Ok(galfus_vm::VmValue::Null),
        (BytecodeType::Bool, BoundaryValue::Bool(value)) => Ok(galfus_vm::VmValue::Bool(value)),
        (BytecodeType::Int8, BoundaryValue::I8(value)) => Ok(galfus_vm::VmValue::Int8(value)),
        (BytecodeType::Int16, BoundaryValue::I16(value)) => Ok(galfus_vm::VmValue::Int16(value)),
        (BytecodeType::Int32, BoundaryValue::I32(value)) => Ok(galfus_vm::VmValue::Int32(value)),
        (BytecodeType::Int64, BoundaryValue::I64(value)) => Ok(galfus_vm::VmValue::Int64(value)),
        (BytecodeType::Uint8, BoundaryValue::U8(value)) => Ok(galfus_vm::VmValue::Uint8(value)),
        (BytecodeType::Uint16, BoundaryValue::U16(value)) => Ok(galfus_vm::VmValue::Uint16(value)),
        (BytecodeType::Uint32, BoundaryValue::U32(value)) => Ok(galfus_vm::VmValue::Uint32(value)),
        (BytecodeType::Uint64, BoundaryValue::U64(value)) => Ok(galfus_vm::VmValue::Uint64(value)),
        (BytecodeType::Float32, BoundaryValue::F32(value)) => Ok(galfus_vm::VmValue::Float32(
            galfus_core::normalize_f32(value),
        )),
        (BytecodeType::Float64, BoundaryValue::F64(value)) => Ok(galfus_vm::VmValue::Float64(
            galfus_core::normalize_f64(value),
        )),
        (
            BytecodeType::Function { .. },
            BoundaryValue::Function {
                module_id,
                func_idx,
            },
        ) => Ok(galfus_vm::VmValue::Function {
            module_id: galfus_core::ModuleId::new(module_id),
            func_idx: galfus_bytecode::instruction::FuncIdx(func_idx),
        }),
        (
            BytecodeType::AdapterHandle(type_id),
            BoundaryValue::Handle {
                type_id: actual,
                binding_id: Some(binding_id),
                id,
            },
        ) if type_id == &actual => Ok(galfus_vm::VmValue::Object(
            heap.alloc(galfus_vm::HeapObject::AdapterHandle {
                binding_id,
                type_id: actual,
                id,
            })
            .map_err(|_| BoundaryCodecError::HeapExhausted)?,
        )),
        (BytecodeType::Array(element_type), BoundaryValue::Bytes(bytes))
            if matches!(
                module.types.get(element_type.raw() as usize),
                Some(BytecodeType::Uint8)
            ) =>
        {
            let elements = bytes.into_iter().map(galfus_vm::VmValue::Uint8).collect();
            let reference = heap
                .alloc(galfus_vm::HeapObject::Array {
                    module_id,
                    element_ty: *element_type,
                    elements,
                })
                .map_err(|_| BoundaryCodecError::HeapExhausted)?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (
            BytecodeType::Array(element_type),
            BoundaryValue::Array {
                element_type: actual_element_type,
                values,
            },
        ) => {
            if actual_element_type != module.boundary_type(*element_type)? {
                return Err(mismatch());
            }
            let elements = values
                .into_iter()
                .map(|element| {
                    encode_into_thread_heap(heap, element, *element_type, module_id, module)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap
                .alloc(galfus_vm::HeapObject::Array {
                    module_id,
                    element_ty: *element_type,
                    elements,
                })
                .map_err(|_| BoundaryCodecError::HeapExhausted)?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (BytecodeType::Nullable(_), BoundaryValue::Null) => Ok(galfus_vm::VmValue::Null),
        (BytecodeType::Nullable(inner), value) => {
            encode_into_thread_heap(heap, value, *inner, module_id, module)
        }
        (BytecodeType::Tuple(element_types), BoundaryValue::Tuple(values)) => {
            if values.len() != element_types.len() {
                return Err(mismatch());
            }
            let elements = values
                .into_iter()
                .zip(element_types)
                .map(|(element, ty)| encode_into_thread_heap(heap, element, *ty, module_id, module))
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap
                .alloc(galfus_vm::HeapObject::Tuple { elements })
                .map_err(|_| BoundaryCodecError::HeapExhausted)?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (BytecodeType::Choice(layout_idx), BoundaryValue::Choice { variant, payload }) => {
            let layout = module
                .choice_layouts
                .get(layout_idx.raw() as usize)
                .ok_or(BoundaryCodecError::UnsupportedType)?;
            let variant_layout = layout.variants.get(variant as usize).ok_or_else(mismatch)?;
            let payload = match (variant_layout.payload_ty, payload) {
                (None, None) => galfus_vm::VmValue::Null,
                (Some(payload_type), Some(payload)) => {
                    encode_into_thread_heap(heap, *payload, payload_type, module_id, module)?
                }
                _ => return Err(mismatch()),
            };
            let reference = heap
                .alloc(galfus_vm::HeapObject::Choice {
                    module_id,
                    layout_idx: *layout_idx,
                    variant_idx: variant as u16,
                    payload,
                })
                .map_err(|_| BoundaryCodecError::HeapExhausted)?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        _ => Err(mismatch()),
    }
}

pub struct RuntimeTask {
    pub thread_id: registry::ThreadId,
    pub thread: Option<galfus_vm::thread::VmThreadState>,
    pub vm: Arc<VirtualMachine>,
    pub events: Arc<dyn RuntimeEventSink>,
    pub future_completion: Option<(registry::ThreadId, galfus_core::FutureLease)>,
}

pub(crate) struct QuotaTask<T: galfus_contract::RunnableTask> {
    inner: Option<T>,
    quota: Arc<std::sync::Mutex<galfus_vm::quota::GlobalQuota>>,
}

impl<T: galfus_contract::RunnableTask> QuotaTask<T> {
    pub(crate) fn new(
        inner: T,
        quota: Arc<std::sync::Mutex<galfus_vm::quota::GlobalQuota>>,
    ) -> Self {
        Self {
            inner: Some(inner),
            quota,
        }
    }
}

impl<T: galfus_contract::RunnableTask> galfus_contract::RunnableTask for QuotaTask<T> {
    fn run(mut self: Box<Self>, budget: usize) -> galfus_contract::ThreadResult {
        let inner = self.inner.take().unwrap();
        Box::new(inner).run(budget)
    }
}

impl<T: galfus_contract::RunnableTask> Drop for QuotaTask<T> {
    fn drop(&mut self) {
        self.quota.lock().unwrap().release_kernel_tasks(1);
    }
}

impl RuntimeTask {
    pub(crate) fn new(
        thread_id: registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        vm: Arc<VirtualMachine>,
        events: Arc<dyn RuntimeEventSink>,
        future_completion: Option<(registry::ThreadId, galfus_core::FutureLease)>,
    ) -> Self {
        Self {
            thread_id,
            thread: Some(thread),
            vm,
            events,
            future_completion,
        }
    }
}

fn with_initialization_context(
    thread: &galfus_vm::thread::VmThreadState,
    failure: ExecutionFailure,
) -> ExecutionFailure {
    match thread.initializing_module() {
        Some(module_id) => {
            let mut initialization_failure = ExecutionFailure::new(
                ExecutionFailureKind::InitializationFailure,
                "module initializer failed",
            )
            .with_module_id(module_id.raw().into());
            if let Some(thread_id) = failure.thread_id {
                initialization_failure = initialization_failure.with_thread_id(thread_id);
            }
            initialization_failure.with_cause(failure)
        }
        None => failure,
    }
}

pub(crate) fn execution_stack(thread: &galfus_vm::thread::VmThreadState) -> Vec<ExecutionFrame> {
    thread
        .call_stack
        .iter()
        .rev()
        .map(|frame| ExecutionFrame {
            module_id: frame.module_id.raw().into(),
            function_id: frame.func_idx.raw().into(),
            instruction_offset: frame.pc.saturating_sub(1) as u32,
        })
        .collect()
}

pub(crate) fn with_execution_stack(
    failure: ExecutionFailure,
    stack: Vec<ExecutionFrame>,
) -> ExecutionFailure {
    if failure.stack.is_empty() {
        failure.with_stack(stack)
    } else {
        failure
    }
}

fn panic_stack(panic: &galfus_vm::VmPanic) -> Vec<ExecutionFrame> {
    panic
        .stack_trace
        .iter()
        .map(|frame| ExecutionFrame {
            module_id: frame.module_id.raw().into(),
            function_id: frame.func_idx.raw().into(),
            instruction_offset: frame.instruction_offset as u32,
        })
        .collect()
}

impl RunnableTask for RuntimeTask {
    fn run(mut self: Box<Self>, budget: usize) -> ThreadResult {
        let mut thread = self.thread.take().unwrap();

        let step = match self.vm.execute_with_budget(&mut thread, budget) {
            Ok(step) => step,
            Err(e) => {
                let failure = with_initialization_context(
                    &thread,
                    ExecutionFailure::new(ExecutionFailureKind::VmPanic, e.to_string())
                        .with_thread_id(self.thread_id)
                        .with_stack(panic_stack(&e)),
                );
                let _ = self.events.submit(crate::event::RuntimeEvent::Failed {
                    thread_id: self.thread_id,
                    error: failure.clone(),
                });
                return ThreadResult::Discarded;
            }
        };

        match step {
            galfus_vm::VmStep::Continue => {
                let _ = self.events.submit(crate::event::RuntimeEvent::Yielded {
                    thread_id: self.thread_id,
                    thread,
                });
                ThreadResult::Discarded
            }
            galfus_vm::VmStep::Return {
                value,
                module_id,
                return_type,
            } => {
                if let Some(module_id) = thread.finish_module_initialization() {
                    let _ = self.events.submit(crate::event::RuntimeEvent::Initialized {
                        thread_id: self.thread_id,
                        thread,
                        module_id,
                    });
                    return ThreadResult::Completed(Ok(galfus_contract::BoundaryValue::I32(0)));
                }
                let module = match self.vm.graph.get(module_id) {
                    Some(node) => &node.module,
                    None => {
                        return ThreadResult::Completed(Err(ExecutionFailure::new(
                            ExecutionFailureKind::InternalRuntimeFailure,
                            format!("module {} missing during return decoding", module_id.raw()),
                        )));
                    }
                };
                let result = match decode_from_thread_heap(&thread.heap, value, return_type, module)
                {
                    Ok(value) => Ok(value),
                    Err(e) => Err(ExecutionFailure::new(
                        ExecutionFailureKind::BoundaryCodecFailure,
                        format!("failed to decode thread return value: {e:?}"),
                    )
                    .with_thread_id(self.thread_id)
                    .with_module_id(module_id.raw().into())
                    .with_stack(execution_stack(&thread))),
                };

                if let Some((owner_thread_id, future_lease)) = self.future_completion {
                    let _ = self
                        .events
                        .submit(crate::event::RuntimeEvent::FutureWorkerCompleted {
                            worker_thread_id: self.thread_id,
                            owner_thread_id,
                            future_lease,
                            thread,
                            result: result.clone(),
                        });
                } else {
                    let _ = self.events.submit(crate::event::RuntimeEvent::Exited {
                        thread_id: self.thread_id,
                        thread,
                        result: result.clone(),
                    });
                }
                ThreadResult::Completed(result)
            }
            galfus_vm::VmStep::Suspend {
                effect,
                mut continuation,
            } => {
                continuation.origin_thread_id = Some(self.thread_id);
                let _ = self.events.submit(crate::event::RuntimeEvent::Syscall {
                    thread_id: self.thread_id,
                    thread,
                    effect,
                    continuation,
                });
                ThreadResult::Blocked { timeout: None }
            }
            galfus_vm::VmStep::Failed(err) => {
                let err = with_initialization_context(
                    &thread,
                    with_execution_stack(
                        err.with_thread_id(self.thread_id),
                        execution_stack(&thread),
                    ),
                );
                let _ = self.events.submit(crate::event::RuntimeEvent::Failed {
                    thread_id: self.thread_id,
                    error: err.clone(),
                });
                ThreadResult::Discarded
            }
        }
    }

    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        Some(self)
    }
}
