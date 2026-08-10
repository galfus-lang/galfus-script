use crate::thread;

use super::*;

impl VirtualMachine {
    pub(super) fn execute_control_instruction(
        &self,
        thread: &mut thread::VmThreadState,
        instr: Instruction,
    ) -> Result<VmStep, VmError> {
        match instr {
            // Category C: Control Flow & Subroutines
            Instruction::Jump { offset } => {
                let frame = thread
                    .call_stack
                    .last_mut()
                    .ok_or(VmError::EmptyCallStack)?;
                let new_pc = (frame.pc as i32 + offset) as usize;
                frame.pc = new_pc;
            }
            Instruction::JumpTrue { cond, offset } => {
                let cond_val = thread.read_reg(cond)?;
                match cond_val {
                    Value::Bool(b) => {
                        if b {
                            let frame = thread
                                .call_stack
                                .last_mut()
                                .ok_or(VmError::EmptyCallStack)?;
                            let new_pc = (frame.pc as i32 + offset) as usize;
                            frame.pc = new_pc;
                        }
                    }
                    x => {
                        return Err(VmError::TypeMismatch {
                            expected: "boolean conditional".to_string(),
                            found: format!("{:?}", x),
                        });
                    }
                }
            }
            Instruction::JumpFalse { cond, offset } => {
                let cond_val = thread.read_reg(cond)?;
                match cond_val {
                    Value::Bool(b) => {
                        if !b {
                            let frame = thread
                                .call_stack
                                .last_mut()
                                .ok_or(VmError::EmptyCallStack)?;
                            let new_pc = (frame.pc as i32 + offset) as usize;
                            frame.pc = new_pc;
                        }
                    }
                    x => {
                        return Err(VmError::TypeMismatch {
                            expected: "boolean conditional".to_string(),
                            found: format!("{:?}", x),
                        });
                    }
                }
            }
            Instruction::JumpNull { val, offset } => {
                let val_read = thread.read_reg(val)?;
                if matches!(val_read, Value::Null) {
                    let frame = thread
                        .call_stack
                        .last_mut()
                        .ok_or(VmError::EmptyCallStack)?;
                    let new_pc = (frame.pc as i32 + offset) as usize;
                    frame.pc = new_pc;
                }
            }
            Instruction::Call {
                dest,
                func: func_idx,
                args_start,
                arg_count,
            } => {
                let frame = thread.call_stack.last().ok_or(VmError::EmptyCallStack)?;
                let current_module_id = frame.module_id;
                let current_image = self.get_module(current_module_id)?;

                let (target_module_id, target_func_idx) =
                    if (func_idx.raw() as usize) < current_image.functions.len() {
                        (current_module_id, func_idx)
                    } else {
                        let import_idx = (func_idx.raw() as usize) - current_image.functions.len();
                        let link = self
                            .graph
                            .resolve_imports(current_module_id)
                            .map_err(|_| VmError::FunctionOutOfBounds { index: func_idx })?;
                        let import = link
                            .imports
                            .get(import_idx)
                            .ok_or(VmError::FunctionOutOfBounds { index: func_idx })?;
                        let func = match &import.kind {
                            galfus_bytecode::graph_resolver::ResolvedImportKind::Function(f) => *f,
                            _ => return Err(VmError::FunctionOutOfBounds { index: func_idx }),
                        };
                        (import.module_id, func)
                    };

                let target_image = self.get_module(target_module_id)?;
                let callee = target_image
                    .functions
                    .get(target_func_idx.raw() as usize)
                    .ok_or(VmError::FunctionOutOfBounds {
                        index: target_func_idx,
                    })?;

                if arg_count != callee.param_count {
                    return Err(VmError::TypeMismatch {
                        expected: format!("{} arguments", callee.param_count),
                        found: format!("{} arguments", arg_count),
                    });
                }

                let mut callee_regs = vec![
                    Value::Null;
                    callee.param_count as usize
                        + callee.local_count as usize
                        + callee.temp_count as usize
                ];

                for (i, dest) in callee_regs.iter_mut().enumerate().take(arg_count as usize) {
                    let src_reg = Reg(args_start.raw() + i as u16);
                    let val = thread.read_reg(src_reg)?;
                    *dest = val;
                }

                // Save destination register inside call frame to write return value back
                thread.call_stack.push(CallFrame {
                    module_id: target_module_id,
                    func_idx: target_func_idx,
                    pc: 0,
                    registers: callee_regs,
                    return_dest: Some(dest),
                });
            }
            Instruction::CallMethod {
                dest,
                obj,
                name_const,
                args_start,
                arg_count,
            } => {
                // Resolve method name from constant pool.
                let method_name = match self
                    .current_image(thread)?
                    .constants
                    .constants
                    .get(name_const.raw() as usize)
                    .ok_or(VmError::ConstantOutOfBounds { index: name_const })?
                {
                    Constant::String(s) => s.clone(),
                    other => {
                        return Err(VmError::TypeMismatch {
                            expected: "string constant for method name".to_string(),
                            found: format!("{:?}", other),
                        });
                    }
                };

                if let Some(value) =
                    self.execute_array_iterator_method(thread, obj, method_name.as_str())?
                {
                    thread.write_reg(dest, value)?;
                    return Ok(VmStep::Continue);
                }

                if method_name == "compare" {
                    let obj_val = thread.read_reg(obj)?;
                    if !matches!(obj_val, Value::Object(_)) {
                        let arg_val = thread.read_reg(Reg(args_start.raw() + 1))?;
                        let is_equal = obj_val == arg_val;
                        thread.write_reg(dest, Value::Bool(is_equal))?;
                        return Ok(VmStep::Continue);
                    }
                }

                let receiver_layout = match thread.read_reg(obj)? {
                    Value::Object(obj_ref) => match thread.heap.get_object(obj_ref)? {
                        HeapObject::Struct {
                            module_id,
                            layout_idx,
                            ..
                        } => self
                            .get_module(*module_id)?
                            .struct_layouts
                            .get(layout_idx.raw() as usize)
                            .map(|layout| (*module_id, layout.name.clone())),
                        _ => None,
                    },
                    _ => None,
                };
                let qualified_name = receiver_layout
                    .as_ref()
                    .map(|(_, layout_name)| format!("{layout_name}::{method_name}"));

                let mut resolved_target = None;

                let check_name = |name: &str, target_name: &str, is_qualified: bool| -> bool {
                    if is_qualified {
                        let clean_name = if let Some(start) = name.find('<') {
                            if let Some(end) = name.find(">::") {
                                format!("{}::{}", &name[..start], &name[end + 3..])
                            } else {
                                name.to_string()
                            }
                        } else {
                            name.to_string()
                        };
                        clean_name == target_name
                            || clean_name.starts_with(&format!("{}#", target_name))
                    } else {
                        name == target_name || name.ends_with(&format!("::{}", target_name))
                    }
                };

                let current_module_id = thread.call_stack.last().ok_or(VmError::EmptyCallStack)?.module_id;
                let resolution_module_id = receiver_layout
                    .as_ref()
                    .map(|(module_id, _)| *module_id)
                    .unwrap_or(current_module_id);
                let resolution_image = self.get_module(resolution_module_id)?;

                // 1. Search in the receiver's module, or the current module for primitives.
                if let Some(qualified_name) = &qualified_name
                    && let Some(index) = resolution_image
                        .functions
                        .iter()
                        .position(|f| check_name(&f.name, qualified_name, true))
                {
                    resolved_target = Some((resolution_module_id, FuncIdx(index as u16)));
                }
                if resolved_target.is_none()
                    && let Some(index) = resolution_image
                        .functions
                        .iter()
                        .position(|f| check_name(&f.name, &method_name, false))
                {
                    resolved_target = Some((resolution_module_id, FuncIdx(index as u16)));
                }

                // 2. Search in imports
                if resolved_target.is_none()
                    && let Ok(link) = self.graph.resolve_imports(resolution_module_id)
                {
                    for imp in &link.imports {
                        let target_func_idx = match &imp.kind {
                            galfus_bytecode::graph_resolver::ResolvedImportKind::Function(f) => *f,
                            _ => continue,
                        };
                        let target_image = self.get_module(imp.module_id)?;
                        let target_func = &target_image.functions[target_func_idx.raw() as usize];
                        let matched = if let Some(qualified_name) = &qualified_name {
                            check_name(&target_func.name, qualified_name, true)
                        } else {
                            check_name(&target_func.name, &method_name, false)
                        };
                        if matched {
                            resolved_target = Some((imp.module_id, target_func_idx));
                            break;
                        }
                    }
                }

                let (target_module_id, target_func_idx) = resolved_target.ok_or_else(|| {
                    let mut available = resolution_image
                        .functions
                        .iter()
                        .map(|f| f.name.clone())
                        .collect::<Vec<_>>();
                    if let Ok(link) = self.graph.resolve_imports(resolution_module_id) {
                        for imp in &link.imports {
                            let target_func_idx = match &imp.kind {
                                galfus_bytecode::graph_resolver::ResolvedImportKind::Function(
                                    f,
                                ) => *f,
                                _ => continue,
                            };
                            let Ok(target_image) = self.get_module(imp.module_id) else { continue; };
                            available.push(
                                target_image.functions[target_func_idx.raw() as usize]
                                    .name
                                    .clone(),
                            );
                        }
                    }
                    VmError::TypeMismatch {
                        expected: format!(
                            "function named '{}'",
                            qualified_name.as_ref().unwrap_or(&method_name)
                        ),
                        found: format!(
                            "no matching function in module. Available: {}",
                            available.join(", ")
                        ),
                    }
                })?;

                let target_image = self.get_module(target_module_id)?;
                let callee = &target_image.functions[target_func_idx.raw() as usize];

                if arg_count > callee.param_count {
                    return Err(VmError::TypeMismatch {
                        expected: format!("{} arguments", callee.param_count),
                        found: format!("{} arguments", arg_count),
                    });
                }

                let mut callee_regs = vec![
                    Value::Null;
                    callee.param_count as usize
                        + callee.local_count as usize
                        + callee.temp_count as usize
                ];

                for (i, dest) in callee_regs.iter_mut().enumerate().take(arg_count as usize) {
                    // First arg is obj, then args_start + 1, +2, ...
                    let src_reg = if i == 0 {
                        obj
                    } else {
                        Reg(args_start.raw() + i as u16)
                    };
                    *dest = thread.read_reg(src_reg)?;
                }

                thread.call_stack.push(CallFrame {
                    module_id: target_module_id,
                    func_idx: target_func_idx,
                    pc: 0,
                    registers: callee_regs,
                    return_dest: Some(dest),
                });
            }
            Instruction::CallDynamic {
                dest,
                func_reg,
                args_start,
                arg_count,
            } => {
                let (target_module_id, func_idx) = match thread.read_reg(func_reg)? {
                    Value::Function {
                        module_id,
                        func_idx,
                    } => (module_id, func_idx),
                    value => {
                        return Err(VmError::TypeMismatch {
                            expected: "function value".to_string(),
                            found: format!("{:?}", value),
                        });
                    }
                };

                let current_image = self.get_module(target_module_id)?;

                let (target_module_id, target_func_idx) =
                    if (func_idx.raw() as usize) < current_image.functions.len() {
                        (target_module_id, func_idx)
                    } else {
                        let import_idx = (func_idx.raw() as usize) - current_image.functions.len();
                        let link = self
                            .graph
                            .resolve_imports(target_module_id)
                            .map_err(|_| VmError::FunctionOutOfBounds { index: func_idx })?;
                        let import = link
                            .imports
                            .get(import_idx)
                            .ok_or(VmError::FunctionOutOfBounds { index: func_idx })?;
                        let func = match &import.kind {
                            galfus_bytecode::graph_resolver::ResolvedImportKind::Function(f) => *f,
                            _ => return Err(VmError::FunctionOutOfBounds { index: func_idx }),
                        };
                        (import.module_id, func)
                    };

                let target_image = self.get_module(target_module_id)?;
                let callee = target_image
                    .functions
                    .get(target_func_idx.raw() as usize)
                    .ok_or(VmError::FunctionOutOfBounds {
                        index: target_func_idx,
                    })?;

                if arg_count != callee.param_count {
                    return Err(VmError::TypeMismatch {
                        expected: format!("{} arguments", callee.param_count),
                        found: format!("{} arguments", arg_count),
                    });
                }

                let mut callee_regs = vec![
                    Value::Null;
                    callee.param_count as usize
                        + callee.local_count as usize
                        + callee.temp_count as usize
                ];
                for (index, callee_reg) in
                    callee_regs.iter_mut().enumerate().take(arg_count as usize)
                {
                    let source = Reg(args_start.raw() + index as u16);
                    *callee_reg = thread.read_reg(source)?;
                }

                thread.call_stack.push(CallFrame {
                    module_id: target_module_id,
                    func_idx: target_func_idx,
                    pc: 0,
                    registers: callee_regs,
                    return_dest: Some(dest),
                });
            }

            Instruction::Ret { src } => {
                let val = thread.read_reg(src)?;
                let completed_frame = thread.call_stack.pop().ok_or(VmError::EmptyCallStack)?;

                match completed_frame.return_dest {
                    Some(dest) => {
                        thread.write_reg(dest, val)?;
                    }
                    None => {
                        let return_type = self
                            .graph
                            .get(completed_frame.module_id)
                            .expect("call frame module is loaded")
                            .module
                            .functions[completed_frame.func_idx.raw() as usize]
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
                let completed_frame = thread.call_stack.pop().ok_or(VmError::EmptyCallStack)?;

                match completed_frame.return_dest {
                    Some(dest) => {
                        thread.write_reg(dest, Value::Null)?;
                    }
                    None => {
                        let return_type = self
                            .graph
                            .get(completed_frame.module_id)
                            .expect("call frame module is loaded")
                            .module
                            .functions[completed_frame.func_idx.raw() as usize]
                            .return_ty;
                        return Ok(VmStep::Return {
                            value: Value::Null,
                            module_id: completed_frame.module_id,
                            return_type,
                        });
                    }
                }
            }
            Instruction::Panic { const_idx } => {
                let constant = self
                    .current_image(thread)?
                    .constants
                    .constants
                    .get(const_idx.raw() as usize)
                    .ok_or(VmError::ConstantOutOfBounds { index: const_idx })?;
                let msg = match constant {
                    Constant::String(s) => s.clone(),
                    Constant::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
                    x => format!("{:?}", x),
                };
                return Err(VmError::Panic { message: msg });
            }

            _ => unreachable!("instruction routed to the wrong runtime handler"),
        }

        Ok(VmStep::Continue)
    }

    fn execute_array_iterator_method(
        &self,
        thread: &mut thread::VmThreadState,
        obj: Reg,
        method_name: &str,
    ) -> Result<Option<Value>, VmError> {
        let Value::Object(iterator_ref) = thread.read_reg(obj)? else {
            return Ok(None);
        };

        let (array_ref, current) = match thread.heap.get_object(iterator_ref)? {
            HeapObject::Struct {
                module_id,
                layout_idx,
                fields,
            } => {
                let Some(layout) = self
                    .get_module(*module_id)?
                    .struct_layouts
                    .get(layout_idx.raw() as usize)
                else {
                    return Ok(None);
                };
                if layout.name != "ArrayIterator" {
                    return Ok(None);
                }
                let array_ref = match fields.first() {
                    Some(Value::Object(array_ref)) => *array_ref,
                    value => {
                        return Err(VmError::TypeMismatch {
                            expected: "ArrayIterator array field".to_string(),
                            found: format!("{value:?}"),
                        });
                    }
                };
                let current = match fields.get(1) {
                    Some(Value::Int32(current)) => *current,
                    value => {
                        return Err(VmError::TypeMismatch {
                            expected: "ArrayIterator index field".to_string(),
                            found: format!("{value:?}"),
                        });
                    }
                };
                (array_ref, current)
            }
            _ => return Ok(None),
        };

        match method_name {
            "iter" => {
                let HeapObject::Struct { fields, .. } = thread.heap.get_object_mut(iterator_ref)?
                else {
                    unreachable!("iterator object was validated as a struct")
                };
                fields[1] = Value::Int32(0);
                Ok(Some(Value::Object(iterator_ref)))
            }
            "next" => {
                let value = match thread.heap.get_object(array_ref)? {
                    HeapObject::Array { elements, .. } => elements.get(current as usize).cloned(),
                    value => {
                        return Err(VmError::TypeMismatch {
                            expected: "ArrayIterator backing array".to_string(),
                            found: format!("{value:?}"),
                        });
                    }
                };
                if value.is_some() {
                    let HeapObject::Struct { fields, .. } =
                        thread.heap.get_object_mut(iterator_ref)?
                    else {
                        unreachable!("iterator object was validated as a struct")
                    };
                    fields[1] = Value::Int32(current + 1);
                }
                Ok(Some(value.unwrap_or(Value::Null)))
            }
            _ => Ok(None),
        }
    }
}
