use crate::error::VmError;
use crate::runtime::{HeapObject, VmObjectRef, VmValue};
use std::collections::HashSet;

pub struct HeapSlot {
    pub generation: u32,
    pub anchors: u32,
    pub edges: u32,
    pub heap_bytes: usize,
    pub object: Option<HeapObject>,
}

pub struct PrivateHeap {
    objects: Vec<HeapSlot>,
    free_slots: Vec<usize>,
    pub pending_adapter_handle_drops: Vec<(
        galfus_core::BindingId,
        galfus_core::OpaqueTypeId,
        galfus_core::HandleId,
    )>,
    pub(crate) quota: std::sync::Arc<crate::quota::ThreadQuota>,
}

impl PrivateHeap {
    pub fn test_new() -> Self {
        let limits = galfus_contract::LimitsMetadata::default();
        Self::new(std::sync::Arc::new(crate::quota::ThreadQuota::new(limits)))
    }

    pub fn new(quota: std::sync::Arc<crate::quota::ThreadQuota>) -> Self {
        Self {
            objects: Vec::new(),
            free_slots: Vec::new(),
            pending_adapter_handle_drops: Vec::new(),
            quota,
        }
    }

    pub fn alloc(&mut self, obj: HeapObject) -> Result<VmObjectRef, VmError> {
        if self.free_slots.is_empty() && self.objects.len() >= u32::MAX as usize {
            return Err(VmError::IdCounterExhausted);
        }

        let heap_bytes = obj.heap_bytes();
        self.quota
            .try_reserve_heap(1, heap_bytes)
            .map_err(VmError::ResourceLimitExceeded)?;

        let (idx, generation) = if let Some(idx) = self.free_slots.pop() {
            let slot = &mut self.objects[idx];
            slot.generation += 1;
            slot.anchors = 1;
            slot.edges = 0;
            slot.heap_bytes = heap_bytes;
            slot.object = Some(obj);
            (idx, slot.generation)
        } else {
            let idx = self.objects.len();
            let generation = 1;
            self.objects.push(HeapSlot {
                generation,
                anchors: 1,
                edges: 0,
                heap_bytes,
                object: Some(obj),
            });
            (idx, generation)
        };

        Ok(VmObjectRef::new(idx as u32, generation))
    }

    #[cfg(test)]
    pub fn exhaust_id_counter(&mut self) {}

    pub fn get_object(&self, obj_ref: VmObjectRef) -> Result<&HeapObject, VmError> {
        let slot = self
            .objects
            .get(obj_ref.index as usize)
            .ok_or(VmError::InvalidObjectReference)?;
        if slot.generation == obj_ref.generation
            && let Some(ref obj) = slot.object
        {
            return Ok(obj);
        }
        Err(VmError::InvalidObjectReference)
    }

    pub fn get_object_mut(&mut self, obj_ref: VmObjectRef) -> Result<&mut HeapObject, VmError> {
        let slot = self
            .objects
            .get_mut(obj_ref.index as usize)
            .ok_or(VmError::InvalidObjectReference)?;
        if slot.generation == obj_ref.generation
            && let Some(ref mut obj) = slot.object
        {
            return Ok(obj);
        }
        Err(VmError::InvalidObjectReference)
    }

    pub fn free_object(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        // Force free
        let idx = obj_ref.index as usize;
        let slot = self
            .objects
            .get_mut(idx)
            .ok_or(VmError::InvalidObjectReference)?;

        if slot.generation != obj_ref.generation || slot.object.is_none() {
            return Err(VmError::InvalidObjectReference);
        }

        self.destroy_slot(idx)
    }

    // Anchor Management (Registers, Stack, Globals)
    pub fn retain_anchor(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        let slot = self
            .objects
            .get_mut(obj_ref.index as usize)
            .ok_or(VmError::InvalidObjectReference)?;
        if slot.generation == obj_ref.generation && slot.object.is_some() {
            slot.anchors = slot
                .anchors
                .checked_add(1)
                .ok_or(VmError::ReferenceCountOverflow)?;
            Ok(())
        } else {
            Err(VmError::InvalidObjectReference)
        }
    }

    pub fn release_anchor(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        let idx = obj_ref.index as usize;
        let slot = self
            .objects
            .get_mut(idx)
            .ok_or(VmError::InvalidObjectReference)?;

        if slot.generation != obj_ref.generation || slot.object.is_none() {
            return Err(VmError::InvalidObjectReference);
        }

        slot.anchors = slot
            .anchors
            .checked_sub(1)
            .ok_or(VmError::ReferenceCountUnderflow)?;
        if slot.anchors == 0 && slot.edges == 0 {
            self.destroy_slot(idx)?;
        }
        Ok(())
    }

    // Edge Management (Heap-to-Heap references)
    pub fn retain_edge(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        let slot = self
            .objects
            .get_mut(obj_ref.index as usize)
            .ok_or(VmError::InvalidObjectReference)?;
        if slot.generation == obj_ref.generation && slot.object.is_some() {
            slot.edges = slot
                .edges
                .checked_add(1)
                .ok_or(VmError::ReferenceCountOverflow)?;
            Ok(())
        } else {
            Err(VmError::InvalidObjectReference)
        }
    }

    pub fn release_edge(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        let idx = obj_ref.index as usize;
        let slot = self
            .objects
            .get_mut(idx)
            .ok_or(VmError::InvalidObjectReference)?;

        if slot.generation != obj_ref.generation || slot.object.is_none() {
            return Err(VmError::InvalidObjectReference);
        }

        slot.edges = slot
            .edges
            .checked_sub(1)
            .ok_or(VmError::ReferenceCountUnderflow)?;
        if slot.anchors == 0 && slot.edges == 0 {
            self.destroy_slot(idx)?;
        }
        Ok(())
    }

    pub fn retain_edge_from(
        &mut self,
        parent: VmObjectRef,
        child: VmObjectRef,
    ) -> Result<(), VmError> {
        self.get_object(parent)?;
        if self.reaches(child, parent, &mut HashSet::new())? {
            return Err(VmError::OwnershipCycle);
        }
        self.retain_edge(child)
    }

    fn destroy_slot(&mut self, idx: usize) -> Result<(), VmError> {
        let (obj, heap_bytes) = {
            let slot = self
                .objects
                .get_mut(idx)
                .ok_or(VmError::InvalidObjectReference)?;
            let obj = slot.object.take().ok_or(VmError::InvalidObjectReference)?;
            let heap_bytes = std::mem::take(&mut slot.heap_bytes);
            slot.anchors = 0;
            slot.edges = 0;
            (obj, heap_bytes)
        };

        self.quota.release_heap(1, heap_bytes);
        self.free_slots.push(idx);

        let children = Self::get_children(&obj);
        if let HeapObject::AdapterHandle {
            binding_id,
            type_id,
            id,
        } = obj
        {
            self.pending_adapter_handle_drops
                .push((binding_id, type_id, id));
        }

        for child in children {
            self.release_edge(child)?;
        }
        Ok(())
    }

    fn get_children(obj: &HeapObject) -> Vec<VmObjectRef> {
        let mut children = Vec::new();
        match obj {
            HeapObject::Struct {
                fields,
                strong_fields,
                ..
            } => {
                for (field, is_strong) in fields.iter().zip(strong_fields) {
                    if *is_strong && let VmValue::Object(child_ref) = field {
                        children.push(*child_ref);
                    }
                }
            }
            HeapObject::Array { elements, .. } => {
                for el in elements {
                    if let VmValue::Object(child_ref) = el {
                        children.push(*child_ref);
                    }
                }
            }
            HeapObject::Tuple { elements } => {
                for el in elements {
                    if let VmValue::Object(child_ref) = el {
                        children.push(*child_ref);
                    }
                }
            }
            HeapObject::Choice { payload, .. } => {
                if let VmValue::Object(child_ref) = payload {
                    children.push(*child_ref);
                }
            }
            HeapObject::AdapterHandle { .. } => {}
        }
        children
    }

    fn reaches(
        &self,
        current: VmObjectRef,
        target: VmObjectRef,
        visited: &mut HashSet<VmObjectRef>,
    ) -> Result<bool, VmError> {
        if current == target {
            return Ok(true);
        }
        if !visited.insert(current) {
            return Ok(false);
        }
        let object = self.get_object(current)?;
        for child in Self::get_children(object) {
            if self.reaches(child, target, visited)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn iter_live_objects(
        &self,
    ) -> impl Iterator<Item = (VmObjectRef, &crate::runtime::HeapObject)> {
        self.objects.iter().enumerate().filter_map(|(idx, slot)| {
            slot.object
                .as_ref()
                .map(|o| (VmObjectRef::new(idx as u32, slot.generation), o))
        })
    }

    pub fn iter_live_objects_mut(
        &mut self,
    ) -> impl Iterator<Item = (VmObjectRef, &mut crate::runtime::HeapObject)> {
        self.objects
            .iter_mut()
            .enumerate()
            .filter_map(|(idx, slot)| {
                let generation = slot.generation;
                slot.object
                    .as_mut()
                    .map(|o| (VmObjectRef::new(idx as u32, generation), o))
            })
    }

    pub fn extract_adapter_handles(
        &mut self,
    ) -> Vec<(
        galfus_core::BindingId,
        galfus_core::OpaqueTypeId,
        galfus_core::HandleId,
    )> {
        let mut extracted = Vec::new();
        let mut to_free = Vec::new();

        for (idx, slot) in self.objects.iter_mut().enumerate() {
            if let Some(crate::runtime::HeapObject::AdapterHandle {
                binding_id,
                type_id,
                id,
            }) = &slot.object
            {
                extracted.push((*binding_id, type_id.clone(), *id));
                to_free.push(VmObjectRef::new(idx as u32, slot.generation));
            }
        }

        for obj_ref in to_free {
            self.free_object(obj_ref)
                .expect("adapter handle references are valid while extracting them");
        }

        extracted
    }
}

impl Drop for PrivateHeap {
    fn drop(&mut self) {
        for slot in &mut self.objects {
            if slot.object.take().is_some() {
                self.quota.release_heap(1, slot.heap_bytes);
                slot.heap_bytes = 0;
            }
        }
    }
}
