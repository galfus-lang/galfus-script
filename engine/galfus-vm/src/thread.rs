use crate::VmValue;
use crate::runtime;

use crate::error::VmError;
use crate::runtime::Value;
use crate::runtime::{CallFrame, HeapObject, RuntimeModuleState, VisitRoots, VmObjectRef};
use galfus_bytecode::instruction::Reg;
use galfus_core::ModuleId;
use std::collections::HashMap;

pub struct PrivateHeap {
    objects: Vec<Option<(VmObjectRef, HeapObject)>>,
    free_slots: Vec<usize>,
    allocations_since_release: usize,
    next_id: u64,
    object_to_slot: HashMap<VmObjectRef, usize>,
}

impl Default for PrivateHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivateHeap {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            free_slots: Vec::new(),
            allocations_since_release: 0,
            next_id: 1,
            object_to_slot: HashMap::new(),
        }
    }

    pub fn alloc(&mut self, obj: HeapObject) -> Result<VmObjectRef, VmError> {
        self.allocations_since_release += 1;

        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(VmError::IdCounterExhausted)?;
        let obj_ref = VmObjectRef(id);

        let idx = if let Some(idx) = self.free_slots.pop() {
            idx
        } else {
            let idx = self.objects.len();
            self.objects.push(None);
            idx
        };

        self.objects[idx] = Some((obj_ref, obj));
        self.object_to_slot.insert(obj_ref, idx);

        Ok(obj_ref)
    }

    #[cfg(test)]
    pub fn exhaust_id_counter(&mut self) {
        self.next_id = u64::MAX;
    }

    pub fn get_object(&self, obj_ref: VmObjectRef) -> Result<&HeapObject, VmError> {
        let idx = *self
            .object_to_slot
            .get(&obj_ref)
            .ok_or(VmError::InvalidObjectReference)?;
        if let Some((_, ref obj)) = self.objects[idx] {
            return Ok(obj);
        }
        Err(VmError::InvalidObjectReference)
    }

    pub fn get_object_mut(&mut self, obj_ref: VmObjectRef) -> Result<&mut HeapObject, VmError> {
        let idx = *self
            .object_to_slot
            .get(&obj_ref)
            .ok_or(VmError::InvalidObjectReference)?;
        if let Some((_, ref mut obj)) = self.objects[idx] {
            return Ok(obj);
        }
        Err(VmError::InvalidObjectReference)
    }

    pub fn free_object(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        let idx = self
            .object_to_slot
            .remove(&obj_ref)
            .ok_or(VmError::InvalidObjectReference)?;
        self.objects[idx] = None;
        self.free_slots.push(idx);
        Ok(())
    }

    pub fn allocations_since_release(&self) -> usize {
        self.allocations_since_release
    }

    pub fn reset_allocations_since_release(&mut self) {
        self.allocations_since_release = 0;
    }

    pub fn iter_live_objects(&self) -> impl Iterator<Item = (VmObjectRef, &HeapObject)> {
        self.objects
            .iter()
            .filter_map(|slot| slot.as_ref().map(|(r, o)| (*r, o)))
    }

    pub fn iter_live_objects_mut(
        &mut self,
    ) -> impl Iterator<Item = (VmObjectRef, &mut HeapObject)> {
        self.objects
            .iter_mut()
            .filter_map(|slot| slot.as_mut().map(|(r, o)| (*r, o)))
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
            if let Some((
                obj_ref,
                crate::runtime::HeapObject::AdapterHandle {
                    binding_id,
                    type_id,
                    id,
                },
            )) = slot
            {
                extracted.push((*binding_id, type_id.clone(), *id));
                to_free.push((*obj_ref, idx));
            }
        }

        for (obj_ref, idx) in to_free {
            self.objects[idx] = None;
            self.object_to_slot.remove(&obj_ref);
            self.free_slots.push(idx);
        }

        extracted
    }
}

pub struct VmThreadState {
    pub call_stack: Vec<CallFrame>,
    pub system_response: Option<VmValue>,
    pub heap: PrivateHeap,
    pub module_states: HashMap<ModuleId, RuntimeModuleState>,
    pub entry_func: Option<runtime::Value>,
    pub initializing_module: Option<ModuleId>,
    /// Adapter handles detached by graph release. The runtime owns dispatching
    /// their adapter release notifications on the main thread.
    pub pending_adapter_handle_drops: Vec<(
        galfus_core::BindingId,
        galfus_core::OpaqueTypeId,
        galfus_core::HandleId,
    )>,
}

impl Default for VmThreadState {
    fn default() -> Self {
        Self::new()
    }
}

impl VmThreadState {
    pub fn new() -> Self {
        Self {
            call_stack: Vec::new(),
            system_response: None,
            heap: PrivateHeap::new(),
            module_states: HashMap::new(),
            entry_func: None,
            initializing_module: None,
            pending_adapter_handle_drops: Vec::new(),
        }
    }

    pub fn module_state(&self, module_id: ModuleId) -> Option<&RuntimeModuleState> {
        self.module_states.get(&module_id)
    }

    pub fn is_module_initialized(&self, module_id: ModuleId) -> bool {
        self.module_state(module_id)
            .is_some_and(|state| state.initialized)
    }

    pub fn mark_module_initialized(&mut self, module_id: ModuleId) {
        self.module_states.entry(module_id).or_default().initialized = true;
    }

    pub fn extract_all_adapter_handles(
        &mut self,
    ) -> Vec<(
        galfus_core::BindingId,
        galfus_core::OpaqueTypeId,
        galfus_core::HandleId,
    )> {
        let mut extracted = std::mem::take(&mut self.pending_adapter_handle_drops);
        extracted.extend(self.heap.extract_adapter_handles());
        extracted
    }

    pub fn begin_module_initialization(&mut self, module_id: ModuleId) {
        self.initializing_module = Some(module_id);
    }

    pub fn finish_module_initialization(&mut self) -> Option<ModuleId> {
        self.initializing_module.take()
    }

    pub fn initializing_module(&self) -> Option<ModuleId> {
        self.initializing_module
    }

    pub fn read_reg(&self, reg: Reg) -> Result<Value, VmError> {
        let frame = self.call_stack.last().ok_or(VmError::EmptyCallStack)?;
        frame
            .registers
            .get(reg.raw() as usize)
            .cloned()
            .ok_or(VmError::RegisterOutOfBounds { reg })
    }

    pub fn write_reg(&mut self, reg: Reg, val: Value) -> Result<(), VmError> {
        let frame = self.call_stack.last_mut().ok_or(VmError::EmptyCallStack)?;
        if (reg.raw() as usize) < frame.registers.len() {
            frame.registers[reg.raw() as usize] = val;
            Ok(())
        } else {
            Err(VmError::RegisterOutOfBounds { reg })
        }
    }

    pub fn contains_future_handle(&self, future_id: galfus_core::FutureId) -> bool {
        self.call_stack.iter().any(|frame| {
            frame
                .registers
                .iter()
                .any(|value| matches!(value, Value::Future(id) if *id == future_id))
        }) || self.module_states.values().any(|state| {
            state
                .globals
                .iter()
                .any(|value| matches!(value, Value::Future(id) if *id == future_id))
        }) || self
            .heap
            .iter_live_objects()
            .any(|(_, object)| match object {
                HeapObject::Struct { fields, .. } | HeapObject::Tuple { elements: fields } => {
                    fields
                        .iter()
                        .any(|value| matches!(value, Value::Future(id) if *id == future_id))
                }
                HeapObject::Array { elements, .. } => elements
                    .iter()
                    .any(|value| matches!(value, Value::Future(id) if *id == future_id)),
                HeapObject::Choice { payload, .. } => {
                    matches!(payload, Value::Future(id) if *id == future_id)
                }
                HeapObject::AdapterHandle { .. } => false,
            })
    }
}

impl VisitRoots for VmThreadState {
    fn visit_roots(&self, visitor: &mut impl FnMut(VmObjectRef)) {
        for state in self.module_states.values() {
            state.visit_roots(visitor);
        }
        for frame in &self.call_stack {
            frame.visit_roots(visitor);
        }
        if let Some(ref response) = self.system_response {
            response.visit_roots(visitor);
        }
        if let Some(ref entry) = self.entry_func {
            entry.visit_roots(visitor);
        }
    }
}
