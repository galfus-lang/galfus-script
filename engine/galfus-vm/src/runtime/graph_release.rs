use crate::thread;

use super::*;

impl VirtualMachine {
    #[allow(clippy::collapsible_if)]
    pub fn release_unreachable(
        &self,
        thread: &mut thread::VmThreadState,
    ) -> Result<
        Vec<(
            galfus_core::BindingId,
            galfus_core::OpaqueTypeId,
            galfus_core::HandleId,
        )>,
        VmError,
    > {
        use std::collections::{HashSet, VecDeque};

        let mut roots = VecDeque::new();
        let mut reachable = HashSet::new();

        thread.visit_roots(&mut |obj_ref| {
            if reachable.insert(obj_ref) {
                roots.push_back(obj_ref);
            }
        });

        while let Some(obj_ref) = roots.pop_front() {
            let obj = thread.heap.get_object(obj_ref)?;
            match obj {
                HeapObject::Struct {
                    module_id,
                    layout_idx,
                    fields,
                } => {
                    let layout = self
                        .get_module(*module_id)?
                        .struct_layouts
                        .get(layout_idx.raw() as usize)
                        .ok_or(VmError::StructLayoutOutOfBounds { index: *layout_idx })?;

                    for (i, field_val) in fields.iter().enumerate() {
                        if let Value::Object(target_ref) = field_val {
                            let field_layout =
                                layout.fields.get(i).ok_or(VmError::FieldOutOfBounds {
                                    index: galfus_bytecode::instruction::FieldIdx(i as u16),
                                })?;
                            if field_layout.ownership != OwnershipKind::Weak {
                                if reachable.insert(*target_ref) {
                                    roots.push_back(*target_ref);
                                }
                            }
                        }
                    }
                }
                HeapObject::Array { elements, .. } => {
                    for el in elements {
                        if let Value::Object(target_ref) = el {
                            if reachable.insert(*target_ref) {
                                roots.push_back(*target_ref);
                            }
                        }
                    }
                }
                HeapObject::Tuple { elements } => {
                    for el in elements {
                        if let Value::Object(target_ref) = el {
                            if reachable.insert(*target_ref) {
                                roots.push_back(*target_ref);
                            }
                        }
                    }
                }
                HeapObject::Choice { payload, .. } => {
                    if let Value::Object(target_ref) = payload {
                        if reachable.insert(*target_ref) {
                            roots.push_back(*target_ref);
                        }
                    }
                }
                HeapObject::AdapterHandle { .. } => {}
            }
        }

        let mut dead_objects_vec = Vec::new();
        let mut dead_objects_set = HashSet::new();

        for (obj_ref, _) in thread.heap.iter_live_objects() {
            if !reachable.contains(&obj_ref) {
                dead_objects_vec.push(obj_ref);
                dead_objects_set.insert(obj_ref);
            }
        }

        if dead_objects_vec.is_empty() {
            thread.heap.reset_allocations_since_release();
            return Ok(Vec::new());
        }

        for (_, obj) in thread.heap.iter_live_objects_mut() {
            if let HeapObject::Struct {
                module_id,
                layout_idx,
                fields,
            } = obj
            {
                let layout = self
                    .get_module(*module_id)?
                    .struct_layouts
                    .get(layout_idx.raw() as usize)
                    .ok_or(VmError::StructLayoutOutOfBounds { index: *layout_idx })?;
                for (i, field_val) in fields.iter_mut().enumerate() {
                    if let Value::Object(target_ref) = field_val {
                        if dead_objects_set.contains(target_ref) {
                            let field_layout =
                                layout.fields.get(i).ok_or(VmError::FieldOutOfBounds {
                                    index: galfus_bytecode::instruction::FieldIdx(i as u16),
                                })?;
                            if field_layout.ownership == OwnershipKind::Weak {
                                *field_val = Value::Null;
                            }
                        }
                    }
                }
            }
        }

        let mut released_handles = Vec::new();
        for &obj_ref in &dead_objects_vec {
            if let Ok(HeapObject::AdapterHandle {
                binding_id,
                type_id,
                id,
            }) = thread.heap.get_object(obj_ref)
            {
                released_handles.push((*binding_id, type_id.clone(), *id));
            }
            thread.heap.free_object(obj_ref)?;
        }

        thread.heap.reset_allocations_since_release();
        Ok(released_handles)
    }
}
