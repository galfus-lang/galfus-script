use crate::thread;

use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

impl VirtualMachine {
    pub(super) fn execute_object_instruction(
        &self,
        thread: &mut thread::VmThreadState,
        instr: Instruction,
    ) -> Result<VmStep, VmError> {
        match instr {
            // Category D: Heaps, Structs & Collections
            Instruction::AllocLocal { dest, type_idx } => {
                let ty = self
                    .current_image(thread)?
                    .types
                    .get(type_idx.raw() as usize)
                    .ok_or(VmError::TypeOutOfBounds { index: type_idx })?;
                if let BytecodeType::Struct(layout_idx) = ty {
                    let layout = self
                        .current_image(thread)?
                        .struct_layouts
                        .get(layout_idx.raw() as usize)
                        .ok_or(VmError::TypeOutOfBounds { index: type_idx })?;
                    let fields = vec![Value::Null; layout.fields.len()];
                    let obj_ref = thread.heap.alloc(HeapObject::Struct {
                        module_id: thread
                            .call_stack
                            .last()
                            .ok_or(VmError::EmptyCallStack)?
                            .module_id,
                        layout_idx: *layout_idx,
                        fields,
                    })?;
                    thread.write_reg(dest, Value::Object(obj_ref))?;
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Struct type".to_string(),
                        found: format!("{:?}", ty),
                    });
                }
            }
            Instruction::LoadField { dest, obj, field } => {
                let obj_val = thread.read_reg(obj)?;
                if let Value::Object(obj_ref) = obj_val {
                    let heap_obj = thread.heap.get_object(obj_ref)?;
                    if let HeapObject::Struct { fields, .. } = heap_obj {
                        let val = fields
                            .get(field.raw() as usize)
                            .cloned()
                            .ok_or(VmError::FieldOutOfBounds { index: field })?;
                        thread.write_reg(dest, val)?;
                    } else if let HeapObject::Tuple { elements } = heap_obj {
                        let val = elements
                            .get(field.raw() as usize)
                            .cloned()
                            .ok_or(VmError::FieldOutOfBounds { index: field })?;
                        thread.write_reg(dest, val)?;
                    } else if let HeapObject::Choice { payload, .. } = heap_obj {
                        thread.write_reg(dest, payload.clone())?;
                    } else {
                        return Err(VmError::TypeMismatch {
                            expected: "Struct or Choice object".to_string(),
                            found: format!("{:?}", heap_obj),
                        });
                    }
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Object reference".to_string(),
                        found: format!("{:?}", obj_val),
                    });
                }
            }
            Instruction::StoreField { obj, field, val } => {
                let obj_val = thread.read_reg(obj)?;
                let val_to_store = thread.read_reg(val)?;
                if let Value::Object(obj_ref) = obj_val {
                    let heap_obj = thread.heap.get_object_mut(obj_ref)?;
                    if let HeapObject::Struct { fields, .. } = heap_obj {
                        if (field.raw() as usize) < fields.len() {
                            fields[field.raw() as usize] = val_to_store;
                        } else {
                            return Err(VmError::FieldOutOfBounds { index: field });
                        }
                    } else {
                        return Err(VmError::TypeMismatch {
                            expected: "Struct object".to_string(),
                            found: format!("{:?}", heap_obj),
                        });
                    }
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Object reference".to_string(),
                        found: format!("{:?}", obj_val),
                    });
                }
            }
            Instruction::NewArray {
                dest,
                type_idx,
                len_reg,
            } => {
                let ty = self
                    .current_image(thread)?
                    .types
                    .get(type_idx.raw() as usize)
                    .ok_or(VmError::TypeOutOfBounds { index: type_idx })?;
                let element_ty = match ty {
                    BytecodeType::Array(el_ty) => *el_ty,
                    _ => {
                        return Err(VmError::TypeMismatch {
                            expected: "Array type".to_string(),
                            found: format!("{:?}", ty),
                        });
                    }
                };
                let len_val = thread.read_reg(len_reg)?;
                let len = match len_val {
                    Value::Int8(x) if x >= 0 => x as usize,
                    Value::Int16(x) if x >= 0 => x as usize,
                    Value::Int32(x) if x >= 0 => x as usize,
                    Value::Int64(x) if x >= 0 => x as usize,
                    Value::Uint8(x) => x as usize,
                    Value::Uint16(x) => x as usize,
                    Value::Uint32(x) => x as usize,
                    Value::Uint64(x) => x as usize,
                    x => {
                        return Err(VmError::TypeMismatch {
                            expected: "positive array length".to_string(),
                            found: format!("{:?}", x),
                        });
                    }
                };
                let zero = self.zero_value_for_type(thread, element_ty)?;
                let elements = vec![zero; len];
                let obj_ref = thread.heap.alloc(HeapObject::Array {
                    module_id: thread
                        .call_stack
                        .last()
                        .ok_or(VmError::EmptyCallStack)?
                        .module_id,
                    element_ty,
                    elements,
                })?;
                thread.write_reg(dest, Value::Object(obj_ref))?;
            }
            Instruction::LoadIndex { dest, arr, idx } => {
                let arr_val = thread.read_reg(arr)?;
                let idx_val = thread.read_reg(idx)?;
                let raw_index = self.to_raw_array_index(idx_val)?;

                if let Value::Object(obj_ref) = arr_val {
                    let heap_obj = thread.heap.get_object(obj_ref)?;
                    let val = match heap_obj {
                        HeapObject::Array { elements, .. } | HeapObject::Tuple { elements } => {
                            if let Some(index) =
                                self.resolve_raw_array_index(raw_index, elements.len())
                            {
                                elements[index].clone()
                            } else {
                                Value::Null
                            }
                        }
                        _ => {
                            return Err(VmError::TypeMismatch {
                                expected: "Array or Tuple object".to_string(),
                                found: format!("{:?}", heap_obj),
                            });
                        }
                    };

                    thread.write_reg(dest, val)?;
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Object reference".to_string(),
                        found: format!("{:?}", arr_val),
                    });
                }
            }
            Instruction::StoreIndex { arr, idx, val } => {
                let arr_val = thread.read_reg(arr)?;
                let idx_val = thread.read_reg(idx)?;
                let raw_index = self.to_raw_array_index(idx_val)?;
                let val_to_store = thread.read_reg(val)?;

                if let Value::Object(obj_ref) = arr_val {
                    let (index, val_to_store) = {
                        let heap_obj = thread.heap.get_object(obj_ref)?;

                        match heap_obj {
                            HeapObject::Array {
                                element_ty,
                                elements,
                                ..
                            } => {
                                let index = self
                                    .resolve_raw_array_index(raw_index, elements.len())
                                    .ok_or(VmError::IndexOutOfBounds {
                                        index: raw_index,
                                        len: elements.len(),
                                    })?;

                                let val_to_store =
                                    self.execute_cast(thread, &val_to_store, *element_ty)?;
                                (index, val_to_store)
                            }
                            HeapObject::Tuple { elements } => {
                                let index = self
                                    .resolve_raw_array_index(raw_index, elements.len())
                                    .ok_or(VmError::IndexOutOfBounds {
                                        index: raw_index,
                                        len: elements.len(),
                                    })?;

                                (index, val_to_store)
                            }
                            heap_obj => {
                                return Err(VmError::TypeMismatch {
                                    expected: "Array or Tuple object".to_string(),
                                    found: format!("{:?}", heap_obj),
                                });
                            }
                        }
                    };

                    let heap_obj = thread.heap.get_object_mut(obj_ref)?;

                    match heap_obj {
                        HeapObject::Array { elements, .. } | HeapObject::Tuple { elements } => {
                            elements[index] = val_to_store;
                        }
                        heap_obj => {
                            return Err(VmError::TypeMismatch {
                                expected: "Array or Tuple object".to_string(),
                                found: format!("{:?}", heap_obj),
                            });
                        }
                    }
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Object reference".to_string(),
                        found: format!("{:?}", arr_val),
                    });
                }
            }
            Instruction::NewTuple {
                dest,
                type_idx,
                start,
                count,
            } => {
                let ty = self
                    .current_image(thread)?
                    .types
                    .get(type_idx.raw() as usize)
                    .ok_or(VmError::TypeOutOfBounds { index: type_idx })?;
                if let BytecodeType::Tuple(tys) = ty {
                    if count as usize != tys.len() {
                        return Err(VmError::TypeMismatch {
                            expected: format!("Tuple size {}", tys.len()),
                            found: format!("Tuple size {}", count),
                        });
                    }
                    let mut elements = Vec::new();
                    for i in 0..count as usize {
                        let src_reg = Reg(start.raw() + i as u16);
                        elements.push(thread.read_reg(src_reg)?);
                    }
                    let obj_ref = thread.heap.alloc(HeapObject::Tuple { elements })?;
                    thread.write_reg(dest, Value::Object(obj_ref))?;
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Tuple type".to_string(),
                        found: format!("{:?}", ty),
                    });
                }
            }
            Instruction::NewChoice {
                dest,
                type_idx,
                variant_idx,
                payload,
            } => {
                let ty = self
                    .current_image(thread)?
                    .types
                    .get(type_idx.raw() as usize)
                    .ok_or(VmError::TypeOutOfBounds { index: type_idx })?;
                if let BytecodeType::Choice(layout_idx) = ty {
                    let payload_val = thread.read_reg(payload)?;
                    let obj_ref = thread.heap.alloc(HeapObject::Choice {
                        module_id: thread
                            .call_stack
                            .last()
                            .ok_or(VmError::EmptyCallStack)?
                            .module_id,
                        layout_idx: *layout_idx,
                        variant_idx,
                        payload: payload_val,
                    })?;
                    thread.write_reg(dest, Value::Object(obj_ref))?;
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Choice type".to_string(),
                        found: format!("{:?}", ty),
                    });
                }
            }
            Instruction::Cast {
                dest,
                src,
                type_idx,
            } => {
                let val = thread.read_reg(src)?;
                let res = self.execute_cast(thread, &val, type_idx)?;
                thread.write_reg(dest, res)?;
            }
            Instruction::Copy { dest, src } => {
                let val = thread.read_reg(src)?;
                let copied = self.deep_copy_value(thread, &val)?;
                thread.write_reg(dest, copied)?;
            }
            Instruction::Instanceof {
                dest,
                src,
                type_idx,
            } => {
                let val = thread.read_reg(src)?;
                let is_instance = self.check_value_type(thread, &val, type_idx);
                thread.write_reg(dest, Value::Bool(is_instance))?;
            }

            _ => unreachable!("instruction routed to the wrong runtime handler"),
        }

        Ok(VmStep::Continue)
    }

    fn deep_copy_value(
        &self,
        thread: &mut thread::VmThreadState,
        value: &Value,
    ) -> Result<Value, VmError> {
        let Value::Object(obj_ref) = value else {
            return Ok(value.clone());
        };

        let strong_closure = self.discover_strong_copy_closure(thread, *obj_ref)?;
        let copied = self.allocate_copy_placeholders(thread, &strong_closure)?;
        self.fill_copy_placeholders(thread, &strong_closure, &copied)?;

        copied
            .get(obj_ref)
            .copied()
            .map(Value::Object)
            .ok_or_else(|| VmError::TypeMismatch {
                expected: "copied root object".to_string(),
                found: format!("{:?}", obj_ref),
            })
    }

    fn discover_strong_copy_closure(
        &self,
        thread: &thread::VmThreadState,
        root_ref: ObjectRef,
    ) -> Result<HashSet<VmObjectRef>, VmError> {
        let mut closure = HashSet::new();
        let mut pending = VecDeque::new();

        closure.insert(root_ref);
        pending.push_back(root_ref);

        while let Some(obj_ref) = pending.pop_front() {
            let object = thread.heap.get_object(obj_ref)?;

            match object {
                HeapObject::Struct {
                    module_id,
                    layout_idx,
                    fields,
                } => {
                    let layout = self
                        .get_module(*module_id)?
                        .struct_layouts
                        .get(layout_idx.raw() as usize)
                        .ok_or(VmError::TypeMismatch {
                            expected: "valid struct layout".to_string(),
                            found: format!("{:?}", layout_idx),
                        })?;

                    for (index, field) in fields.iter().enumerate() {
                        let is_weak = layout.fields.get(index).is_some_and(|field_layout| {
                            field_layout.ownership == OwnershipKind::Weak
                        });

                        if !is_weak {
                            self.enqueue_copy_target(thread, field, &mut closure, &mut pending)?;
                        }
                    }
                }
                HeapObject::Array { elements, .. } | HeapObject::Tuple { elements } => {
                    for element in elements {
                        self.enqueue_copy_target(thread, element, &mut closure, &mut pending)?;
                    }
                }
                HeapObject::Choice { payload, .. } => {
                    self.enqueue_copy_target(thread, payload, &mut closure, &mut pending)?;
                }
                HeapObject::AdapterHandle { .. } => {}
            }
        }

        Ok(closure)
    }

    fn enqueue_copy_target(
        &self,
        thread: &thread::VmThreadState,
        value: &Value,
        closure: &mut HashSet<VmObjectRef>,
        pending: &mut VecDeque<ObjectRef>,
    ) -> Result<(), VmError> {
        if let Value::Object(obj_ref) = value {
            thread.heap.get_object(*obj_ref)?;

            if closure.insert(*obj_ref) {
                pending.push_back(*obj_ref);
            }
        }

        Ok(())
    }

    fn allocate_copy_placeholders(
        &self,
        thread: &mut thread::VmThreadState,
        strong_closure: &HashSet<VmObjectRef>,
    ) -> Result<HashMap<VmObjectRef, ObjectRef>, VmError> {
        let mut copied = HashMap::new();

        for &old_ref in strong_closure {
            let object = thread.heap.get_object(old_ref)?.clone();

            let placeholder = match object {
                HeapObject::Struct {
                    module_id,
                    layout_idx,
                    fields,
                } => {
                    let layout = self
                        .get_module(module_id)?
                        .struct_layouts
                        .get(layout_idx.raw() as usize)
                        .ok_or(VmError::TypeMismatch {
                            expected: "valid struct layout".to_string(),
                            found: format!("{:?}", layout_idx),
                        })?;

                    if layout.fields.is_empty() {
                        return Err(VmError::TypeMismatch {
                            expected: "copyable struct with fields".to_string(),
                            found: format!("fieldless struct `{}`", layout.name),
                        });
                    }

                    HeapObject::Struct {
                        module_id,
                        layout_idx,
                        fields: vec![Value::Null; fields.len()],
                    }
                }
                HeapObject::Array {
                    module_id,
                    element_ty,
                    ..
                } => HeapObject::Array {
                    module_id,
                    element_ty,
                    elements: Vec::new(),
                },
                HeapObject::Tuple { .. } => HeapObject::Tuple {
                    elements: Vec::new(),
                },
                HeapObject::Choice {
                    module_id,
                    layout_idx,
                    variant_idx,
                    ..
                } => HeapObject::Choice {
                    module_id,
                    layout_idx,
                    variant_idx,
                    payload: Value::Null,
                },
                HeapObject::AdapterHandle {
                    binding_id,
                    type_id,
                    id,
                } => HeapObject::AdapterHandle {
                    binding_id,
                    type_id,
                    id,
                },
            };

            let copied_ref = thread.heap.alloc(placeholder)?;
            copied.insert(old_ref, copied_ref);
        }

        Ok(copied)
    }

    fn fill_copy_placeholders(
        &self,
        thread: &mut thread::VmThreadState,
        strong_closure: &HashSet<VmObjectRef>,
        copied: &HashMap<VmObjectRef, ObjectRef>,
    ) -> Result<(), VmError> {
        for &old_ref in strong_closure {
            let copied_ref = *copied.get(&old_ref).ok_or_else(|| VmError::TypeMismatch {
                expected: "copied object".to_string(),
                found: format!("{:?}", old_ref),
            })?;
            let object = thread.heap.get_object(old_ref)?.clone();

            match object {
                HeapObject::Struct {
                    module_id,
                    layout_idx,
                    fields,
                } => {
                    let layout = self
                        .get_module(module_id)?
                        .struct_layouts
                        .get(layout_idx.raw() as usize)
                        .ok_or(VmError::TypeMismatch {
                            expected: "valid struct layout".to_string(),
                            found: format!("{:?}", layout_idx),
                        })?
                        .clone();

                    let copied_fields = fields
                        .iter()
                        .enumerate()
                        .map(|(index, field)| {
                            let is_weak = layout
                                .fields
                                .get(index)
                                .is_some_and(|field| field.ownership == OwnershipKind::Weak);

                            if is_weak {
                                Ok(self.copy_weak_value(field, copied))
                            } else {
                                self.copy_strong_value(thread, field, copied)
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    match thread.heap.get_object_mut(copied_ref)? {
                        HeapObject::Struct { fields, .. } => {
                            *fields = copied_fields;
                        }
                        other => {
                            return Err(VmError::TypeMismatch {
                                expected: "Struct object".to_string(),
                                found: format!("{:?}", other),
                            });
                        }
                    }
                }
                HeapObject::Array { elements, .. } => {
                    let copied_elements = elements
                        .iter()
                        .map(|element| self.copy_strong_value(thread, element, copied))
                        .collect::<Result<Vec<_>, _>>()?;

                    match thread.heap.get_object_mut(copied_ref)? {
                        HeapObject::Array { elements, .. } => {
                            *elements = copied_elements;
                        }
                        other => {
                            return Err(VmError::TypeMismatch {
                                expected: "Array object".to_string(),
                                found: format!("{:?}", other),
                            });
                        }
                    }
                }
                HeapObject::Tuple { elements } => {
                    let copied_elements = elements
                        .iter()
                        .map(|element| self.copy_strong_value(thread, element, copied))
                        .collect::<Result<Vec<_>, _>>()?;

                    match thread.heap.get_object_mut(copied_ref)? {
                        HeapObject::Tuple { elements } => {
                            *elements = copied_elements;
                        }
                        other => {
                            return Err(VmError::TypeMismatch {
                                expected: "Tuple object".to_string(),
                                found: format!("{:?}", other),
                            });
                        }
                    }
                }
                HeapObject::Choice { payload, .. } => {
                    let copied_payload = self.copy_strong_value(thread, &payload, copied)?;

                    match thread.heap.get_object_mut(copied_ref)? {
                        HeapObject::Choice { payload, .. } => {
                            *payload = copied_payload;
                        }
                        other => {
                            return Err(VmError::TypeMismatch {
                                expected: "Choice object".to_string(),
                                found: format!("{:?}", other),
                            });
                        }
                    }
                }
                HeapObject::AdapterHandle { .. } => {}
            }
        }

        Ok(())
    }

    fn copy_strong_value(
        &self,
        _thread: &thread::VmThreadState,
        value: &Value,
        copied: &HashMap<VmObjectRef, ObjectRef>,
    ) -> Result<Value, VmError> {
        let Value::Object(obj_ref) = value else {
            return Ok(value.clone());
        };

        if let Some(copied_ref) = copied.get(&obj_ref) {
            return Ok(Value::Object(*copied_ref));
        }

        Err(VmError::TypeMismatch {
            expected: "object in copied strong closure".to_string(),
            found: format!("{:?}", obj_ref),
        })
    }

    fn copy_weak_value(&self, value: &Value, copied: &HashMap<VmObjectRef, ObjectRef>) -> Value {
        let Value::Object(obj_ref) = value else {
            return value.clone();
        };

        copied
            .get(&obj_ref)
            .copied()
            .map(Value::Object)
            .unwrap_or(Value::Null)
    }

    /// Returns the default `Value` for element types that can be safely default-initialized.
    fn zero_value_for_type(
        &self,
        thread: &thread::VmThreadState,
        type_idx: TypeIdx,
    ) -> Result<Value, VmError> {
        let ty = self
            .current_image(thread)?
            .types
            .get(type_idx.raw() as usize)
            .ok_or(VmError::TypeOutOfBounds { index: type_idx })?;

        Ok(match ty {
            BytecodeType::Bool => Value::Bool(false),
            BytecodeType::Int8 => Value::Int8(0),
            BytecodeType::Int16 => Value::Int16(0),
            BytecodeType::Int32 => Value::Int32(0),
            BytecodeType::Int64 => Value::Int64(0),
            BytecodeType::Uint8 => Value::Uint8(0),
            BytecodeType::Uint16 => Value::Uint16(0),
            BytecodeType::Uint32 => Value::Uint32(0),
            BytecodeType::Uint64 => Value::Uint64(0),
            BytecodeType::Float32 => Value::Float32(0.0),
            BytecodeType::Float64 => Value::Float64(0.0),
            BytecodeType::Null => Value::Null,
            BytecodeType::Nullable(_) => Value::Null,
            BytecodeType::Struct(_)
            | BytecodeType::Choice(_)
            | BytecodeType::Array(_)
            | BytecodeType::Tuple(_) => Value::Null,
            _ => {
                return Err(VmError::TypeMismatch {
                    expected: "defaultable array element type".to_string(),
                    found: format!("{:?}", ty),
                });
            }
        })
    }
}
