mod casts;
mod control;
mod data;
mod graph_release;
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
    pub origin_thread_id: Option<u64>,
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

    pub fn with_origin(mut self, origin: u64) -> Self {
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
        future_id: u64,
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
        arg_types: Vec<TypeIdx>,
        return_type: TypeIdx,
    },
    CreateIndirectFuture {
        module_id: ModuleId,
        func: Value,
        args: Vec<Value>,
        arg_types: Vec<TypeIdx>,
        return_type: TypeIdx,
    },
    FutureDropped {
        future_id: u64,
    },
    AdapterHandleDropped {
        binding_id: BindingId,
        type_id: OpaqueTypeId,
        id: HandleId,
    },
    FutureWaitAll {
        future_ids: Vec<u64>,
        module_id: ModuleId,
        return_type: TypeIdx,
    },
    FutureWaitRace {
        future_ids: Vec<u64>,
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
pub struct VmObjectRef(pub usize);

impl VmObjectRef {
    pub const fn raw(&self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    Future(u64),
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

const RELEASE_ALLOCATION_THRESHOLD: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum HeapObject {
    Struct {
        module_id: galfus_core::ModuleId,
        layout_idx: StructLayoutIdx,
        fields: Vec<Value>,
    },
    Array {
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
    pub registers: Vec<Value>,
    pub return_dest: Option<Reg>,
}

#[derive(Clone)]
pub struct VirtualMachine {
    pub graph: Arc<BytecodeGraph>,
    pub context: VmContext,
}

impl VirtualMachine {
    /// Resumes a suspended VM operation exactly once without exposing register layout.
    #[allow(clippy::result_large_err)]
    pub fn resume(
        &self,
        thread_id: u64,
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
            && !self.value_matches_type(thread, value.clone(), module_id, expected_type)
        {
            return Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InvalidContinuation,
                "continuation result does not match its declared type",
            ));
        }
        if let Some(dest) = continuation.dest {
            thread.write_reg(dest, value).map_err(|error| {
                galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InvalidContinuation,
                    error.to_string(),
                )
            })?;
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
                    element_ty,
                    elements,
                }) = thread.heap.get_object(reference)
                else {
                    return false;
                };
                element_ty == element_type
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
                        self.value_matches_type(thread, payload.clone(), module_id, type_idx)
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
        Self {
            graph,
            context: VmContext::new(None),
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

    pub fn current_image(
        &self,
        thread: &thread::VmThreadState,
    ) -> Result<&galfus_bytecode::BytecodeModule, VmError> {
        let frame = thread.call_stack.last().ok_or(VmError::EmptyCallStack)?;
        Ok(&self.graph.get(frame.module_id).unwrap().module)
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

        if (func_idx.raw() as usize) >= self.graph.get(module_id).unwrap().module.functions.len() {
            return Err(VmPanic {
                error: VmError::FunctionOutOfBounds { index: func_idx },
                stack_trace: vec![],
            });
        }

        let func = &self.graph.get(module_id).unwrap().module.functions[func_idx.raw() as usize];
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
        let total_regs =
            func.param_count as usize + func.local_count as usize + func.temp_count as usize;
        let mut registers = vec![Value::Null; total_regs];
        for (i, val) in args.into_iter().enumerate() {
            registers[i] = val;
        }

        thread.call_stack.push(CallFrame {
            module_id,
            func_idx,
            pc: 0,
            registers,
            return_dest: None,
        });

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

        if (func_idx.raw() as usize) >= self.graph.get(module_id).unwrap().module.functions.len() {
            return Err(VmPanic {
                error: VmError::FunctionOutOfBounds { index: func_idx },
                stack_trace: vec![],
            });
        }

        let func = &self.graph.get(module_id).unwrap().module.functions[func_idx.raw() as usize];
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
        let total_regs =
            func.param_count as usize + func.local_count as usize + func.temp_count as usize;
        let mut registers = vec![Value::Null; total_regs];
        for (i, val) in args.into_iter().enumerate() {
            registers[i] = val;
        }

        thread.call_stack.push(CallFrame {
            module_id,
            func_idx,
            pc: 0,
            registers,
            return_dest: None,
        });

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
        while budget > 0 {
            match self.step(thread) {
                Ok(VmStep::Continue) => budget -= 1,
                Ok(step) => return Ok(step),
                Err(err) => {
                    let mut stack_trace = Vec::new();
                    for frame in thread.call_stack.iter().rev() {
                        stack_trace.push(StackFrameInfo {
                            module_id: frame.module_id,
                            func_idx: frame.func_idx,
                            instruction_offset: frame.pc.saturating_sub(1),
                        });
                    }
                    return Err(VmPanic {
                        error: err,
                        stack_trace,
                    });
                }
            }
        }
        Ok(VmStep::Continue)
    }

    pub fn step(&self, thread: &mut thread::VmThreadState) -> Result<VmStep, VmError> {
        self.validate_graph_format()?;

        if let Some((binding_id, type_id, id)) = thread.pending_adapter_handle_drops.pop() {
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
            let func = &self.graph.get(frame.module_id).unwrap().module.functions
                [frame.func_idx.raw() as usize];
            if frame.pc >= func.instructions.len() {
                return Err(VmError::InstructionPointerOutOfBounds { pc: frame.pc });
            }
            let instr = func.instructions[frame.pc].clone();
            frame.pc += 1;
            instr
        };

        let release_instruction = instr.clone();
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
        };

        if matches!(step, VmStep::Continue) {
            self.release_unreachable_if_needed(thread, release_instruction);
        }

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

    fn release_unreachable_if_needed(
        &self,
        thread: &mut thread::VmThreadState,
        instr: Instruction,
    ) {
        if matches!(instr, Instruction::Drop { .. })
            || thread.heap.allocations_since_release >= RELEASE_ALLOCATION_THRESHOLD
        {
            let released_handles = self.release_unreachable(thread);
            thread.pending_adapter_handle_drops.extend(released_handles);
        }
    }
}
