#[cfg(test)]
mod tests;

use crate::registry;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind, RunnableTask, ThreadResult};
use galfus_vm::VirtualMachine;
use galfus_vm::thread::VirtualThread;
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
            Ok(BoundaryValue::F32(value))
        }
        (BytecodeType::Float64, galfus_vm::VmValue::Float64(value)) => {
            Ok(BoundaryValue::F64(value))
        }
        (BytecodeType::ExternalHandle(kind), galfus_vm::VmValue::Object(reference)) => match heap
            .get_object(reference)
        {
            Ok(galfus_vm::HeapObject::ExternalHandle { kind: actual, id }) if actual == kind => {
                Ok(BoundaryValue::Handle {
                    kind: kind.clone(),
                    id: *id,
                })
            }
            _ => Err(mismatch()),
        },
        (BytecodeType::Array(element_type), galfus_vm::VmValue::Object(reference)) => {
            let galfus_vm::HeapObject::Array { elements, .. } = heap
                .get_object(reference)
                .map_err(|_| BoundaryCodecError::UnsupportedType)?
            else {
                return Err(mismatch());
            };
            let values = elements
                .iter()
                .cloned()
                .map(|element| decode_from_thread_heap(heap, element, *element_type, module))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BoundaryValue::Array {
                element_type: boundary_type(module, *element_type)?,
                values,
            })
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
                    decode_from_thread_heap(heap, payload.clone(), payload_type, module)
                        .map(Box::new)
                })
                .transpose()?;
            Ok(BoundaryValue::Choice {
                variant: *variant_idx as usize,
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
        (BytecodeType::Float32, BoundaryValue::F32(value)) => {
            Ok(galfus_vm::VmValue::Float32(value))
        }
        (BytecodeType::Float64, BoundaryValue::F64(value)) => {
            Ok(galfus_vm::VmValue::Float64(value))
        }
        (BytecodeType::ExternalHandle(kind), BoundaryValue::Handle { kind: actual, id })
            if kind == &actual =>
        {
            Ok(galfus_vm::VmValue::Object(heap.alloc(
                galfus_vm::HeapObject::ExternalHandle { kind: actual, id },
            )))
        }
        (BytecodeType::Array(element_type), BoundaryValue::Bytes(bytes))
            if matches!(
                module.types.get(element_type.raw() as usize),
                Some(BytecodeType::Uint8)
            ) =>
        {
            let elements = bytes.into_iter().map(galfus_vm::VmValue::Uint8).collect();
            let reference = heap.alloc(galfus_vm::HeapObject::Array {
                element_ty: *element_type,
                elements,
            });
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (BytecodeType::Array(element_type), BoundaryValue::Array { values, .. }) => {
            let elements = values
                .into_iter()
                .map(|element| {
                    encode_into_thread_heap(heap, element, *element_type, module_id, module)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap.alloc(galfus_vm::HeapObject::Array {
                element_ty: *element_type,
                elements,
            });
            Ok(galfus_vm::VmValue::Object(reference))
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
            let reference = heap.alloc(galfus_vm::HeapObject::Tuple { elements });
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (BytecodeType::Choice(layout_idx), BoundaryValue::Choice { variant, payload }) => {
            let layout = module
                .choice_layouts
                .get(layout_idx.raw() as usize)
                .ok_or(BoundaryCodecError::UnsupportedType)?;
            let variant_layout = layout.variants.get(variant).ok_or_else(mismatch)?;
            let payload = match (variant_layout.payload_ty, payload) {
                (None, None) => galfus_vm::VmValue::Null,
                (Some(payload_type), Some(payload)) => {
                    encode_into_thread_heap(heap, *payload, payload_type, module_id, module)?
                }
                _ => return Err(mismatch()),
            };
            let reference = heap.alloc(galfus_vm::HeapObject::Choice {
                module_id,
                layout_idx: *layout_idx,
                variant_idx: variant as u16,
                payload,
            });
            Ok(galfus_vm::VmValue::Object(reference))
        }
        _ => Err(mismatch()),
    }
}

pub(crate) fn boundary_type(
    module: &galfus_bytecode::BytecodeModule,
    type_index: galfus_bytecode::instruction::TypeIdx,
) -> Result<galfus_contract::BoundaryType, galfus_contract::BoundaryCodecError> {
    use galfus_bytecode::BytecodeType;
    use galfus_contract::{BoundaryCodecError, BoundaryType};

    match module.types.get(type_index.raw() as usize) {
        Some(BytecodeType::Null) => Ok(BoundaryType::Null),
        Some(BytecodeType::Bool) => Ok(BoundaryType::Bool),
        Some(BytecodeType::Int8) => Ok(BoundaryType::I8),
        Some(BytecodeType::Int16) => Ok(BoundaryType::I16),
        Some(BytecodeType::Int32) => Ok(BoundaryType::I32),
        Some(BytecodeType::Int64) => Ok(BoundaryType::I64),
        Some(BytecodeType::Uint8) => Ok(BoundaryType::U8),
        Some(BytecodeType::Uint16) => Ok(BoundaryType::U16),
        Some(BytecodeType::Uint32) => Ok(BoundaryType::U32),
        Some(BytecodeType::Uint64) => Ok(BoundaryType::U64),
        Some(BytecodeType::Float32) => Ok(BoundaryType::F32),
        Some(BytecodeType::Float64) => Ok(BoundaryType::F64),
        Some(BytecodeType::ExternalHandle(kind)) => Ok(BoundaryType::Handle { kind: kind.clone() }),
        Some(BytecodeType::Array(element)) => Ok(BoundaryType::Array(Box::new(boundary_type(
            module, *element,
        )?))),
        Some(BytecodeType::Tuple(elements)) => elements
            .iter()
            .copied()
            .map(|element| boundary_type(module, element))
            .collect::<Result<Vec<_>, _>>()
            .map(BoundaryType::Tuple),
        Some(BytecodeType::Choice(_)) => Err(BoundaryCodecError::UnsupportedType),
        _ => Err(BoundaryCodecError::UnsupportedType),
    }
}

pub struct RuntimeTask {
    pub thread_id: registry::ThreadId,
    pub thread: Option<VirtualThread>,
    pub vm: Arc<VirtualMachine>,
    pub events: crate::event::EventSink,
}

impl RuntimeTask {
    pub(crate) fn new(
        thread_id: registry::ThreadId,
        thread: VirtualThread,
        vm: Arc<VirtualMachine>,
        events: crate::event::EventSink,
    ) -> Self {
        Self {
            thread_id,
            thread: Some(thread),
            vm,
            events,
        }
    }
}

fn with_initialization_context(
    thread: &VirtualThread,
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

impl RunnableTask for RuntimeTask {
    fn run(mut self: Box<Self>, budget: usize) -> ThreadResult {
        let mut thread = self.thread.take().unwrap();

        let step = match self.vm.execute_with_budget(&mut thread, budget) {
            Ok(step) => step,
            Err(e) => {
                let failure = with_initialization_context(
                    &thread,
                    ExecutionFailure::new(ExecutionFailureKind::VmPanic, e.to_string())
                        .with_thread_id(self.thread_id.raw()),
                );
                self.events.send(crate::event::RuntimeEvent::Failed {
                    thread_id: self.thread_id,
                    error: failure.clone(),
                });
                return ThreadResult::Failed(failure);
            }
        };

        match step {
            galfus_vm::VmStep::Continue => {
                self.thread = Some(thread);
                ThreadResult::Yielded(self)
            }
            galfus_vm::VmStep::Return {
                value,
                module_id,
                return_type,
            } => {
                if let Some(module_id) = thread.finish_module_initialization() {
                    self.events.send(crate::event::RuntimeEvent::Initialized {
                        thread_id: self.thread_id,
                        thread,
                        module_id,
                    });
                    return ThreadResult::Completed(0);
                }
                let module = &self
                    .vm
                    .graph
                    .get(module_id)
                    .expect("returned module is loaded")
                    .module;
                let result = match decode_from_thread_heap(&thread.heap, value, return_type, module)
                {
                    Ok(galfus_contract::BoundaryValue::I32(code)) => code,
                    Ok(value) => {
                        let failure = ExecutionFailure::new(
                            ExecutionFailureKind::BoundaryCodecFailure,
                            format!("entry result must be i32, found {value:?}"),
                        )
                        .with_thread_id(self.thread_id.raw())
                        .with_module_id(module_id.raw().into());
                        self.events.send(crate::event::RuntimeEvent::Failed {
                            thread_id: self.thread_id,
                            error: failure.clone(),
                        });
                        return ThreadResult::Failed(failure);
                    }
                    Err(error) => {
                        let failure = ExecutionFailure::new(
                            ExecutionFailureKind::BoundaryCodecFailure,
                            format!("invalid entry result: {error:?}"),
                        )
                        .with_thread_id(self.thread_id.raw())
                        .with_module_id(module_id.raw().into());
                        self.events.send(crate::event::RuntimeEvent::Failed {
                            thread_id: self.thread_id,
                            error: failure.clone(),
                        });
                        return ThreadResult::Failed(failure);
                    }
                };
                self.events.send(crate::event::RuntimeEvent::Exited {
                    thread_id: self.thread_id,
                    thread,
                    code: result,
                });
                ThreadResult::Completed(result)
            }
            galfus_vm::VmStep::Suspend {
                effect,
                continuation,
            } => {
                self.events.send(crate::event::RuntimeEvent::Syscall {
                    thread_id: self.thread_id,
                    thread,
                    effect,
                    continuation,
                });
                ThreadResult::Blocked { timeout: None }
            }
            galfus_vm::VmStep::Failed(err) => {
                let err =
                    with_initialization_context(&thread, err.with_thread_id(self.thread_id.raw()));
                self.events.send(crate::event::RuntimeEvent::Failed {
                    thread_id: self.thread_id,
                    error: err.clone(),
                });
                ThreadResult::Failed(err)
            }
        }
    }

    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        Some(self)
    }
}

pub(crate) fn thread_key(thread: &VirtualThread, value: galfus_vm::VmValue) -> Option<String> {
    match value {
        galfus_vm::VmValue::Object(r) => match thread.heap.get_object(r).ok() {
            Some(galfus_vm::HeapObject::Array { elements, .. }) => {
                let mut s = String::new();
                for e in elements {
                    if let galfus_vm::VmValue::Uint8(b) = e {
                        s.push(*b as char);
                    }
                }
                Some(s)
            }
            _ => None,
        },
        _ => None,
    }
}
