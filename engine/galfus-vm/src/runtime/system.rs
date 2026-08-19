use crate::thread;

use super::*;

impl VirtualMachine {
    pub(super) fn execute_system_instruction(
        &self,
        thread: &mut thread::VmThreadState,
        instr: &Instruction,
    ) -> Result<VmStep, VmError> {
        match *instr {
            // Category E: Memory Ownership
            Instruction::Drop { reg } => {
                let value = thread.read_reg(reg)?;
                thread.write_reg(reg, Value::Null)?;
                if let Value::Future(future_id) = value
                    && !thread.contains_future_handle(future_id)
                {
                    return Ok(VmStep::Suspend {
                        effect: VmEffect::FutureDropped { future_id },
                        continuation: Continuation::new(None),
                    });
                }
            }

            Instruction::AwaitFuture {
                dest,
                future_id,
                return_type,
            } => {
                let val = thread.read_reg(future_id)?;
                let Value::Future(future_id) = val else {
                    return Err(VmError::TypeMismatch {
                        expected: "Future<T>".to_string(),
                        found: format!("{val:?}"),
                    });
                };
                let module_id = thread
                    .call_stack
                    .last()
                    .ok_or(VmError::EmptyCallStack)?
                    .module_id;
                return Ok(VmStep::Suspend {
                    effect: VmEffect::FutureWait {
                        future_id,
                        module_id,
                        return_type,
                    },
                    continuation: Continuation::for_provider(dest, module_id, return_type),
                });
            }
            Instruction::Len { dest, src } => {
                let val = thread.read_reg(src)?;
                if let Value::Object(obj_ref) = val {
                    let heap_obj = thread.heap.get_object(obj_ref)?;
                    match heap_obj {
                        HeapObject::Array { elements, .. } => {
                            thread.write_reg(dest, Value::Int32(elements.len() as i32))?;
                        }
                        HeapObject::Tuple { elements, .. } => {
                            thread.write_reg(dest, Value::Int32(elements.len() as i32))?;
                        }
                        _ => {
                            return Err(VmError::TypeMismatch {
                                expected: "Array or Tuple object".to_string(),
                                found: format!("{:?}", heap_obj),
                            });
                        }
                    }
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Object reference".to_string(),
                        found: format!("{:?}", val),
                    });
                }
            }
            Instruction::CopyArray {
                dest,
                dest_start,
                src,
            } => {
                let dest_val = thread.read_reg(dest)?;
                let start_val = thread.read_reg(dest_start)?;
                let src_val = thread.read_reg(src)?;

                let start_idx = match start_val {
                    Value::Int8(x) if x >= 0 => x as usize,
                    Value::Int16(x) if x >= 0 => x as usize,
                    Value::Int32(x) if x >= 0 => x as usize,
                    Value::Int64(x) if x >= 0 => x as usize,
                    Value::Uint8(x) => x as usize,
                    Value::Uint16(x) => x as usize,
                    Value::Uint32(x) => x as usize,
                    Value::Uint64(x) => x as usize,
                    x => {
                        return Err(VmError::TypeMismatch {
                            expected: "positive index".to_string(),
                            found: format!("{:?}", x),
                        });
                    }
                };

                if let (Value::Object(dest_ref), Value::Object(src_ref)) =
                    (dest_val.clone(), src_val.clone())
                {
                    let src_elements = match thread.heap.get_object(src_ref)? {
                        HeapObject::Array { elements, .. } => elements.clone(),
                        HeapObject::Tuple { elements, .. } => elements.clone(),
                        other => {
                            return Err(VmError::TypeMismatch {
                                expected: "Array or Tuple".to_string(),
                                found: format!("{:?}", other),
                            });
                        }
                    };

                    let dest_obj = thread.heap.get_object_mut(dest_ref)?;
                    if let HeapObject::Array { elements, .. } = dest_obj {
                        if start_idx + src_elements.len() > elements.len() {
                            return Err(VmError::IndexOutOfBounds {
                                index: (start_idx + src_elements.len() - 1) as i128,
                                len: elements.len(),
                            });
                        }
                        for (i, elem) in src_elements.into_iter().enumerate() {
                            elements[start_idx + i] = elem;
                        }
                    } else {
                        return Err(VmError::TypeMismatch {
                            expected: "Array object".to_string(),
                            found: format!("{:?}", dest_obj),
                        });
                    }
                } else {
                    return Err(VmError::TypeMismatch {
                        expected: "Object references for source and destination".to_string(),
                        found: format!("{:?}, {:?}", dest_val, src_val),
                    });
                }
            }
            Instruction::CreateFuture {
                dest,
                func: func_idx,
                args_start,
                arg_count,
                ref arg_types,
                return_type,
            } => {
                let mut args = Vec::with_capacity(arg_count as usize);
                for i in 0..arg_count {
                    let val = thread.read_reg(Reg(args_start.raw() + i as u16))?;
                    thread.retain_anchor_val(&val);
                    args.push(val);
                }
                let module_id = thread
                    .call_stack
                    .last()
                    .ok_or(VmError::EmptyCallStack)?
                    .module_id;
                let current_image = self.get_module(module_id)?;
                let (target_module_id, func_idx) =
                    if (func_idx.raw() as usize) < current_image.functions.len() {
                        (module_id, func_idx)
                    } else {
                        let import_idx = (func_idx.raw() as usize) - current_image.functions.len();
                        let link = self
                            .graph
                            .resolve_imports(module_id)
                            .map_err(|_| VmError::FunctionOutOfBounds { index: func_idx })?;
                        let import = link
                            .imports
                            .get(import_idx)
                            .ok_or(VmError::FunctionOutOfBounds { index: func_idx })?;
                        let target_func_idx = match &import.kind {
                            galfus_bytecode::graph_resolver::ResolvedImportKind::Function(
                                index,
                            ) => *index,
                            _ => return Err(VmError::FunctionOutOfBounds { index: func_idx }),
                        };
                        (import.module_id, target_func_idx)
                    };
                return Ok(VmStep::Suspend {
                    effect: VmEffect::CreateFuture {
                        module_id,
                        target_module_id,
                        func_idx,
                        args,
                        arg_types: arg_types.clone(),
                        return_type,
                    },
                    continuation: Continuation::for_future_handle(dest),
                });
            }
            Instruction::CreateIndirectFuture {
                dest,
                func_reg,
                args_start,
                arg_count,
                ref arg_types,
                return_type,
            } => {
                let func_val = thread.read_reg(func_reg)?;
                let mut args = Vec::with_capacity(arg_count as usize);
                for i in 0..arg_count {
                    let val = thread.read_reg(Reg(args_start.raw() + i as u16))?;
                    thread.retain_anchor_val(&val);
                    args.push(val);
                }
                let module_id = thread
                    .call_stack
                    .last()
                    .ok_or(VmError::EmptyCallStack)?
                    .module_id;
                return Ok(VmStep::Suspend {
                    effect: VmEffect::CreateIndirectFuture {
                        module_id,
                        func: func_val,
                        args,
                        arg_types: arg_types.clone(),
                        return_type,
                    },
                    continuation: Continuation::for_future_handle(dest),
                });
            }
            Instruction::AwaitAll {
                dest,
                futures_start,
                count,
                return_type,
            } => {
                let future_ids = (0..count)
                    .map(|index| {
                        let val = thread.read_reg(Reg(futures_start.raw() + index as u16))?;
                        match val {
                            Value::Future(id) => Ok(id),
                            value => Err(VmError::TypeMismatch {
                                expected: "Future<T>".to_string(),
                                found: format!("{value:?}"),
                            }),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let module_id = thread
                    .call_stack
                    .last()
                    .ok_or(VmError::EmptyCallStack)?
                    .module_id;
                return Ok(VmStep::Suspend {
                    effect: VmEffect::FutureWaitAll {
                        future_ids,
                        module_id,
                        return_type,
                    },
                    continuation: Continuation::for_provider(dest, module_id, return_type),
                });
            }
            Instruction::AwaitRace {
                dest,
                futures_start,
                count,
                return_type,
            } => {
                let future_ids = (0..count)
                    .map(|index| {
                        let val = thread.read_reg(Reg(futures_start.raw() + index as u16))?;
                        match val {
                            Value::Future(id) => Ok(id),
                            value => Err(VmError::TypeMismatch {
                                expected: "Future<T>".to_string(),
                                found: format!("{value:?}"),
                            }),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let module_id = thread
                    .call_stack
                    .last()
                    .ok_or(VmError::EmptyCallStack)?
                    .module_id;
                return Ok(VmStep::Suspend {
                    effect: VmEffect::FutureWaitRace {
                        future_ids,
                        module_id,
                        return_type,
                    },
                    continuation: Continuation::for_provider(dest, module_id, return_type),
                });
            }
            _ => unreachable!("instruction routed to the wrong runtime handler"),
        }

        Ok(VmStep::Continue)
    }
}
