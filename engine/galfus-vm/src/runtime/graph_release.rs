use crate::thread;

use super::*;

impl VirtualMachine {
    #[allow(clippy::collapsible_if)]
    pub fn release_unreachable(
        &self,
        thread: &mut thread::VmThreadState,
    ) -> Vec<(String, String, u64)> {
        use std::collections::{HashSet, VecDeque};

        thread.heap.allocations_since_release = 0;

        let mut roots = VecDeque::new();
        let mut reachable = HashSet::new();

        for state in thread.module_states.values() {
            for val in &state.globals {
                if let Value::Object(obj_ref) = val {
                    if reachable.insert(obj_ref.raw()) {
                        roots.push_back(*obj_ref);
                    }
                }
            }
        }

        for frame in &thread.call_stack {
            for val in &frame.registers {
                if let Value::Object(obj_ref) = val {
                    if reachable.insert(obj_ref.raw()) {
                        roots.push_back(*obj_ref);
                    }
                }
            }
        }

        while let Some(obj_ref) = roots.pop_front() {
            if let Some(Some(obj)) = thread.heap.objects.get(obj_ref.raw()) {
                match obj {
                    HeapObject::Struct {
                        module_id,
                        layout_idx,
                        fields,
                    } => {
                        if let Some(layout) = self
                            .graph
                            .get(*module_id)
                            .unwrap()
                            .module
                            .struct_layouts
                            .get(layout_idx.raw() as usize)
                        {
                            for (i, field_val) in fields.iter().enumerate() {
                                if let Value::Object(target_ref) = field_val {
                                    if let Some(field_layout) = layout.fields.get(i) {
                                        if field_layout.ownership != OwnershipKind::Weak {
                                            if reachable.insert(target_ref.raw()) {
                                                roots.push_back(*target_ref);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    HeapObject::Array { elements, .. } => {
                        for el in elements {
                            if let Value::Object(target_ref) = el {
                                if reachable.insert(target_ref.raw()) {
                                    roots.push_back(*target_ref);
                                }
                            }
                        }
                    }
                    HeapObject::Tuple { elements } => {
                        for el in elements {
                            if let Value::Object(target_ref) = el {
                                if reachable.insert(target_ref.raw()) {
                                    roots.push_back(*target_ref);
                                }
                            }
                        }
                    }
                    HeapObject::Choice { payload, .. } => {
                        if let Value::Object(target_ref) = payload {
                            if reachable.insert(target_ref.raw()) {
                                roots.push_back(*target_ref);
                            }
                        }
                    }
                    HeapObject::AdapterHandle { .. } => {}
                }
            }
        }

        let mut dead_objects = Vec::new();
        for idx in 0..thread.heap.objects.len() {
            if thread.heap.objects[idx].is_some() && !reachable.contains(&idx) {
                dead_objects.push(idx);
            }
        }

        if dead_objects.is_empty() {
            return Vec::new();
        }

        let released_handles: Vec<(String, String, u64)> = dead_objects
            .iter()
            .filter_map(|&idx| match thread.heap.objects[idx].as_ref() {
                Some(HeapObject::AdapterHandle {
                    proxy_module,
                    kind,
                    id,
                }) => Some((proxy_module.clone(), kind.clone(), *id)),
                _ => None,
            })
            .collect();

        for &idx in &dead_objects {
            thread.heap.objects[idx] = None;
            thread.heap.free_slots.push(idx);
        }

        for idx in 0..thread.heap.objects.len() {
            if let Some(Some(HeapObject::Struct {
                module_id,
                layout_idx,
                fields,
            })) = thread.heap.objects.get_mut(idx)
            {
                let layout_idx_val = *layout_idx;
                if let Some(layout) = self
                    .graph
                    .get(*module_id)
                    .unwrap()
                    .module
                    .struct_layouts
                    .get(layout_idx_val.raw() as usize)
                {
                    for (i, field_val) in fields.iter_mut().enumerate() {
                        if let Value::Object(target_ref) = field_val {
                            if dead_objects.contains(&target_ref.raw()) {
                                if let Some(field_layout) = layout.fields.get(i) {
                                    if field_layout.ownership == OwnershipKind::Weak {
                                        *field_val = Value::Null;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        released_handles
    }
}
