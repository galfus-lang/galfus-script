#[cfg(test)]
mod tests;

use crate::registry;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind, RunnableTask, ThreadResult};
use galfus_vm::VirtualMachine;
use galfus_vm::thread::VirtualThread;
use std::sync::Arc;

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

impl RunnableTask for RuntimeTask {
    fn run(mut self: Box<Self>, budget: usize) -> ThreadResult {
        let mut thread = self.thread.take().unwrap();

        let step = match self.vm.execute_with_budget(&mut thread, budget) {
            Ok(step) => step,
            Err(e) => {
                self.events.send(crate::event::RuntimeEvent::Failed {
                    thread_id: self.thread_id,
                    error: ExecutionFailure::new(ExecutionFailureKind::VmPanic, e.to_string()),
                });
                return ThreadResult::Failed(ExecutionFailure::new(
                    ExecutionFailureKind::VmPanic,
                    e.to_string(),
                ));
            }
        };

        match step {
            galfus_vm::VmStep::Continue => {
                self.thread = Some(thread);
                ThreadResult::Yielded(self)
            }
            galfus_vm::VmStep::Return(val) => {
                let code = match val {
                    galfus_vm::VmValue::Int32(c) => c,
                    galfus_vm::VmValue::Null => 0,
                    _ => 1,
                };
                self.events.send(crate::event::RuntimeEvent::Exited {
                    thread_id: self.thread_id,
                    thread,
                    code,
                });
                ThreadResult::Completed(code)
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
                self.events.send(crate::event::RuntimeEvent::Failed {
                    thread_id: self.thread_id,
                    error: err.clone(),
                });
                ThreadResult::Failed(err)
            }
        }
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

pub(crate) fn to_boundary_value(
    heap: &galfus_vm::thread::PrivateHeap,
    val: galfus_vm::VmValue,
) -> Option<galfus_contract::BoundaryValue> {
    match val {
        galfus_vm::VmValue::Null => Some(galfus_contract::BoundaryValue::Null),
        galfus_vm::VmValue::Bool(b) => Some(galfus_contract::BoundaryValue::Bool(b)),
        galfus_vm::VmValue::Int32(i) => Some(galfus_contract::BoundaryValue::I32(i)),
        galfus_vm::VmValue::Int64(i) => Some(galfus_contract::BoundaryValue::I64(i)),
        galfus_vm::VmValue::Float32(f) => Some(galfus_contract::BoundaryValue::F32(f)),
        galfus_vm::VmValue::Float64(f) => Some(galfus_contract::BoundaryValue::F64(f)),
        galfus_vm::VmValue::Uint8(u) => Some(galfus_contract::BoundaryValue::U8(u)),
        galfus_vm::VmValue::Object(r) => match heap.get_object(r).ok() {
            Some(galfus_vm::HeapObject::Array { elements, .. }) => {
                if elements
                    .iter()
                    .all(|value| matches!(value, galfus_vm::VmValue::Uint8(_)))
                {
                    return Some(galfus_contract::BoundaryValue::Bytes(
                        elements
                            .iter()
                            .filter_map(|value| match value {
                                galfus_vm::VmValue::Uint8(value) => Some(*value),
                                _ => None,
                            })
                            .collect(),
                    ));
                }
                let mut mapped = Vec::new();
                for e in elements {
                    if let Some(v) = to_boundary_value(heap, e.clone()) {
                        mapped.push(v);
                    } else {
                        return None;
                    }
                }
                Some(galfus_contract::BoundaryValue::Array {
                    element_type: galfus_contract::BoundaryType::U8,
                    values: mapped,
                })
            }
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn from_boundary_value(
    heap: &mut galfus_vm::thread::PrivateHeap,
    val: galfus_contract::BoundaryValue,
    vm: &galfus_vm::VirtualMachine,
) -> galfus_vm::VmValue {
    match val {
        galfus_contract::BoundaryValue::Null => galfus_vm::VmValue::Null,
        galfus_contract::BoundaryValue::Bool(b) => galfus_vm::VmValue::Bool(b),
        galfus_contract::BoundaryValue::I32(i) => galfus_vm::VmValue::Int32(i),
        galfus_contract::BoundaryValue::I64(i) => galfus_vm::VmValue::Int64(i),
        galfus_contract::BoundaryValue::F32(f) => galfus_vm::VmValue::Float32(f),
        galfus_contract::BoundaryValue::F64(f) => galfus_vm::VmValue::Float64(f),
        galfus_contract::BoundaryValue::U8(u) => galfus_vm::VmValue::Uint8(u),
        galfus_contract::BoundaryValue::Bytes(bytes) => {
            let elements = bytes.into_iter().map(galfus_vm::VmValue::Uint8).collect();
            let obj = heap.alloc(galfus_vm::HeapObject::Array {
                element_ty: galfus_bytecode::instruction::TypeIdx(0),
                elements,
            });
            galfus_vm::VmValue::Object(obj)
        }
        galfus_contract::BoundaryValue::Array { values, .. } => {
            let mut elements = Vec::new();
            for e in values {
                elements.push(from_boundary_value(heap, e, vm));
            }
            let obj = heap.alloc(galfus_vm::HeapObject::Array {
                element_ty: galfus_bytecode::instruction::TypeIdx(0),
                elements,
            });
            galfus_vm::VmValue::Object(obj)
        }
        _ => galfus_vm::VmValue::Null,
    }
}
