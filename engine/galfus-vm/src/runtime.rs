mod casts;
mod control;
mod data;

mod heap;
pub mod objects;
mod operators;
mod system;
#[cfg(test)]
mod tests;

use crate::thread;

use crate::error::{StackFrameInfo, VmError, VmPanic};
use galfus_bytecode::instruction::{
    ChoiceLayoutIdx, FuncIdx, Instruction, Reg, StructLayoutIdx, TypeIdx,
};
use galfus_bytecode::{BytecodeGraph, BytecodeType, Constant, OwnershipKind};
use galfus_contract::Providers;
use galfus_core::{BindingId, HandleId, ModuleId, OpaqueTypeId};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct Continuation {
    dest: Option<Reg>,
    expected_result: Option<(ModuleId, TypeIdx)>,
    resumed: Arc<AtomicBool>,
    pub origin_thread_id: Option<galfus_core::ThreadId>,
}

impl Continuation {
    pub(crate) fn new(dest: Option<Reg>) -> Self {
        Self {
            dest,
            expected_result: None,
            resumed: Arc::new(AtomicBool::new(false)),
            origin_thread_id: None,
        }
    }

    pub fn with_origin(mut self, origin: galfus_core::ThreadId) -> Self {
        self.origin_thread_id = Some(origin);
        self
    }

    pub fn for_provider(dest: Reg, module_id: ModuleId, return_type: TypeIdx) -> Self {
        Self {
            dest: Some(dest),
            expected_result: Some((module_id, return_type)),
            resumed: Arc::new(AtomicBool::new(false)),
            origin_thread_id: None,
        }
    }

    pub fn for_future_handle(dest: Reg) -> Self {
        Self::new(Some(dest))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmEffect {
    FutureWait {
        future_id: galfus_core::FutureId,
        module_id: ModuleId,
        return_type: TypeIdx,
    },
    CreateFuture {
        /// Module whose type table encodes the call arguments and Future payload.
        module_id: ModuleId,
        /// Resolved module that owns `func_idx`.
        target_module_id: ModuleId,
        func_idx: FuncIdx,
        args: Vec<Value>,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    },
    CreateIndirectFuture {
        module_id: ModuleId,
        func: Value,
        args: Vec<Value>,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    },
    FutureDropped {
        future_id: galfus_core::FutureId,
    },
    AdapterHandleDropped {
        binding_id: BindingId,
        type_id: OpaqueTypeId,
        id: HandleId,
    },
    FutureWaitAll {
        future_ids: Vec<galfus_core::FutureId>,
        module_id: ModuleId,
        return_type: TypeIdx,
    },
    FutureWaitRace {
        future_ids: Vec<galfus_core::FutureId>,
        module_id: ModuleId,
        return_type: TypeIdx,
    },
}

pub enum VmStep {
    Continue,
    Return {
        value: Value,
        module_id: ModuleId,
        return_type: TypeIdx,
    },
    Suspend {
        effect: VmEffect,
        continuation: Continuation,
    },
    Failed(galfus_contract::ExecutionFailure),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VmObjectRef {
    pub index: u32,
    pub generation: u32,
}

impl VmObjectRef {
    pub fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum VmValue {
    Null,
    Bool(bool),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Uint8(u8),
    Uint16(u16),
    Uint32(u32),
    Uint64(u64),
    Future(galfus_core::FutureId),
    Float32(f32),
    Float64(f64),
    Object(VmObjectRef),
    Function {
        module_id: ModuleId,
        func_idx: FuncIdx,
    },
}

pub type Value = VmValue;
type ObjectRef = VmObjectRef;

#[derive(Clone, Debug, PartialEq)]
pub enum HeapObject {
    Struct {
        module_id: galfus_core::ModuleId,
        layout_idx: StructLayoutIdx,
        fields: Vec<Value>,
    },
    Array {
        module_id: ModuleId,
        element_ty: TypeIdx,
        elements: Vec<Value>,
    },
    Tuple {
        elements: Vec<Value>,
    },
    Choice {
        module_id: galfus_core::ModuleId,
        layout_idx: ChoiceLayoutIdx,
        variant_idx: u16,
        payload: Value,
    },
    AdapterHandle {
        binding_id: BindingId,
        type_id: OpaqueTypeId,
        id: HandleId,
    },
}

impl HeapObject {
    pub fn heap_bytes(&self) -> usize {
        match self {
            Self::Struct { fields, .. } => {
                std::mem::size_of::<Self>() + fields.capacity() * std::mem::size_of::<Value>()
            }
            Self::Array { elements, .. } => {
                std::mem::size_of::<Self>() + elements.capacity() * std::mem::size_of::<Value>()
            }
            Self::Tuple { elements } => {
                std::mem::size_of::<Self>() + elements.capacity() * std::mem::size_of::<Value>()
            }
            Self::Choice { .. } | Self::AdapterHandle { .. } => std::mem::size_of::<Self>(),
        }
    }
}

#[derive(Clone)]
pub struct VmContext {
    providers: Option<Arc<Mutex<Providers>>>,
}

#[derive(Default)]
pub struct RuntimeModuleState {
    pub globals: Vec<VmValue>,
    pub initialized: bool,
}

impl VmContext {
    pub fn new(providers: Option<Providers>) -> Self {
        Self {
            providers: providers.map(|p| Arc::new(Mutex::new(p))),
        }
    }
}

pub struct CallFrame {
    pub module_id: ModuleId,
    pub func_idx: FuncIdx,
    pub pc: usize,
    pub register_base: usize,
    pub return_dest: Option<Reg>,
    pub cached_instructions: *const [galfus_bytecode::Instruction],
    pub has_objects: bool,
}

unsafe impl Send for CallFrame {}
unsafe impl Sync for CallFrame {}

#[derive(Clone)]
pub struct VirtualMachine {
    pub graph: Arc<BytecodeGraph>,
    pub context: VmContext,
    pub fast_modules: Vec<(
        galfus_core::ModuleId,
        *const galfus_bytecode::BytecodeModule,
    )>,
    uint8_type_indexes: Vec<(galfus_core::ModuleId, Option<TypeIdx>)>,
}

impl VirtualMachine {
    /// Resumes a suspended VM operation exactly once without exposing register layout.
    #[allow(clippy::result_large_err)]
    pub fn resume(
        &self,
        thread_id: galfus_core::ThreadId,
        thread: &mut thread::VmThreadState,
        continuation: Continuation,
        value: Value,
    ) -> Result<(), galfus_contract::ExecutionFailure> {
        if continuation.origin_thread_id != Some(thread_id) {
            return Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InvalidContinuation,
                "continuation resumed by wrong thread",
            ));
        }
        if continuation.resumed.swap(true, Ordering::AcqRel) {
            return Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::DuplicateCompletion,
                "continuation was already resumed",
            ));
        }
        if let Some((module_id, expected_type)) = continuation.expected_result
            && !self.value_matches_type(thread, value, module_id, expected_type)
        {
            return Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InvalidContinuation,
                "continuation result does not match its declared type",
            ));
        }
        if let Some(dest) = continuation.dest {
            thread.write_reg(dest, value);
        }
        Ok(())
    }

    fn value_matches_type(
        &self,
        thread: &thread::VmThreadState,
        value: Value,
        module_id: ModuleId,
        type_idx: TypeIdx,
    ) -> bool {
        let Some(module) = self.graph.get(module_id).map(|node| &node.module) else {
            return false;
        };
        let Some(expected) = module.types.get(type_idx.raw() as usize) else {
            return false;
        };
        match (expected, value) {
            (BytecodeType::Null, Value::Null)
            | (BytecodeType::Bool, Value::Bool(_))
            | (BytecodeType::Int8, Value::Int8(_))
            | (BytecodeType::Int16, Value::Int16(_))
            | (BytecodeType::Int32, Value::Int32(_))
            | (BytecodeType::Int64, Value::Int64(_))
            | (BytecodeType::Uint8, Value::Uint8(_))
            | (BytecodeType::Uint16, Value::Uint16(_))
            | (BytecodeType::Uint32, Value::Uint32(_))
            | (BytecodeType::Uint64, Value::Uint64(_))
            | (BytecodeType::Float32, Value::Float32(_))
            | (BytecodeType::Float64, Value::Float64(_)) => true,
            (BytecodeType::Nullable(_), Value::Null) => true,
            (BytecodeType::Nullable(inner), value) => {
                self.value_matches_type(thread, value, module_id, *inner)
            }
            (BytecodeType::Array(element_type), Value::Object(reference)) => {
                let Ok(HeapObject::Array {
                    module_id: value_module,
                    element_ty,
                    elements,
                }) = thread.heap.get_object(reference)
                else {
                    return false;
                };
                *value_module == module_id
                    && element_ty == element_type
                    && elements.iter().cloned().all(|value| {
                        self.value_matches_type(thread, value, module_id, *element_type)
                    })
            }
            (BytecodeType::Tuple(element_types), Value::Object(reference)) => {
                let Ok(HeapObject::Tuple { elements }) = thread.heap.get_object(reference) else {
                    return false;
                };
                elements.len() == element_types.len()
                    && elements
                        .iter()
                        .cloned()
                        .zip(element_types)
                        .all(|(value, type_idx)| {
                            self.value_matches_type(thread, value, module_id, *type_idx)
                        })
            }
            (BytecodeType::Choice(layout_idx), Value::Object(reference)) => {
                let Ok(HeapObject::Choice {
                    module_id: value_module,
                    layout_idx: actual_layout,
                    variant_idx,
                    payload,
                }) = thread.heap.get_object(reference)
                else {
                    return false;
                };
                if *value_module != module_id || actual_layout != layout_idx {
                    return false;
                }
                let Some(variant) = module
                    .choice_layouts
                    .get(layout_idx.raw() as usize)
                    .and_then(|layout| layout.variants.get(*variant_idx as usize))
                else {
                    return false;
                };
                match variant.payload_ty {
                    Some(type_idx) => {
                        self.value_matches_type(thread, *payload, module_id, type_idx)
                    }
                    None => matches!(payload, Value::Null),
                }
            }
            (BytecodeType::AdapterHandle(type_id), Value::Object(reference)) => matches!(
                thread.heap.get_object(reference),
                Ok(HeapObject::AdapterHandle { type_id: actual, .. }) if actual == type_id
            ),
            _ => false,
        }
    }
    pub fn providers(&self) -> Option<Arc<Mutex<Providers>>> {
        self.context.providers.clone()
    }

    pub fn new(graph: Arc<BytecodeGraph>) -> Self {
        let mut fast_modules = Vec::new();
        let mut uint8_type_indexes = Vec::new();
        for module in graph.modules() {
            fast_modules.push((module.id, &module.module as *const _));
            uint8_type_indexes.push((
                module.id,
                module
                    .module
                    .types
                    .iter()
                    .position(|ty| matches!(ty, BytecodeType::Uint8))
                    .map(|idx| TypeIdx(idx as u16)),
            ));
        }
        fast_modules.sort_unstable_by_key(|&(id, _)| id);
        uint8_type_indexes.sort_unstable_by_key(|&(id, _)| id);
        Self {
            graph,
            context: VmContext::new(None),
            fast_modules,
            uint8_type_indexes,
        }
    }

    pub fn with_context(mut self, context: VmContext) -> Self {
        self.context = context;
        self
    }

    pub fn with_providers(mut self, providers: Option<Providers>) -> Self {
        self.context = VmContext::new(providers);
        self
    }

    pub fn with_provider_handle(mut self, providers: Option<Arc<Mutex<Providers>>>) -> Self {
        self.context.providers = providers;
        self
    }

    pub(crate) fn get_module(
        &self,
        id: galfus_core::ModuleId,
    ) -> Result<&galfus_bytecode::BytecodeModule, VmError> {
        match self.fast_modules.binary_search_by_key(&id, |&(k, _)| k) {
            Ok(idx) => Ok(unsafe { &*self.fast_modules[idx].1 }),
            Err(_) => Err(VmError::ModuleNotFound { module_id: id }),
        }
    }

    pub fn get_function(
        &self,
        module_id: galfus_core::ModuleId,
        func_idx: FuncIdx,
    ) -> Result<&galfus_bytecode::BytecodeFunction, VmError> {
        let module = self.get_module(module_id)?;
        module
            .functions
            .get(func_idx.raw() as usize)
            .ok_or(VmError::FunctionOutOfBounds { index: func_idx })
    }

    pub fn current_image(
        &self,
        thread: &thread::VmThreadState,
    ) -> Result<&galfus_bytecode::BytecodeModule, VmError> {
        let frame = thread.call_stack.last().ok_or(VmError::EmptyCallStack)?;
        self.get_module(frame.module_id)
    }

    pub fn prepare_function(
        &self,
        thread: &mut thread::VmThreadState,
        module_id: galfus_core::ModuleId,
        func_idx: FuncIdx,
        args: Vec<Value>,
    ) -> Result<(), VmPanic> {
        self.validate_graph_format().map_err(|error| VmPanic {
            error,
            stack_trace: vec![],
        })?;

        let func = self
            .get_function(module_id, func_idx)
            .map_err(|error| VmPanic {
                error,
                stack_trace: vec![],
            })?;

        if args.len() != func.param_count as usize {
            return Err(VmPanic {
                error: VmError::TypeMismatch {
                    expected: format!("{} arguments", func.param_count),
                    found: format!("{} arguments", args.len()),
                },
                stack_trace: vec![],
            });
        }

        thread.call_stack.clear();
        thread.registers.clear();
        thread.current_register_base = 0;
        let total_regs =
            func.param_count as usize + func.local_count as usize + func.temp_count as usize;
        let mut registers = vec![Value::Null; total_regs];
        for (i, val) in args.into_iter().enumerate() {
            registers[i] = val;
        }

        let cached_instructions = func.instructions.as_slice() as *const _;
        thread
            .push_frame(
                module_id,
                func_idx,
                0,
                None,
                registers.len(),
                cached_instructions,
            )
            .map_err(|error| VmPanic {
                error,
                stack_trace: vec![],
            })?;

        let register_base = thread.call_stack.last().unwrap().register_base;
        for (i, val) in registers.into_iter().enumerate() {
            thread.retain_anchor_val(&val);
            thread.registers[register_base + i] = val;
        }

        Ok(())
    }

    pub fn run_function(
        &self,
        thread: &mut thread::VmThreadState,
        module_id: galfus_core::ModuleId,
        func_idx: FuncIdx,
        args: Vec<Value>,
    ) -> Result<Value, VmPanic> {
        self.validate_graph_format().map_err(|error| VmPanic {
            error,
            stack_trace: vec![],
        })?;

        let func = self
            .get_function(module_id, func_idx)
            .map_err(|error| VmPanic {
                error,
                stack_trace: vec![],
            })?;

        if args.len() != func.param_count as usize {
            return Err(VmPanic {
                error: VmError::TypeMismatch {
                    expected: format!("{} arguments", func.param_count),
                    found: format!("{} arguments", args.len()),
                },
                stack_trace: vec![],
            });
        }

        thread.call_stack.clear();
        thread.registers.clear();
        thread.current_register_base = 0;
        let total_regs =
            func.param_count as usize + func.local_count as usize + func.temp_count as usize;
        let mut registers = vec![Value::Null; total_regs];
        for (i, val) in args.into_iter().enumerate() {
            registers[i] = val;
        }

        let cached_instructions = func.instructions.as_slice() as *const _;
        thread
            .push_frame(
                module_id,
                func_idx,
                0,
                None,
                registers.len(),
                cached_instructions,
            )
            .map_err(|error| VmPanic {
                error,
                stack_trace: vec![],
            })?;

        let register_base = thread.call_stack.last().unwrap().register_base;
        for (i, val) in registers.into_iter().enumerate() {
            thread.retain_anchor_val(&val);
            thread.registers[register_base + i] = val;
        }

        match self.execute_loop(thread) {
            Ok(val) => Ok(val),
            Err(err) => {
                let mut stack_trace = Vec::new();
                for frame in thread.call_stack.iter().rev() {
                    stack_trace.push(StackFrameInfo {
                        module_id: frame.module_id,
                        func_idx: frame.func_idx,
                        instruction_offset: frame.pc.saturating_sub(1),
                    });
                }
                Err(VmPanic {
                    error: err,
                    stack_trace,
                })
            }
        }
    }

    pub fn execute_with_budget(
        &self,
        thread: &mut thread::VmThreadState,
        mut budget: usize,
    ) -> Result<VmStep, VmPanic> {
        // Check pending adapter handle drops first
        if let Some((binding_id, type_id, id)) = thread.heap.pending_adapter_handle_drops.pop() {
            return Ok(VmStep::Suspend {
                effect: VmEffect::AdapterHandleDropped {
                    binding_id,
                    type_id,
                    id,
                },
                continuation: Continuation::new(None),
            });
        }

        let Some(frame) = thread.call_stack.last_mut() else {
            return Err(VmPanic {
                error: VmError::EmptyCallStack,
                stack_trace: vec![],
            });
        };

        // Cache hot state in locals — LLVM will promote these to CPU registers
        let mut pc = frame.pc;
        let mut instructions: *const [Instruction] = frame.cached_instructions;
        let mut register_base = frame.register_base;
        let mut current_module_id = frame.module_id;
        let mut current_image = self.get_module(current_module_id).unwrap();

        // Macro to sync local state back to the call frame
        macro_rules! sync_frame {
            ($thread:expr) => {
                let len = $thread.call_stack.len();
                if len > 0 {
                    unsafe {
                        $thread.call_stack.get_unchecked_mut(len - 1).pc = pc;
                    }
                }
            };
        }

        // Macro to reload local state from the current call frame
        macro_rules! reload_frame {
            ($thread:expr) => {
                let len = $thread.call_stack.len();
                if len > 0 {
                    let f = unsafe { $thread.call_stack.get_unchecked_mut(len - 1) };
                    pc = f.pc;
                    instructions = f.cached_instructions;
                    register_base = f.register_base;
                    if current_module_id != f.module_id {
                        current_module_id = f.module_id;
                        current_image = self.get_module(current_module_id).unwrap();
                    }
                }
            };
        }

        let mut registers_ptr = thread.registers.as_mut_ptr();

        // Inline register access using cached base
        macro_rules! read_reg {
            ($thread:expr, $reg:expr) => {{
                let idx = register_base + $reg.raw() as usize;
                unsafe { *registers_ptr.add(idx) }
            }};
        }

        macro_rules! write_reg {
            ($thread:expr, $reg:expr, $val:expr) => {{
                let idx = register_base + $reg.raw() as usize;
                let val = $val;
                if matches!(val, Value::Object(_)) {
                    $thread.current_frame_has_objects = true;
                }
                let old_val = unsafe { registers_ptr.add(idx).replace(val) };
                if let Value::Object(obj_ref) = old_val {
                    let _ = $thread.heap.release_anchor(obj_ref);
                }
            }};
        }

        macro_rules! write_prim_reg {
            ($thread:expr, $reg:expr, $val:expr) => {{
                let idx = register_base + $reg.raw() as usize;
                let val = $val;
                let old_val = unsafe { registers_ptr.add(idx).replace(val) };
                if let Value::Object(obj_ref) = old_val {
                    let _ = $thread.heap.release_anchor(obj_ref);
                }
            }};
        }

        while budget > 0 {
            let instr = unsafe {
                let slice = &*instructions;
                slice.get_unchecked(pc)
            };
            pc += 1;

            match instr {
                // ===== HOT PATH: Arithmetic =====

                // --- AOT Specialized I32 Operations ---
                Instruction::AddI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Int32(l.wrapping_add(r)));
                }
                Instruction::SubI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Int32(l.wrapping_sub(r)));
                }
                Instruction::MulI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Int32(l.wrapping_mul(r)));
                }
                Instruction::DivI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Int32(if r == 0 {
                            return Err(self.make_panic(thread, VmError::DivisionByZero));
                        } else {
                            l.wrapping_div(r)
                        })
                    );
                }
                Instruction::RemI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Int32(if r == 0 {
                            return Err(self.make_panic(thread, VmError::DivisionByZero));
                        } else {
                            l.wrapping_rem(r)
                        })
                    );
                }
                Instruction::EqI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l == r));
                }
                Instruction::NeI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l != r));
                }
                Instruction::LtI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l < r));
                }
                Instruction::LeI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l <= r));
                }
                Instruction::GtI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l > r));
                }
                Instruction::GeI32 { dest, lhs, rhs } => {
                    let Value::Int32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l >= r));
                }

                // --- AOT Specialized I64 Operations ---
                Instruction::AddI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Int64(l.wrapping_add(r)));
                }
                Instruction::SubI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Int64(l.wrapping_sub(r)));
                }
                Instruction::MulI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Int64(l.wrapping_mul(r)));
                }
                Instruction::DivI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Int64(if r == 0 {
                            return Err(self.make_panic(thread, VmError::DivisionByZero));
                        } else {
                            l.wrapping_div(r)
                        })
                    );
                }
                Instruction::RemI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Int64(if r == 0 {
                            return Err(self.make_panic(thread, VmError::DivisionByZero));
                        } else {
                            l.wrapping_rem(r)
                        })
                    );
                }
                Instruction::EqI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l == r));
                }
                Instruction::NeI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l != r));
                }
                Instruction::LtI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l < r));
                }
                Instruction::LeI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l <= r));
                }
                Instruction::GtI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l > r));
                }
                Instruction::GeI64 { dest, lhs, rhs } => {
                    let Value::Int64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Int64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l >= r));
                }

                // --- AOT Specialized F32 Operations ---
                Instruction::AddF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float32(galfus_core::normalize_f32(l + r))
                    );
                }
                Instruction::SubF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float32(galfus_core::normalize_f32(l - r))
                    );
                }
                Instruction::MulF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float32(galfus_core::normalize_f32(l * r))
                    );
                }
                Instruction::DivF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float32(galfus_core::normalize_f32(l / r))
                    );
                }
                Instruction::RemF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float32(galfus_core::normalize_f32(l % r))
                    );
                }
                Instruction::EqF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l == r));
                }
                Instruction::NeF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l != r));
                }
                Instruction::LtF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l < r));
                }
                Instruction::LeF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l <= r));
                }
                Instruction::GtF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l > r));
                }
                Instruction::GeF32 { dest, lhs, rhs } => {
                    let Value::Float32(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float32(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l >= r));
                }

                // --- AOT Specialized F64 Operations ---
                Instruction::AddF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float64(galfus_core::normalize_f64(l + r))
                    );
                }
                Instruction::SubF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float64(galfus_core::normalize_f64(l - r))
                    );
                }
                Instruction::MulF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float64(galfus_core::normalize_f64(l * r))
                    );
                }
                Instruction::DivF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float64(galfus_core::normalize_f64(l / r))
                    );
                }
                Instruction::RemF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(
                        thread,
                        dest,
                        Value::Float64(galfus_core::normalize_f64(l % r))
                    );
                }
                Instruction::EqF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l == r));
                }
                Instruction::NeF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l != r));
                }
                Instruction::LtF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l < r));
                }
                Instruction::LeF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l <= r));
                }
                Instruction::GtF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l > r));
                }
                Instruction::GeF64 { dest, lhs, rhs } => {
                    let Value::Float64(l) = read_reg!(thread, lhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    let Value::Float64(r) = read_reg!(thread, rhs) else {
                        unsafe { std::hint::unreachable_unchecked() }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(l >= r));
                }

                Instruction::Add { dest, lhs, rhs } => {
                    let lhs_val = read_reg!(thread, lhs);
                    let rhs_val = read_reg!(thread, rhs);
                    let res = match (lhs_val, rhs_val) {
                        (Value::Int32(l), Value::Int32(r)) => Value::Int32(l.wrapping_add(r)),
                        (Value::Int64(l), Value::Int64(r)) => Value::Int64(l.wrapping_add(r)),
                        (Value::Int8(l), Value::Int8(r)) => Value::Int8(l.wrapping_add(r)),
                        (Value::Int16(l), Value::Int16(r)) => Value::Int16(l.wrapping_add(r)),
                        (Value::Uint8(l), Value::Uint8(r)) => Value::Uint8(l.wrapping_add(r)),
                        (Value::Uint16(l), Value::Uint16(r)) => Value::Uint16(l.wrapping_add(r)),
                        (Value::Uint32(l), Value::Uint32(r)) => Value::Uint32(l.wrapping_add(r)),
                        (Value::Uint64(l), Value::Uint64(r)) => Value::Uint64(l.wrapping_add(r)),
                        (Value::Float32(l), Value::Float32(r)) => {
                            Value::Float32(galfus_core::normalize_f32(l + r))
                        }
                        (Value::Float64(l), Value::Float64(r)) => {
                            Value::Float64(galfus_core::normalize_f64(l + r))
                        }
                        (l, r) => {
                            sync_frame!(thread);
                            return Err(self.make_panic(
                                thread,
                                VmError::TypeMismatch {
                                    expected: "matching numeric types".to_string(),
                                    found: format!("{:?} and {:?}", l, r),
                                },
                            ));
                        }
                    };
                    write_prim_reg!(thread, dest, res);
                }
                Instruction::Sub { dest, lhs, rhs } => {
                    let lhs_val = read_reg!(thread, lhs);
                    let rhs_val = read_reg!(thread, rhs);
                    let res = match (lhs_val, rhs_val) {
                        (Value::Int32(l), Value::Int32(r)) => Value::Int32(l.wrapping_sub(r)),
                        (Value::Int64(l), Value::Int64(r)) => Value::Int64(l.wrapping_sub(r)),
                        (Value::Int8(l), Value::Int8(r)) => Value::Int8(l.wrapping_sub(r)),
                        (Value::Int16(l), Value::Int16(r)) => Value::Int16(l.wrapping_sub(r)),
                        (Value::Uint8(l), Value::Uint8(r)) => Value::Uint8(l.wrapping_sub(r)),
                        (Value::Uint16(l), Value::Uint16(r)) => Value::Uint16(l.wrapping_sub(r)),
                        (Value::Uint32(l), Value::Uint32(r)) => Value::Uint32(l.wrapping_sub(r)),
                        (Value::Uint64(l), Value::Uint64(r)) => Value::Uint64(l.wrapping_sub(r)),
                        (Value::Float32(l), Value::Float32(r)) => {
                            Value::Float32(galfus_core::normalize_f32(l - r))
                        }
                        (Value::Float64(l), Value::Float64(r)) => {
                            Value::Float64(galfus_core::normalize_f64(l - r))
                        }
                        (l, r) => {
                            sync_frame!(thread);
                            return Err(self.make_panic(
                                thread,
                                VmError::TypeMismatch {
                                    expected: "matching numeric types".to_string(),
                                    found: format!("{:?} and {:?}", l, r),
                                },
                            ));
                        }
                    };
                    write_prim_reg!(thread, dest, res);
                }

                // ===== HOT PATH: Comparisons =====
                Instruction::Lt { dest, lhs, rhs } => {
                    let lhs_val = read_reg!(thread, lhs);
                    let rhs_val = read_reg!(thread, rhs);
                    let result = match (lhs_val, rhs_val) {
                        (Value::Int32(l), Value::Int32(r)) => l < r,
                        (Value::Int64(l), Value::Int64(r)) => l < r,
                        (Value::Uint32(l), Value::Uint32(r)) => l < r,
                        (Value::Uint64(l), Value::Uint64(r)) => l < r,
                        _ => {
                            sync_frame!(thread);
                            let cmp = self
                                .compare_values(&lhs_val, &rhs_val)
                                .map_err(|e| self.make_panic(thread, e))?;
                            cmp.is_some_and(|o| o.is_lt())
                        }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(result));
                }
                Instruction::Le { dest, lhs, rhs } => {
                    let lhs_val = read_reg!(thread, lhs);
                    let rhs_val = read_reg!(thread, rhs);
                    let result = match (lhs_val, rhs_val) {
                        (Value::Int32(l), Value::Int32(r)) => l <= r,
                        (Value::Int64(l), Value::Int64(r)) => l <= r,
                        _ => {
                            sync_frame!(thread);
                            let cmp = self
                                .compare_values(&lhs_val, &rhs_val)
                                .map_err(|e| self.make_panic(thread, e))?;
                            cmp.is_some_and(|o| o.is_le())
                        }
                    };
                    write_prim_reg!(thread, dest, Value::Bool(result));
                }
                Instruction::Eq { dest, lhs, rhs } => {
                    let lhs_val = read_reg!(thread, lhs);
                    let rhs_val = read_reg!(thread, rhs);
                    write_prim_reg!(thread, dest, Value::Bool(lhs_val == rhs_val));
                }
                Instruction::Ne { dest, lhs, rhs } => {
                    let lhs_val = read_reg!(thread, lhs);
                    let rhs_val = read_reg!(thread, rhs);
                    write_prim_reg!(thread, dest, Value::Bool(lhs_val != rhs_val));
                }

                // ===== HOT PATH: Control Flow =====
                Instruction::Jump { offset } => {
                    pc = (pc as i32 + offset) as usize;
                    budget -= 1;
                }
                Instruction::JumpTrue { cond, offset } => {
                    let cond_val = read_reg!(thread, cond);
                    match cond_val {
                        Value::Bool(true) => {
                            pc = (pc as i32 + offset) as usize;
                        }
                        Value::Bool(false) => {}
                        x => {
                            sync_frame!(thread);
                            return Err(self.make_panic(
                                thread,
                                VmError::TypeMismatch {
                                    expected: "boolean conditional".to_string(),
                                    found: format!("{:?}", x),
                                },
                            ));
                        }
                    }
                    budget -= 1;
                }
                Instruction::JumpFalse { cond, offset } => {
                    let cond_val = read_reg!(thread, cond);
                    match cond_val {
                        Value::Bool(false) => {
                            pc = (pc as i32 + offset) as usize;
                        }
                        Value::Bool(true) => {}
                        x => {
                            sync_frame!(thread);
                            return Err(self.make_panic(
                                thread,
                                VmError::TypeMismatch {
                                    expected: "boolean conditional".to_string(),
                                    found: format!("{:?}", x),
                                },
                            ));
                        }
                    }
                    budget -= 1;
                }
                Instruction::JumpNull { val, offset } => {
                    let val_read = read_reg!(thread, val);
                    if matches!(val_read, Value::Null) {
                        pc = (pc as i32 + offset) as usize;
                    }
                    budget -= 1;
                }

                // ===== HOT PATH: Data Movement =====
                Instruction::LoadConst {
                    dest: _,
                    const_idx: _,
                } => {
                    sync_frame!(thread);
                    let step = self
                        .execute_data_instruction(thread, instr)
                        .map_err(|e| self.make_panic(thread, e))?;
                    reload_frame!(thread);
                    match step {
                        VmStep::Continue => {
                            budget -= 1;
                        }
                        other => return Ok(other),
                    }
                }
                Instruction::Move { dest, src } => {
                    let val = read_reg!(thread, src);
                    thread.retain_anchor_val(&val);
                    write_reg!(thread, dest, val);
                    budget -= 1;
                }
                Instruction::LoadNull { dest } => {
                    write_prim_reg!(thread, dest, Value::Null);
                }

                // ===== HOT PATH: Call (with fast intra-module path) =====
                Instruction::Call {
                    dest,
                    func: func_idx,
                    args_start,
                    arg_count,
                } => {
                    // Fast path: local function call (no import resolution)
                    if (func_idx.raw() as usize) < current_image.functions.len() {
                        let callee = unsafe {
                            current_image
                                .functions
                                .get_unchecked(func_idx.raw() as usize)
                        };
                        let register_count = callee.param_count as usize
                            + callee.local_count as usize
                            + callee.temp_count as usize;
                        let cached_instr = callee.instructions.as_slice() as *const _;

                        // Sync pc before pushing new frame
                        sync_frame!(thread);

                        let caller_base = register_base;
                        let callee_base = thread.current_register_top;
                        let new_top = callee_base + register_count;

                        let limits = thread.thread_quota().limits();
                        if thread.call_stack.len() >= limits.max_call_depth {
                            return Err(self.make_panic(
                                thread,
                                VmError::ResourceLimitExceeded(
                                    galfus_contract::ExecutionFailureKind::ResourceLimitExceeded {
                                        resource: galfus_contract::ResourceLimitKind::CallDepth,
                                        current: thread.call_stack.len(),
                                        requested: 1,
                                        limit: limits.max_call_depth,
                                    },
                                ),
                            ));
                        }

                        if new_top > thread.registers.len() {
                            thread
                                .registers
                                .resize(new_top.max(thread.registers.len() * 2), Value::Null);
                            registers_ptr = thread.registers.as_mut_ptr();
                        }

                        thread.call_stack.push(CallFrame {
                            module_id: current_module_id,
                            func_idx: *func_idx,
                            pc: 0,
                            register_base: callee_base,
                            return_dest: Some(*dest),
                            cached_instructions: cached_instr,
                            has_objects: false,
                        });

                        thread.current_register_base = callee_base;
                        thread.current_register_top = new_top;
                        thread.current_frame_has_objects = false;
                        let count = *arg_count as usize;
                        if count > 0 {
                            let src_ptr = unsafe {
                                registers_ptr.add(caller_base + args_start.raw() as usize)
                            };
                            let dst_ptr = unsafe { registers_ptr.add(callee_base) };
                            unsafe { std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, count) };

                            for i in 0..count {
                                let val_ref = unsafe { &*dst_ptr.add(i) };
                                if let Value::Object(obj_ref) = val_ref {
                                    thread.current_frame_has_objects = true;
                                    let _ = thread.heap.retain_anchor(*obj_ref);
                                }
                            }
                        }

                        // Sync local cache
                        pc = 0;
                        instructions = cached_instr;
                        register_base = callee_base;
                        budget -= 1;
                    } else {
                        // Slow path: import resolution
                        sync_frame!(thread);
                        let step = self
                            .execute_control_instruction(thread, instr)
                            .map_err(|e| self.make_panic(thread, e))?;
                        match step {
                            VmStep::Continue => {
                                reload_frame!(thread);
                                registers_ptr = thread.registers.as_mut_ptr();
                                budget -= 1;
                            }
                            other => return Ok(other),
                        }
                    }
                }
                Instruction::TailCall {
                    func: func_idx,
                    args_start,
                    arg_count,
                } => {
                    if (func_idx.raw() as usize) < current_image.functions.len() {
                        let callee = unsafe {
                            current_image
                                .functions
                                .get_unchecked(func_idx.raw() as usize)
                        };
                        let register_count = callee.param_count as usize
                            + callee.local_count as usize
                            + callee.temp_count as usize;
                        let cached_instr = callee.instructions.as_slice() as *const _;

                        let caller_base = register_base;
                        let new_top = caller_base + register_count;

                        if new_top > thread.registers.len() {
                            thread
                                .registers
                                .resize(new_top.max(thread.registers.len() * 2), Value::Null);
                            registers_ptr = thread.registers.as_mut_ptr();
                        }

                        // We must read arguments FIRST, because clearing registers will destroy them
                        let count = *arg_count as usize;
                        let mut temp_args = Vec::with_capacity(count);
                        if count > 0 {
                            let src_ptr = unsafe {
                                registers_ptr.add(caller_base + args_start.raw() as usize)
                            };
                            for i in 0..count {
                                temp_args.push(*unsafe { &*src_ptr.add(i) });
                            }
                        }

                        // Release any objects from the current frame before we overwrite it
                        if thread.current_frame_has_objects {
                            let old_top = thread.current_register_top;
                            for i in caller_base..old_top {
                                let val_ref = unsafe { &mut *registers_ptr.add(i) };
                                if let Value::Object(obj_ref) = val_ref {
                                    let _ = thread.heap.release_anchor(*obj_ref);
                                    *val_ref = Value::Null;
                                } else {
                                    *val_ref = Value::Null; // Clear primitive values too to avoid garbage
                                }
                            }
                        } else {
                            // Even if there are no objects, we should clear the registers to prevent garbage
                            let old_top = thread.current_register_top;
                            let max_clear = old_top.max(new_top);
                            for i in caller_base..max_clear {
                                unsafe { *registers_ptr.add(i) = Value::Null };
                            }
                        }

                        thread.current_frame_has_objects = false;
                        if count > 0 {
                            let dst_ptr = unsafe { registers_ptr.add(caller_base) };
                            for (i, val) in temp_args.into_iter().enumerate() {
                                if let Value::Object(obj_ref) = &val {
                                    thread.current_frame_has_objects = true;
                                    let _ = thread.heap.retain_anchor(*obj_ref);
                                }
                                unsafe { *dst_ptr.add(i) = val };
                            }
                        }

                        // Update current frame
                        thread.current_register_top = new_top;

                        let frame = unsafe { thread.call_stack.last_mut().unwrap_unchecked() };
                        frame.module_id = current_module_id;
                        frame.func_idx = *func_idx;
                        frame.pc = 0;
                        frame.cached_instructions = cached_instr;
                        frame.has_objects = thread.current_frame_has_objects;

                        // Sync local cache
                        pc = 0;
                        instructions = cached_instr;
                        budget -= 1;
                    } else {
                        // Slow path: import resolution
                        sync_frame!(thread);
                        let step = self
                            .execute_control_instruction(thread, instr)
                            .map_err(|e| self.make_panic(thread, e))?;
                        match step {
                            VmStep::Continue => {
                                reload_frame!(thread);
                                registers_ptr = thread.registers.as_mut_ptr();
                                budget -= 1;
                            }
                            other => return Ok(other),
                        }
                    }
                }
                // ===== HOT PATH: Return =====
                Instruction::Ret { src } => {
                    let val = read_reg!(thread, src);
                    thread.retain_anchor_val(&val);
                    let completed_frame = thread.call_stack.pop().unwrap();

                    if thread.current_frame_has_objects {
                        for i in completed_frame.register_base..thread.current_register_top {
                            let val = unsafe { registers_ptr.add(i).replace(Value::Null) };
                            if let Value::Object(obj_ref) = val {
                                let _ = thread.heap.release_anchor(obj_ref);
                            }
                        }
                    }

                    thread.current_register_top = completed_frame.register_base;
                    let stack_len = thread.call_stack.len();
                    thread.current_register_base = if stack_len > 0 {
                        unsafe { thread.call_stack.get_unchecked(stack_len - 1).register_base }
                    } else {
                        0
                    };
                    thread.current_frame_has_objects = completed_frame.has_objects;

                    match completed_frame.return_dest {
                        Some(dest) => {
                            // Reload base from restored frame
                            reload_frame!(thread);
                            write_reg!(thread, dest, val);
                            budget -= 1;
                        }
                        None => {
                            let return_type = self
                                .get_function(completed_frame.module_id, completed_frame.func_idx)
                                .map_err(|e| self.make_panic(thread, e))?
                                .return_ty;
                            return Ok(VmStep::Return {
                                value: val,
                                module_id: completed_frame.module_id,
                                return_type,
                            });
                        }
                    }
                }
                Instruction::RetNull => {
                    let completed_frame = thread.pop_frame().ok_or_else(|| VmPanic {
                        error: VmError::EmptyCallStack,
                        stack_trace: vec![],
                    })?;

                    match completed_frame.return_dest {
                        Some(dest) => {
                            reload_frame!(thread);
                            write_reg!(thread, dest, Value::Null);
                            budget -= 1;
                        }
                        None => {
                            let return_type = self
                                .get_function(completed_frame.module_id, completed_frame.func_idx)
                                .map_err(|e| self.make_panic(thread, e))?
                                .return_ty;
                            return Ok(VmStep::Return {
                                value: Value::Null,
                                module_id: completed_frame.module_id,
                                return_type,
                            });
                        }
                    }
                }

                // ===== COLD PATH: Delegate to existing handlers =====
                _ => {
                    // Sync state to frame before delegating
                    if let Some(f) = thread.call_stack.last_mut() {
                        f.pc = pc;
                    }

                    let step = self.step_cold(thread, instr)?;

                    match step {
                        VmStep::Continue => {
                            reload_frame!(thread);
                            budget -= 1;
                        }
                        other => return Ok(other),
                    }
                }
            }
        }

        // Budget exhausted: sync state back and yield
        sync_frame!(thread);
        Ok(VmStep::Continue)
    }

    /// Cold path dispatcher for infrequently used instructions.
    /// Called from `execute_with_budget` after syncing the local state.
    fn step_cold(
        &self,
        thread: &mut thread::VmThreadState,
        instr: &Instruction,
    ) -> Result<VmStep, VmPanic> {
        let result = match instr {
            Instruction::LoadGlobal { .. } | Instruction::StoreGlobal { .. } => {
                self.execute_data_instruction(thread, instr)
            }

            Instruction::Mul { .. }
            | Instruction::Div { .. }
            | Instruction::Rem { .. }
            | Instruction::Pow { .. }
            | Instruction::Neg { .. }
            | Instruction::Not { .. }
            | Instruction::BitNot { .. }
            | Instruction::Shl { .. }
            | Instruction::Shr { .. }
            | Instruction::And { .. }
            | Instruction::Or { .. }
            | Instruction::Xor { .. }
            | Instruction::Gt { .. }
            | Instruction::Ge { .. }
            | Instruction::Fallback { .. } => self.execute_operator_instruction(thread, instr),

            Instruction::CallMethod { .. }
            | Instruction::CallDynamic { .. }
            | Instruction::Panic { .. } => self.execute_control_instruction(thread, instr),

            Instruction::AllocLocal { .. }
            | Instruction::LoadField { .. }
            | Instruction::StoreField { .. }
            | Instruction::NewArray { .. }
            | Instruction::LoadIndex { .. }
            | Instruction::StoreIndex { .. }
            | Instruction::NewTuple { .. }
            | Instruction::NewChoice { .. }
            | Instruction::Cast { .. }
            | Instruction::Copy { .. }
            | Instruction::Instanceof { .. } => self.execute_object_instruction(thread, instr),

            Instruction::Drop { .. }
            | Instruction::AwaitFuture { .. }
            | Instruction::CreateFuture { .. }
            | Instruction::CreateIndirectFuture { .. }
            | Instruction::AwaitAll { .. }
            | Instruction::AwaitRace { .. }
            | Instruction::Len { .. }
            | Instruction::CopyArray { .. } => self.execute_system_instruction(thread, instr),

            _ => unreachable!("unknown instruction"),
        };

        result.map_err(|e| self.make_panic(thread, e))
    }

    fn make_panic(&self, thread: &thread::VmThreadState, error: VmError) -> VmPanic {
        let mut stack_trace = Vec::new();
        for frame in thread.call_stack.iter().rev() {
            stack_trace.push(StackFrameInfo {
                module_id: frame.module_id,
                func_idx: frame.func_idx,
                instruction_offset: frame.pc.saturating_sub(1),
            });
        }
        VmPanic { error, stack_trace }
    }

    pub fn step(&self, thread: &mut thread::VmThreadState) -> Result<VmStep, VmError> {
        if let Some((binding_id, type_id, id)) = thread.heap.pending_adapter_handle_drops.pop() {
            return Ok(VmStep::Suspend {
                effect: VmEffect::AdapterHandleDropped {
                    binding_id,
                    type_id,
                    id,
                },
                continuation: Continuation::new(None),
            });
        }

        let instr = {
            let frame = thread
                .call_stack
                .last_mut()
                .ok_or(VmError::EmptyCallStack)?;
            let pc = frame.pc;
            frame.pc += 1;
            let slice = unsafe { &*frame.cached_instructions };
            unsafe { slice.get_unchecked(pc) }
        };
        let step = match instr {
            Instruction::LoadConst { .. }
            | Instruction::Move { .. }
            | Instruction::LoadGlobal { .. }
            | Instruction::StoreGlobal { .. }
            | Instruction::LoadNull { .. } => self.execute_data_instruction(thread, instr)?,

            Instruction::Add { .. }
            | Instruction::Sub { .. }
            | Instruction::Mul { .. }
            | Instruction::Div { .. }
            | Instruction::Rem { .. }
            | Instruction::Pow { .. }
            | Instruction::Neg { .. }
            | Instruction::Not { .. }
            | Instruction::BitNot { .. }
            | Instruction::Shl { .. }
            | Instruction::Shr { .. }
            | Instruction::And { .. }
            | Instruction::Or { .. }
            | Instruction::Xor { .. }
            | Instruction::Eq { .. }
            | Instruction::Ne { .. }
            | Instruction::Lt { .. }
            | Instruction::Le { .. }
            | Instruction::Gt { .. }
            | Instruction::Ge { .. }
            | Instruction::Fallback { .. } => self.execute_operator_instruction(thread, instr)?,

            Instruction::Jump { .. }
            | Instruction::JumpTrue { .. }
            | Instruction::JumpFalse { .. }
            | Instruction::JumpNull { .. }
            | Instruction::Call { .. }
            | Instruction::TailCall { .. }
            | Instruction::CallMethod { .. }
            | Instruction::CallDynamic { .. }
            | Instruction::Ret { .. }
            | Instruction::RetNull
            | Instruction::Panic { .. } => self.execute_control_instruction(thread, instr)?,

            Instruction::AllocLocal { .. }
            | Instruction::LoadField { .. }
            | Instruction::StoreField { .. }
            | Instruction::NewArray { .. }
            | Instruction::LoadIndex { .. }
            | Instruction::StoreIndex { .. }
            | Instruction::NewTuple { .. }
            | Instruction::NewChoice { .. }
            | Instruction::Cast { .. }
            | Instruction::Copy { .. }
            | Instruction::Instanceof { .. } => self.execute_object_instruction(thread, instr)?,

            Instruction::Drop { .. }
            | Instruction::AwaitFuture { .. }
            | Instruction::CreateFuture { .. }
            | Instruction::CreateIndirectFuture { .. }
            | Instruction::AwaitAll { .. }
            | Instruction::AwaitRace { .. }
            | Instruction::Len { .. }
            | Instruction::CopyArray { .. } => self.execute_system_instruction(thread, instr)?,
            _ => VmStep::Continue,
        };

        Ok(step)
    }

    fn validate_graph_format(&self) -> Result<(), VmError> {
        self.graph
            .validate_format()
            .map_err(VmError::UnsupportedBytecodeFormat)
    }

    fn execute_loop(&self, thread: &mut thread::VmThreadState) -> Result<Value, VmError> {
        loop {
            match self.step(thread)? {
                VmStep::Continue => {}
                VmStep::Return { value, .. } => return Ok(value),
                VmStep::Suspend { .. } => return Err(VmError::UnresolvedHostBlocked),
                VmStep::Failed(_) => return Err(VmError::UnresolvedHostBlocked),
            }
        }
    }
}
unsafe impl Send for VirtualMachine {}
unsafe impl Sync for VirtualMachine {}
