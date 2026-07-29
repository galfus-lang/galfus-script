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
use galfus_core::ModuleId;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    pub dest: Option<Reg>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmEffect {
    SendMsg {
        target: u64,
        msg: Value,
    },
    ReceiveFilter {
        sender_id: u64,
        timeout: Option<u64>,
    },
    CreateThread {
        func: Value,
        key: Value,
    },
    StartThread {
        thread_id: u64,
        arg: Value,
    },
    GetThread {
        key: Value,
    },
    ThreadIsRunning {
        thread_id: u64,
    },
    ThreadIsExited {
        thread_id: u64,
    },
    ThreadExitReason {
        thread_id: u64,
    },
    Blocked,
}

pub enum VmStep {
    Continue,
    Return(Value),
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
        thread: &thread::VirtualThread,
    ) -> Result<&galfus_bytecode::BytecodeModule, VmError> {
        let frame = thread.call_stack.last().ok_or(VmError::EmptyCallStack)?;
        Ok(&self.graph.get(frame.module_id).unwrap().module)
    }

    pub fn prepare_function(
        &self,
        thread: &mut thread::VirtualThread,
        module_id: galfus_core::ModuleId,
        func_idx: FuncIdx,
        args: Vec<Value>,
    ) -> Result<(), VmPanic> {
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
        thread: &mut thread::VirtualThread,
        module_id: galfus_core::ModuleId,
        func_idx: FuncIdx,
        args: Vec<Value>,
    ) -> Result<Value, VmPanic> {
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
        thread: &mut thread::VirtualThread,
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

    pub fn step(&self, thread: &mut thread::VirtualThread) -> Result<VmStep, VmError> {
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
            let instr = func.instructions[frame.pc];
            frame.pc += 1;
            instr
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
            | Instruction::CallMethod { .. }
            | Instruction::CallDynamic { .. }
            | Instruction::Ret { .. }
            | Instruction::RetNull
            | Instruction::Send { .. }
            | Instruction::ReceiveFilter { .. }
            | Instruction::MailboxHasMessages { .. }
            | Instruction::MailboxGetMessage { .. }
            | Instruction::CreateThread { .. }
            | Instruction::StartThread { .. }
            | Instruction::GetThread { .. }
            | Instruction::ThreadIsRunning { .. }
            | Instruction::ThreadIsExited { .. }
            | Instruction::ThreadExitReason { .. }
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
            | Instruction::CallNative { .. }
            | Instruction::Len { .. }
            | Instruction::CopyArray { .. } => self.execute_system_instruction(thread, instr)?,
        };

        if matches!(step, VmStep::Continue) {
            self.release_unreachable_if_needed(thread, instr);
        }

        Ok(step)
    }

    fn execute_loop(&self, thread: &mut thread::VirtualThread) -> Result<Value, VmError> {
        loop {
            match self.step(thread)? {
                VmStep::Continue => {}
                VmStep::Return(value) => return Ok(value),
                VmStep::Suspend { .. } => return Err(VmError::UnresolvedHostBlocked),
                VmStep::Failed(_) => return Err(VmError::UnresolvedHostBlocked),
            }
        }
    }

    fn release_unreachable_if_needed(
        &self,
        thread: &mut thread::VirtualThread,
        instr: Instruction,
    ) {
        if matches!(instr, Instruction::Drop { .. })
            || thread.heap.allocations_since_release >= RELEASE_ALLOCATION_THRESHOLD
        {
            self.release_unreachable(thread);
        }
    }
}
