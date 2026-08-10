use crate::thread;

use super::*;

impl VirtualMachine {
    pub(super) fn execute_data_instruction(
        &self,
        thread: &mut thread::VmThreadState,
        instr: Instruction,
    ) -> Result<VmStep, VmError> {
        match instr {
            // Category A: Data Movement & Constants
            Instruction::LoadConst { dest, const_idx } => {
                let constant = self
                    .current_image(thread)?
                    .constants
                    .constants
                    .get(const_idx.raw() as usize)
                    .ok_or(VmError::ConstantOutOfBounds { index: const_idx })?;
                let val = match constant {
                    Constant::Bool(b) => Value::Bool(*b),
                    Constant::Int8(i) => Value::Int8(*i),
                    Constant::Int16(i) => Value::Int16(*i),
                    Constant::Int32(i) => Value::Int32(*i),
                    Constant::Int64(i) => Value::Int64(*i),
                    Constant::Uint8(i) => Value::Uint8(*i),
                    Constant::Uint16(i) => Value::Uint16(*i),
                    Constant::Uint32(i) => Value::Uint32(*i),
                    Constant::Uint64(i) => Value::Uint64(*i),
                    Constant::Float32(f) => Value::Float32(galfus_core::normalize_f32(*f)),
                    Constant::Float64(f) => Value::Float64(galfus_core::normalize_f64(*f)),
                    Constant::String(s) => {
                        let element_ty = self.uint8_type_idx(thread)?;
                        let obj = HeapObject::Array {
                            element_ty,
                            elements: s.bytes().map(Value::Uint8).collect(),
                        };
                        Value::Object(thread.heap.alloc(obj))
                    }
                    Constant::Bytes(b) => {
                        let element_ty = self.uint8_type_idx(thread)?;
                        let obj = HeapObject::Array {
                            element_ty,
                            elements: b.iter().map(|&x| Value::Uint8(x)).collect(),
                        };
                        Value::Object(thread.heap.alloc(obj))
                    }
                    Constant::Function(func_idx) => {
                        let module_id = thread
                            .call_stack
                            .last()
                            .ok_or(VmError::EmptyCallStack)?
                            .module_id;
                        Value::Function {
                            module_id,
                            func_idx: *func_idx,
                        }
                    }
                };
                thread.write_reg(dest, val)?;
            }
            Instruction::Move { dest, src } => {
                let val = thread.read_reg(src)?;
                thread.write_reg(dest, val)?;
            }
            Instruction::LoadGlobal {
                dest,
                module_id,
                global_idx,
            } => {
                let module = self.get_module(module_id)?;
                if global_idx.raw() as usize >= module.global_count as usize {
                    return Err(VmError::GlobalOutOfBounds { index: global_idx });
                }
                let val = thread
                    .module_states
                    .get(&module_id)
                    .and_then(|state| state.globals.get(global_idx.raw() as usize))
                    .cloned()
                    .unwrap_or(Value::Null);
                thread.write_reg(dest, val)?;
            }
            Instruction::StoreGlobal {
                module_id,
                global_idx,
                src,
            } => {
                let module = self.get_module(module_id)?;
                if global_idx.raw() as usize >= module.global_count as usize {
                    return Err(VmError::GlobalOutOfBounds { index: global_idx });
                }
                let val = thread.read_reg(src)?;
                let idx = global_idx.raw() as usize;
                let globals = &mut thread.module_states.entry(module_id).or_default().globals;
                if idx >= globals.len() {
                    globals.resize(idx + 1, Value::Null);
                }
                globals[idx] = val;
            }
            Instruction::LoadNull { dest } => {
                thread.write_reg(dest, Value::Null)?;
            }

            _ => unreachable!("instruction routed to the wrong runtime handler"),
        }

        Ok(VmStep::Continue)
    }

    pub(super) fn uint8_type_idx(
        &self,
        thread: &thread::VmThreadState,
    ) -> Result<TypeIdx, VmError> {
        self.current_image(thread)?
            .types
            .iter()
            .position(|ty| matches!(ty, BytecodeType::Uint8))
            .map(|idx| TypeIdx(idx as u16))
            .ok_or(VmError::InvalidModule)
    }
}
