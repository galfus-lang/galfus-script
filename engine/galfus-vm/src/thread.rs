use crate::error::VmError;
use crate::runtime::Value;
use crate::runtime::{CallFrame, HeapObject, RuntimeModuleState};
use galfus_bytecode::instruction::Reg;
use galfus_core::ModuleId;
use std::collections::HashMap;

pub use crate::heap::PrivateHeap;

pub struct VmThreadState {
    pub call_stack: Vec<CallFrame>,
    pub registers: Vec<Value>,
    pub current_register_base: usize,
    pub current_register_top: usize,
    pub current_frame_has_objects: bool,
    pub system_response: Option<crate::VmValue>,
    pub heap: PrivateHeap,
    pub module_states: HashMap<ModuleId, RuntimeModuleState>,
    pub entry_func: Option<crate::runtime::Value>,
    pub(crate) initializing_module: Option<ModuleId>,
    pub is_spawned: bool,
    pub(crate) global_quota: std::sync::Arc<std::sync::Mutex<crate::quota::GlobalQuota>>,
    pub(crate) thread_quota: std::sync::Arc<crate::quota::ThreadQuota>,
}

impl VmThreadState {
    pub fn test_new() -> Self {
        let limits = galfus_contract::LimitsMetadata::default();
        Self::new(
            std::sync::Arc::new(std::sync::Mutex::new(crate::quota::GlobalQuota::new(
                limits.clone(),
            ))),
            std::sync::Arc::new(crate::quota::ThreadQuota::new(limits)),
        )
    }

    pub fn new(
        global_quota: std::sync::Arc<std::sync::Mutex<crate::quota::GlobalQuota>>,
        thread_quota: std::sync::Arc<crate::quota::ThreadQuota>,
    ) -> Self {
        Self {
            call_stack: Vec::with_capacity(64),
            registers: vec![Value::Null; 4096],
            current_register_base: 0,
            current_register_top: 0,
            current_frame_has_objects: false,
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

    pub fn push_frame(
        &mut self,
        module_id: ModuleId,
        func_idx: galfus_bytecode::instruction::FuncIdx,
        pc: usize,
        return_dest: Option<Reg>,
        register_count: usize,
        cached_instructions: *const [galfus_bytecode::instruction::Instruction],
    ) -> Result<(), crate::error::VmError> {
        if self.call_stack.len() >= self.thread_quota().limits().max_call_depth {
            return Err(crate::error::VmError::ResourceLimitExceeded(
                galfus_contract::ExecutionFailureKind::ResourceLimitExceeded {
                    resource: galfus_contract::ResourceLimitKind::CallDepth,
                    current: self.call_stack.len(),
                    requested: 1,
                    limit: self.thread_quota().limits().max_call_depth,
                },
            ));
        }
        let register_base = self.current_register_top;
        let new_top = register_base + register_count;
        if new_top > self.registers.len() {
            self.registers
                .resize(new_top.max(self.registers.len() * 2), Value::Null);
        }
        self.current_register_base = register_base;
        self.current_register_top = new_top;

        self.call_stack.push(CallFrame {
            module_id,
            func_idx,
            register_base,
            pc,
            return_dest,
            cached_instructions,
            has_objects: self.current_frame_has_objects,
        });
        self.current_frame_has_objects = false;
        Ok(())
    }

    pub fn setup_args_from_caller(
        &mut self,
        args_start: Reg,
        arg_count: usize,
        has_obj: Option<Reg>,
    ) -> Result<(), VmError> {
        let stack_len = self.call_stack.len();
        if stack_len < 2 {
            return Err(VmError::EmptyCallStack);
        }
        let caller_base = self.call_stack[stack_len - 2].register_base;
        let callee_base = self.call_stack[stack_len - 1].register_base;

        for i in 0..arg_count {
            let src_reg = if let Some(obj_reg) = has_obj {
                if i == 0 {
                    obj_reg
                } else {
                    Reg(args_start.raw() + i as u16)
                }
            } else {
                Reg(args_start.raw() + i as u16)
            };

            let src_idx = caller_base + src_reg.raw() as usize;
            let val = self
                .registers
                .get(src_idx)
                .cloned()
                .ok_or(VmError::RegisterOutOfBounds { reg: src_reg })?;

            if matches!(val, Value::Object(_)) {
                self.current_frame_has_objects = true;
            }
            self.retain_anchor_val(&val);
            self.registers[callee_base + i] = val;
        }
        Ok(())
    }

    pub fn pop_frame(&mut self) -> Option<CallFrame> {
        let frame = self.call_stack.pop();
        if let Some(frame) = &frame {

            if self.current_frame_has_objects {
                for i in frame.register_base..self.current_register_top {
                    let val = std::mem::replace(&mut self.registers[i], Value::Null);
                    if let Value::Object(obj_ref) = val {
                        let _ = self.heap.release_anchor(obj_ref);
                    }
                }
            }
            self.current_register_top = frame.register_base;
            self.current_register_base =
                self.call_stack.last().map(|f| f.register_base).unwrap_or(0);
            self.current_frame_has_objects = frame.has_objects;
        }
        frame
    }

    pub fn global_quota(&self) -> &std::sync::Arc<std::sync::Mutex<crate::quota::GlobalQuota>> {
        &self.global_quota
    }

    pub fn thread_quota(&self) -> &std::sync::Arc<crate::quota::ThreadQuota> {
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

    pub fn read_reg(&self, reg: Reg) -> Value {
        let idx = self.current_register_base + reg.raw() as usize;
        unsafe { *self.registers.get_unchecked(idx) }
    }

    pub fn write_reg(&mut self, reg: Reg, val: Value) {
        let idx = self.current_register_base + reg.raw() as usize;
        if matches!(val, Value::Object(_)) {
            self.current_frame_has_objects = true;
        }
        let old_val = unsafe { std::mem::replace(self.registers.get_unchecked_mut(idx), val) };
        if let Value::Object(obj_ref) = old_val {
            let _ = self.heap.release_anchor(obj_ref);
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
        self.registers
            .iter()
            .any(|value| matches!(value, Value::Future(id) if *id == future_id))
            || self.module_states.values().any(|state| {
                state
                    .globals
                    .iter()
                    .any(|value| matches!(value, Value::Future(id) if *id == future_id))
            })
            || self
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

impl Drop for VmThreadState {
    fn drop(&mut self) {
        if self.is_spawned {
            self.global_quota().lock().unwrap().release_threads(1);
        }
        self.thread_quota()
            .release_call_depth(self.call_stack.len());
    }
}
