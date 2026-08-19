use crate::error::VmError;
use crate::runtime::Value;
use crate::runtime::{CallFrame, HeapObject, RuntimeModuleState, VisitRoots, VmObjectRef};
use galfus_bytecode::instruction::Reg;
use galfus_core::ModuleId;
use std::collections::HashMap;

pub use crate::heap::PrivateHeap;

pub struct VmThreadState {
    pub call_stack: Vec<CallFrame>,
    pub system_response: Option<crate::VmValue>,
    pub heap: PrivateHeap,
    pub module_states: HashMap<ModuleId, RuntimeModuleState>,
    pub entry_func: Option<crate::runtime::Value>,
    pub(crate) initializing_module: Option<ModuleId>,
    pub is_spawned: bool,
    pub(crate) global_quota: std::sync::Arc<std::sync::Mutex<crate::quota::GlobalQuota>>,
    pub(crate) thread_quota: std::sync::Arc<std::sync::Mutex<crate::quota::ThreadQuota>>,
}

impl VmThreadState {
    pub fn test_new() -> Self {
        let limits = galfus_contract::LimitsMetadata::default();
        Self::new(
            std::sync::Arc::new(std::sync::Mutex::new(crate::quota::GlobalQuota::new(
                limits.clone(),
            ))),
            std::sync::Arc::new(std::sync::Mutex::new(crate::quota::ThreadQuota::new(
                limits,
            ))),
        )
    }

    pub fn new(
        global_quota: std::sync::Arc<std::sync::Mutex<crate::quota::GlobalQuota>>,
        thread_quota: std::sync::Arc<std::sync::Mutex<crate::quota::ThreadQuota>>,
    ) -> Self {
        Self {
            call_stack: Vec::new(),
            system_response: None,
            heap: PrivateHeap::new(thread_quota.clone()),
            module_states: HashMap::new(),
            entry_func: None,
            initializing_module: None,
            is_spawned: false,
            global_quota,
            thread_quota,
        }
    }

    pub fn mark_spawned(&mut self) -> Result<(), galfus_contract::ExecutionFailureKind> {
        self.global_quota().lock().unwrap().try_reserve_threads(1)?;
        self.is_spawned = true;
        Ok(())
    }

    pub fn push_frame(&mut self, frame: CallFrame) -> Result<(), crate::error::VmError> {
        self.thread_quota()
            .lock()
            .unwrap()
            .try_reserve_call_depth(1)
            .map_err(crate::error::VmError::ResourceLimitExceeded)?;
        self.call_stack.push(frame);
        Ok(())
    }

    pub fn pop_frame(&mut self) -> Option<CallFrame> {
        let frame = self.call_stack.pop();
        if let Some(frame) = &frame {
            self.thread_quota().lock().unwrap().release_call_depth(1);
            for val in &frame.registers {
                if let Value::Object(obj_ref) = val {
                    let _ = self.heap.release_anchor(*obj_ref);
                }
            }
        }
        frame
    }

    pub fn global_quota(&self) -> &std::sync::Arc<std::sync::Mutex<crate::quota::GlobalQuota>> {
        &self.global_quota
    }

    pub fn thread_quota(&self) -> &std::sync::Arc<std::sync::Mutex<crate::quota::ThreadQuota>> {
        &self.thread_quota
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
        let mut extracted = std::mem::take(&mut self.heap.pending_adapter_handle_drops);
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
            let old_val = std::mem::replace(&mut frame.registers[reg.raw() as usize], val);
            if let Value::Object(obj_ref) = old_val {
                let _ = self.heap.release_anchor(obj_ref);
            }
            Ok(())
        } else {
            Err(VmError::RegisterOutOfBounds { reg })
        }
    }

    pub fn retain_anchor_val(&mut self, val: &Value) {
        if let Value::Object(obj_ref) = val {
            let _ = self.heap.retain_anchor(*obj_ref);
        }
    }

    pub fn retain_edge_val(&mut self, val: &Value) {
        if let Value::Object(obj_ref) = val {
            let _ = self.heap.retain_edge(*obj_ref);
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

impl Drop for VmThreadState {
    fn drop(&mut self) {
        if self.is_spawned {
            self.global_quota().lock().unwrap().release_threads(1);
        }
        self.thread_quota()
            .lock()
            .unwrap()
            .release_call_depth(self.call_stack.len());
    }
}
