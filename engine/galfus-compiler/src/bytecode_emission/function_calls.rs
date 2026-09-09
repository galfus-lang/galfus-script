use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::Reg;
use galfus_core::FunctionId;
use galfus_ir::mir::{self, LocalId, Operand};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub(super) fn emit_call(
        &mut self,
        func: &FunctionId,
        args: &[Operand],
        destination: LocalId,
        is_external: bool,
    ) -> bool {
        let builtin_name = self
            .ctx
            .function_names
            .get(func)
            .map(|name| name.to_string());
        let is_async_target = self
            .ctx
            .function_is_async
            .get(func)
            .copied()
            .unwrap_or(false);
        if builtin_name.is_some() || is_async_target {
            let name = builtin_name.as_deref().unwrap_or_default();
            let native_math_name = name
                .rsplit("::")
                .next()
                .filter(|name| name.starts_with("__internal_math_"));
            if let Some(native_math_name) = native_math_name {
                let start_reg = self.call_arguments_start(args);
                let operation = match native_math_name {
                    "__internal_math_is_nan" => 0,
                    "__internal_math_is_finite" => 1,
                    "__internal_math_is_infinite" => 2,
                    "__internal_math_sqrt" => 3,
                    "__internal_math_hypot" => 4,
                    "__internal_math_sin" => 5,
                    "__internal_math_cos" => 6,
                    "__internal_math_tan" => 7,
                    "__internal_math_log" => 8,
                    "__internal_math_log2" => 9,
                    "__internal_math_log10" => 10,
                    _ => unreachable!("recognized math intrinsic"),
                };
                self.instructions.push(Instruction::CallInternalMath {
                    dest: Reg(destination.raw() as u16),
                    operation,
                    args_start: start_reg,
                    arg_count: args.len() as u8,
                });
                self.free_call_arguments(args);
                return true;
            }

            let native_thread_name = name.rsplit("::").next().filter(|name| {
                matches!(
                    *name,
                    "__internal_thread_get"
                        | "__internal_thread_is_running"
                        | "__internal_thread_is_exited"
                        | "__internal_thread_exit_reason"
                        | "__internal_thread_send"
                        | "__internal_thread_has_messages"
                        | "__internal_thread_get_message"
                        | "__internal_thread_try_receive"
                )
            });
            if let Some(operation) = native_thread_name {
                let start_reg = self.call_arguments_start(args);
                let return_type = self.call_return_type(destination);
                let arg_types = args
                    .iter()
                    .map(|argument| {
                        crate::bytecode_emission::types::lower_type(
                            self.ctx,
                            self.get_operand_type(argument),
                        )
                    })
                    .collect();
                self.instructions.push(Instruction::CallInternalThread {
                    dest: Reg(destination.raw() as u16),
                    operation: operation.into(),
                    args_start: start_reg,
                    arg_count: args.len() as u8,
                    arg_types,
                    return_type: crate::bytecode_emission::types::lower_type(self.ctx, return_type),
                });
                self.direct_await_candidates
                    .insert(Reg(destination.raw() as u16), operation.into());
                self.free_call_arguments(args);
                return true;
            }

            let native_async_name = name
                .rsplit("::")
                .next()
                .filter(|name| is_external || name.starts_with("__internal_"));
            if native_async_name.is_some() || is_async_target {
                let start_reg = self.call_arguments_start(args);
                let return_type = self.call_return_type(destination);
                let arg_types = args
                    .iter()
                    .map(|argument| self.call_argument_type(argument))
                    .collect();
                let return_type = self
                    .ctx
                    .async_return_type_overrides
                    .get(func)
                    .copied()
                    .unwrap_or_else(|| {
                        crate::bytecode_emission::types::lower_type(self.ctx, return_type)
                    });
                self.instructions.push(Instruction::CreateFuture {
                    dest: Reg(destination.raw() as u16),
                    func: self.function_index(func),
                    args_start: start_reg,
                    arg_count: args.len() as u8,
                    arg_types,
                    return_type,
                });
                if !name.starts_with("__") {
                    self.direct_galfus_await_candidates
                        .insert(Reg(destination.raw() as u16));
                }
                if let Some(name) = native_async_name.filter(|name| name.starts_with("__internal_"))
                {
                    self.direct_await_candidates
                        .insert(Reg(destination.raw() as u16), name.into());
                }
                self.free_call_arguments(args);
                return true;
            }
        }

        if !is_external
            && args.len() == 1
            && let Operand::Local(argument) = &args[0]
        {
            self.instructions.push(Instruction::Call {
                dest: Reg(destination.raw() as u16),
                func: self.function_index(func),
                args_start: Reg(argument.raw() as u16),
                arg_count: 1,
            });
            return true;
        }

        let start_reg = self.regular_call_arguments_start(args);
        self.instructions.push(Instruction::Call {
            dest: Reg(destination.raw() as u16),
            func: self.function_index(func),
            args_start: start_reg,
            arg_count: args.len() as u8,
        });
        self.free_call_arguments(args);
        false
    }

    fn call_arguments_start(&mut self, args: &[Operand]) -> Reg {
        if args.is_empty() {
            return Reg(0);
        }
        let first = self.alloc_temp();
        let mut registers = vec![first];
        for _ in 1..args.len() {
            registers.push(self.alloc_temp());
        }
        for (index, argument) in args.iter().enumerate() {
            self.load_operand_to(argument, registers[index]);
        }
        first
    }

    fn free_call_arguments(&mut self, args: &[Operand]) {
        if !args.is_empty() {
            self.free_temps(args.len() as u16);
        }
    }

    fn regular_call_arguments_start(&mut self, args: &[Operand]) -> Reg {
        if args.is_empty() {
            self.alloc_temp()
        } else {
            self.call_arguments_start(args)
        }
    }

    fn call_return_type(&self, destination: LocalId) -> galfus_core::TypeId {
        let return_type = self
            .func
            .locals
            .iter()
            .find(|local| local.id == destination)
            .expect("native call destination must be a local")
            .ty;
        match self.ctx.type_result.layer().table().kind(return_type) {
            Some(galfus_frontend::TypeKind::GenericInstance { arguments, .. }) => {
                arguments.first().copied().unwrap_or(return_type)
            }
            _ => return_type,
        }
    }

    fn call_argument_type(&mut self, argument: &Operand) -> galfus_bytecode::instruction::TypeIdx {
        let is_string_const = match argument {
            mir::Operand::ConstRef(index) => matches!(
                self.ctx.mir_constants.get(*index),
                Some(mir::Constant::String(_))
            ),
            mir::Operand::Constant(mir::Constant::String(_)) => true,
            _ => false,
        };
        if !is_string_const {
            return crate::bytecode_emission::types::lower_type(
                self.ctx,
                self.get_operand_type(argument),
            );
        }
        let u8_ty = self
            .ctx
            .type_result
            .layer()
            .table()
            .primitive(galfus_frontend::PrimitiveType::Uint8);
        let u8_idx = crate::bytecode_emission::types::lower_type(self.ctx, u8_ty);
        let type_idx = galfus_bytecode::instruction::TypeIdx(self.ctx.types.len() as u16);
        self.ctx
            .types
            .push(galfus_bytecode::BytecodeType::Array(u8_idx));
        type_idx
    }

    fn function_index(&self, func: &FunctionId) -> galfus_bytecode::instruction::FuncIdx {
        *self.ctx.function_map.get(func).unwrap_or_else(|| {
            panic!(
                "missing lowered function mapping for {:?} while emitting {} ({:?})",
                func, self.func.name, self.func.id
            )
        })
    }
}
