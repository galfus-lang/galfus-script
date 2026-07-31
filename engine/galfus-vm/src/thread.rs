use crate::VmValue;
use crate::runtime;

use crate::error::VmError;
use crate::runtime::Value;
use crate::runtime::{CallFrame, HeapObject, RuntimeModuleState, VmObjectRef};
use galfus_bytecode::instruction::Reg;
use galfus_core::ModuleId;
use std::collections::HashMap;

pub struct PrivateHeap {
    pub objects: Vec<Option<HeapObject>>,
    pub free_slots: Vec<usize>,
    pub allocations_since_release: usize,
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
        }
    }

    pub fn alloc(&mut self, obj: HeapObject) -> VmObjectRef {
        self.allocations_since_release += 1;

        if let Some(idx) = self.free_slots.pop() {
            self.objects[idx] = Some(obj);
            VmObjectRef(idx)
        } else {
            let idx = self.objects.len();
            self.objects.push(Some(obj));
            VmObjectRef(idx)
        }
    }

    pub fn get_object(&self, obj_ref: VmObjectRef) -> Result<&HeapObject, VmError> {
        let idx = obj_ref.raw();
        if idx < self.objects.len()
            && let Some(ref obj) = self.objects[idx]
        {
            return Ok(obj);
        }
        Err(VmError::InvalidObjectReference)
    }

    pub fn get_object_mut(&mut self, obj_ref: VmObjectRef) -> Result<&mut HeapObject, VmError> {
        let idx = obj_ref.raw();
        if idx < self.objects.len()
            && let Some(ref mut obj) = self.objects[idx]
        {
            return Ok(obj);
        }
        Err(VmError::InvalidObjectReference)
    }

    pub fn free_object(&mut self, obj_ref: VmObjectRef) -> Result<(), VmError> {
        let idx = obj_ref.raw();
        if idx < self.objects.len() && self.objects[idx].is_some() {
            self.objects[idx] = None;
            self.free_slots.push(idx);
            return Ok(());
        }
        Err(VmError::InvalidObjectReference)
    }
}

pub struct VmThreadState {
    pub call_stack: Vec<CallFrame>,
    pub system_response: Option<VmValue>,
    pub heap: PrivateHeap,
    pub module_states: HashMap<ModuleId, RuntimeModuleState>,
    pub entry_func: Option<runtime::Value>,
    pub initializing_module: Option<ModuleId>,
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
}
