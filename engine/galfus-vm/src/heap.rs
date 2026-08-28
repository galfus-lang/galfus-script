use crate::error::VmError;
use crate::runtime::{HeapObject, VmObjectRef, VmValue};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GcColor {
    Black,
    Gray,
    White,
    Purple,
}

pub struct HeapSlot {
    pub generation: u32,
    pub anchors: u32,
    pub edges: u32,
    pub color: GcColor,
    pub object: Option<HeapObject>,
}

pub struct PrivateHeap {
    objects: Vec<HeapSlot>,
    free_slots: Vec<usize>,
    roots: HashSet<usize>,
    allocations_since_release: usize,
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
            roots: HashSet::new(),
            allocations_since_release: 0,
            pending_adapter_handle_drops: Vec::new(),
            quota,
        }
    }

    pub fn alloc(&mut self, obj: HeapObject) -> Result<VmObjectRef, VmError> {
        self.quota
            .try_reserve_heap(1, obj.heap_bytes())
            .map_err(VmError::ResourceLimitExceeded)?;

        self.allocations_since_release += 1;
        if self.allocations_since_release >= 500 {
            self.collect_cycles();
            self.allocations_since_release = 0;
        }

        // When allocating, an object initially has 0 anchors and 0 edges.
        // It's the caller's responsibility to call retain_anchor or retain_edge.
        // For backwards compatibility and safety, we start anchors at 1 (as it is usually placed in a register).
        let (idx, generation) = if let Some(idx) = self.free_slots.pop() {
            let slot = &mut self.objects[idx];
            slot.generation += 1;
            slot.anchors = 1;
            slot.edges = 0;
            slot.color = GcColor::Black;
            slot.object = Some(obj);
            (idx, slot.generation)
        } else {
            let idx = self.objects.len();
            if idx >= u32::MAX as usize {
                return Err(VmError::IdCounterExhausted);
            }
            let generation = 1;
            self.objects.push(HeapSlot {
                generation,
                anchors: 1,
                edges: 0,
                color: GcColor::Black,
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

        self.roots.remove(&idx);

        if let Some(obj) = slot.object.take() {
            self.quota.release_heap(1, obj.heap_bytes());

            if let HeapObject::AdapterHandle {
                binding_id,
                type_id,
                id,
            } = obj
            {
                self.pending_adapter_handle_drops
                    .push((binding_id, type_id, id));
            }
        }

        slot.anchors = 0;
        slot.edges = 0;
        slot.color = GcColor::Black;
        self.free_slots.push(idx);
        Ok(())
    }

    // Anchor Management (Registers, Stack, Globals)
    pub fn retain_anchor(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        let slot = self
            .objects
            .get_mut(obj_ref.index as usize)
            .ok_or(VmError::InvalidObjectReference)?;
        if slot.generation == obj_ref.generation && slot.object.is_some() {
            slot.anchors = slot.anchors.saturating_add(1);
            slot.color = GcColor::Black;
            self.roots.remove(&(obj_ref.index as usize));
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

        slot.anchors = slot.anchors.saturating_sub(1);
        if slot.anchors == 0 {
            if slot.edges == 0 {
                self.release_internal(idx);
            } else if slot.color != GcColor::Purple {
                slot.color = GcColor::Purple;
                self.roots.insert(idx);
            }
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
            slot.edges = slot.edges.saturating_add(1);
            slot.color = GcColor::Black;
            self.roots.remove(&(obj_ref.index as usize));
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

        slot.edges = slot.edges.saturating_sub(1);
        if slot.anchors == 0 && slot.edges == 0 {
            self.release_internal(idx);
        } else if slot.anchors == 0 && slot.edges > 0 && slot.color != GcColor::Purple {
            slot.color = GcColor::Purple;
            self.roots.insert(idx);
        }
        Ok(())
    }

    fn release_internal(&mut self, idx: usize) {
        let slot = &mut self.objects[idx];
        slot.color = GcColor::Black;
        self.roots.remove(&idx);

        if let Some(obj) = slot.object.take() {
            self.quota.release_heap(1, obj.heap_bytes());

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
                let _ = self.release_edge(child);
            }
        }
    }

    fn get_children(obj: &HeapObject) -> Vec<VmObjectRef> {
        let mut children = Vec::new();
        match obj {
            HeapObject::Struct { fields, .. } => {
                for field in fields {
                    if let VmValue::Object(child_ref) = field {
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

    // State Machine Cycle Collector (Bacon & Rajan Algorithm)
    pub fn collect_cycles(&mut self) {
        if self.roots.is_empty() {
            return;
        }
        self.mark_roots();
        self.scan_roots();
        self.collect_roots();
    }

    fn mark_roots(&mut self) {
        let roots: Vec<usize> = self.roots.drain().collect();
        for idx in roots {
            let (color, anchors, edges) = {
                let slot = &self.objects[idx];
                (slot.color, slot.anchors, slot.edges)
            };
            if color == GcColor::Purple && anchors == 0 && edges > 0 {
                self.mark_gray(idx);
                self.roots.insert(idx);
            } else {
                self.objects[idx].color = GcColor::Black;
                if anchors == 0 && edges == 0 {
                    self.release_internal(idx);
                }
            }
        }
    }

    fn mark_gray(&mut self, idx: usize) {
        let (color, children) = {
            let slot = &self.objects[idx];
            if let Some(obj) = &slot.object {
                (slot.color, Self::get_children(obj))
            } else {
                (slot.color, Vec::new())
            }
        };

        if color != GcColor::Gray {
            self.objects[idx].color = GcColor::Gray;
            for child in children {
                let child_idx = child.index as usize;
                if self.objects[child_idx].generation == child.generation {
                    self.objects[child_idx].edges = self.objects[child_idx].edges.saturating_sub(1);
                    self.mark_gray(child_idx);
                }
            }
        }
    }

    fn scan_roots(&mut self) {
        let roots: Vec<usize> = self.roots.iter().copied().collect();
        for idx in roots {
            self.scan(idx);
        }
    }

    fn scan(&mut self, idx: usize) {
        let (color, anchors, edges, children) = {
            let slot = &self.objects[idx];
            let children = if let Some(obj) = &slot.object {
                Self::get_children(obj)
            } else {
                Vec::new()
            };
            (slot.color, slot.anchors, slot.edges, children)
        };

        if color == GcColor::Gray {
            if anchors > 0 || edges > 0 {
                self.scan_black(idx);
            } else {
                self.objects[idx].color = GcColor::White;
                for child in children {
                    let child_idx = child.index as usize;
                    if self.objects[child_idx].generation == child.generation {
                        self.scan(child_idx);
                    }
                }
            }
        }
    }

    fn scan_black(&mut self, idx: usize) {
        let children = {
            let slot = &self.objects[idx];
            if let Some(obj) = &slot.object {
                Self::get_children(obj)
            } else {
                Vec::new()
            }
        };

        self.objects[idx].color = GcColor::Black;
        for child in children {
            let child_idx = child.index as usize;
            if self.objects[child_idx].generation == child.generation {
                self.objects[child_idx].edges = self.objects[child_idx].edges.saturating_add(1);
                if self.objects[child_idx].color != GcColor::Black {
                    self.scan_black(child_idx);
                }
            }
        }
    }

    fn collect_roots(&mut self) {
        let roots: Vec<usize> = self.roots.drain().collect();
        for idx in roots {
            self.objects[idx].color = GcColor::Black;
            self.collect_white(idx);
        }
    }

    fn collect_white(&mut self, idx: usize) {
        let slot = &mut self.objects[idx];
        if slot.color == GcColor::White && slot.object.is_some() {
            slot.color = GcColor::Black;

            let obj = slot.object.take().unwrap();

            self.quota.release_heap(1, obj.heap_bytes());

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
                let child_idx = child.index as usize;
                if self.objects[child_idx].generation == child.generation {
                    self.collect_white(child_idx);
                }
            }
        }
    }

    // Existing helpers
    pub fn allocations_since_release(&self) -> usize {
        self.allocations_since_release
    }

    pub fn reset_allocations_since_release(&mut self) {
        self.allocations_since_release = 0;
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
            let _ = self.free_object(obj_ref);
        }

        extracted
    }
}

impl Drop for PrivateHeap {
    fn drop(&mut self) {
        for slot in &mut self.objects {
            if let Some(obj) = slot.object.take() {
                self.quota.release_heap(1, obj.heap_bytes());
            }
        }
    }
}
