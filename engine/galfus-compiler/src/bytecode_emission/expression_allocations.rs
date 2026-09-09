use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::{FieldIdx, Reg};
use galfus_core::{FunctionId, TypeId};
use galfus_frontend::TypeKind;
use galfus_ir::mir::{ArrayLiteralElement, Constant as MirConstant, Operand};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub(super) fn emit_new_struct(&mut self, dest: Reg, struct_type: TypeId, fields: &[Operand]) {
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, struct_type);
        self.instructions
            .push(Instruction::AllocLocal { dest, type_idx });

        let _struct_symbol = self.struct_symbol_for_type(struct_type);

        for (i, val_operand) in fields.iter().enumerate() {
            let val_reg = self.operand_reg(val_operand);
            self.instructions.push(Instruction::StoreField {
                obj: dest,
                field: FieldIdx(i as u16),
                val: val_reg,
            });
            self.free_temp_if_operand(val_operand);
        }
    }

    pub(super) fn emit_new_array(&mut self, dest: Reg, element_type: TypeId, elements: &[Operand]) {
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, element_type);
        let size_const = crate::bytecode_emission::constants::get_or_create_constant(
            self.ctx,
            &MirConstant::Int32(elements.len() as i32),
        );
        let size_reg = self.alloc_temp();
        self.instructions.push(Instruction::LoadConst {
            dest: size_reg,
            const_idx: size_const,
        });

        self.instructions.push(Instruction::NewArray {
            dest,
            type_idx,
            len_reg: size_reg,
        });
        self.free_temps(1);

        for (i, elem_operand) in elements.iter().enumerate() {
            let idx_const = crate::bytecode_emission::constants::get_or_create_constant(
                self.ctx,
                &MirConstant::Int32(i as i32),
            );
            let idx_reg = self.alloc_temp();
            self.instructions.push(Instruction::LoadConst {
                dest: idx_reg,
                const_idx: idx_const,
            });

            let val_reg = self.operand_reg(elem_operand);
            self.instructions.push(Instruction::StoreIndex {
                arr: dest,
                idx: idx_reg,
                val: val_reg,
            });
            self.free_temp_if_operand(elem_operand);
            self.free_temps(1);
        }
    }

    pub(super) fn emit_new_array_dynamic(
        &mut self,
        dest: Reg,
        array_type: TypeId,
        elements: &[ArrayLiteralElement],
    ) {
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, array_type);
        let total_len_reg = self.alloc_temp();
        let const_zero = crate::bytecode_emission::constants::get_or_create_constant(
            self.ctx,
            &MirConstant::Int32(0),
        );
        self.instructions.push(Instruction::LoadConst {
            dest: total_len_reg,
            const_idx: const_zero,
        });

        let const_one = crate::bytecode_emission::constants::get_or_create_constant(
            self.ctx,
            &MirConstant::Int32(1),
        );

        for element in elements {
            match element {
                ArrayLiteralElement::Single(_) => {
                    let one_reg = self.alloc_temp();
                    self.instructions.push(Instruction::LoadConst {
                        dest: one_reg,
                        const_idx: const_one,
                    });
                    self.instructions.push(Instruction::Add {
                        dest: total_len_reg,
                        lhs: total_len_reg,
                        rhs: one_reg,
                    });
                    self.free_temps(1);
                }
                ArrayLiteralElement::Spread(operand) => {
                    let operand_reg = self.operand_reg(operand);
                    let len_reg = self.alloc_temp();
                    self.instructions.push(Instruction::Len {
                        dest: len_reg,
                        src: operand_reg,
                    });
                    self.instructions.push(Instruction::Add {
                        dest: total_len_reg,
                        lhs: total_len_reg,
                        rhs: len_reg,
                    });
                    self.free_temps(1);
                    self.free_temp_if_operand(operand);
                }
            }
        }

        self.instructions.push(Instruction::NewArray {
            dest,
            type_idx,
            len_reg: total_len_reg,
        });

        let offset_reg = self.alloc_temp();
        self.instructions.push(Instruction::LoadConst {
            dest: offset_reg,
            const_idx: const_zero,
        });

        for element in elements {
            match element {
                ArrayLiteralElement::Single(operand) => {
                    let val_reg = self.operand_reg(operand);
                    self.instructions.push(Instruction::StoreIndex {
                        arr: dest,
                        idx: offset_reg,
                        val: val_reg,
                    });
                    self.free_temp_if_operand(operand);

                    let one_reg = self.alloc_temp();
                    self.instructions.push(Instruction::LoadConst {
                        dest: one_reg,
                        const_idx: const_one,
                    });
                    self.instructions.push(Instruction::Add {
                        dest: offset_reg,
                        lhs: offset_reg,
                        rhs: one_reg,
                    });
                    self.free_temps(1);
                }
                ArrayLiteralElement::Spread(operand) => {
                    let src_reg = self.operand_reg(operand);
                    self.instructions.push(Instruction::CopyArray {
                        dest,
                        dest_start: offset_reg,
                        src: src_reg,
                    });

                    let len_reg = self.alloc_temp();
                    self.instructions.push(Instruction::Len {
                        dest: len_reg,
                        src: src_reg,
                    });
                    self.instructions.push(Instruction::Add {
                        dest: offset_reg,
                        lhs: offset_reg,
                        rhs: len_reg,
                    });
                    self.free_temps(1);
                    self.free_temp_if_operand(operand);
                }
            }
        }

        self.free_temps(2);
    }

    pub(super) fn emit_new_tuple(&mut self, dest: Reg, tuple_type: TypeId, elements: &[Operand]) {
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, tuple_type);
        let start_reg = self.alloc_temp();
        let mut temp_regs = vec![start_reg];
        for _ in 1..elements.len() {
            temp_regs.push(self.alloc_temp());
        }

        for (i, elem_operand) in elements.iter().enumerate() {
            self.load_operand_to(elem_operand, temp_regs[i]);
        }

        self.instructions.push(Instruction::NewTuple {
            dest,
            type_idx,
            start: start_reg,
            count: elements.len() as u8,
        });
        self.free_temps(elements.len() as u16);
    }

    pub(super) fn emit_array_index(
        &mut self,
        dest: Reg,
        arr_operand: &Operand,
        idx_operand: &Operand,
    ) {
        let arr = self.operand_reg(arr_operand);
        let idx = self.operand_reg(idx_operand);
        self.instructions
            .push(Instruction::LoadIndex { dest, arr, idx });
        self.free_temp_if_operand(idx_operand);
        self.free_temp_if_operand(arr_operand);
    }

    pub(super) fn emit_member_access(
        &mut self,
        dest: Reg,
        obj_operand: &Operand,
        field_name: &str,
    ) {
        let obj = self.operand_reg(obj_operand);
        let field_idx = self.field_idx_for_member(obj_operand, field_name);
        self.instructions.push(Instruction::LoadField {
            dest,
            obj,
            field: field_idx,
        });
        self.free_temp_if_operand(obj_operand);
    }

    pub(super) fn emit_choice(
        &mut self,
        dest: Reg,
        choice_type: TypeId,
        variant_name: &str,
        payload_operand: Option<&Operand>,
    ) {
        let destination_type = self
            .func
            .locals
            .iter()
            .find(|local| local.id.raw() as u16 == dest.raw())
            .map(|local| local.ty)
            .unwrap_or(choice_type);
        let choice_type = if matches!(
            self.ctx.type_result.layer().table().kind(destination_type),
            Some(TypeKind::GenericInstance { .. })
        ) {
            destination_type
        } else if matches!(
            self.ctx.type_result.layer().table().kind(choice_type),
            Some(TypeKind::Named { .. })
        ) && matches!(
            self.ctx
                .type_result
                .layer()
                .table()
                .kind(self.func.return_type),
            Some(TypeKind::GenericInstance { .. })
        ) {
            self.func.return_type
        } else {
            destination_type
        };
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, choice_type);
        let variant_idx = if let Some(choice_symbol) = self.struct_symbol_for_type(choice_type) {
            let variants =
                crate::bytecode_emission::types::get_choice_variants(self.ctx, choice_symbol);
            variants
                .iter()
                .position(|(name, _)| name == variant_name)
                .unwrap_or(0)
        } else if let Some(choice) =
            crate::bytecode_emission::types::find_imported_choice_for_type(self.ctx, choice_type)
        {
            choice
                .variants
                .iter()
                .position(|variant| variant.name == variant_name)
                .unwrap_or(0)
        } else {
            0
        };

        let payload_reg = if let Some(operand) = payload_operand {
            self.operand_reg(operand)
        } else {
            let reg = self.alloc_temp();
            self.instructions.push(Instruction::LoadNull { dest: reg });
            reg
        };

        self.instructions.push(Instruction::NewChoice {
            dest,
            type_idx,
            variant_idx: variant_idx as u16,
            payload: payload_reg,
        });

        if let Some(operand) = payload_operand {
            self.free_temp_if_operand(operand);
        } else {
            self.free_temps(1);
        }
    }

    pub(super) fn emit_len(&mut self, dest: Reg, operand: &Operand) {
        let src = self.operand_reg(operand);
        self.instructions.push(Instruction::Len { dest, src });
        self.free_temp_if_operand(operand);
    }

    pub(super) fn emit_new_array_zeroed(&mut self, dest: Reg, array_type: TypeId, size: usize) {
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, array_type);
        let size_const = crate::bytecode_emission::constants::get_or_create_constant(
            self.ctx,
            &MirConstant::Int32(size as i32),
        );
        let len_reg = self.alloc_temp();
        self.instructions.push(Instruction::LoadConst {
            dest: len_reg,
            const_idx: size_const,
        });
        self.instructions.push(Instruction::NewArray {
            dest,
            type_idx,
            len_reg,
        });
        self.free_temps(1);
    }

    pub(super) fn emit_new_array_zeroed_dynamic(
        &mut self,
        dest: Reg,
        array_type: TypeId,
        length: &Operand,
    ) {
        let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, array_type);
        let len_reg = self.operand_reg(length);
        self.instructions.push(Instruction::NewArray {
            dest,
            type_idx,
            len_reg,
        });
        self.free_temp_if_operand(length);
    }

    pub(super) fn emit_create_future(&mut self, dest: Reg, func: FunctionId, args: &[Operand]) {
        let func_idx = *self.ctx.function_map.get(&func).unwrap_or_else(|| {
            panic!(
                "missing lowered function mapping for {:?} while emitting {} ({:?})",
                func, self.func.name, self.func.id
            )
        });
        let start_reg = self.load_future_arguments(args);
        let arg_types = args
            .iter()
            .map(|argument| {
                crate::bytecode_emission::types::lower_type(
                    self.ctx,
                    self.get_operand_type(argument),
                )
            })
            .collect();
        let future_ty = self
            .func
            .locals
            .iter()
            .find(|local| local.id.raw() as u16 == dest.raw())
            .map(|local| local.ty)
            .expect("CreateFuture destination must be a MIR local");
        let return_type = self.future_payload_type_index(future_ty);
        self.instructions.push(Instruction::CreateFuture {
            dest,
            func: func_idx,
            args_start: start_reg,
            arg_count: args.len() as u8,
            arg_types,
            return_type,
        });
        if self.ctx.function_names.get(&func).is_none_or(|name| {
            !name
                .rsplit("::")
                .next()
                .is_some_and(|name| name.starts_with("__"))
        }) {
            self.direct_galfus_await_candidates.insert(dest);
        }
        if !args.is_empty() {
            self.free_temps(args.len() as u16);
        }
    }

    pub(super) fn emit_create_indirect_future(
        &mut self,
        dest: Reg,
        func_op: &Operand,
        args: &[Operand],
    ) {
        let func_reg = self.operand_reg(func_op);
        let start_reg = self.load_future_arguments(args);
        let function_ty = crate::bytecode_emission::types::resolve_type_with_substitutions(
            self.ctx,
            self.get_operand_type(func_op),
        );
        let table = self.ctx.type_result.layer().table();
        let TypeKind::Function(function) = table
            .kind(function_ty)
            .unwrap_or_else(|| panic!("indirect future target must have a function type"))
        else {
            panic!("indirect future target must have a function type");
        };
        let arg_types = args
            .iter()
            .map(|argument| {
                crate::bytecode_emission::types::lower_type(
                    self.ctx,
                    self.get_operand_type(argument),
                )
            })
            .collect();
        let return_type = self.future_payload_type_index(function.return_type());

        self.instructions.push(Instruction::CreateIndirectFuture {
            dest,
            func_reg,
            args_start: start_reg,
            arg_count: args.len() as u8,
            arg_types,
            return_type,
        });
        self.free_temp_if_operand(func_op);
        if !args.is_empty() {
            self.free_temps(args.len() as u16);
        }
    }

    fn load_future_arguments(&mut self, args: &[Operand]) -> Reg {
        if args.is_empty() {
            return Reg(0);
        }

        let first = self.alloc_temp();
        let mut temp_regs = vec![first];
        for _ in 1..args.len() {
            temp_regs.push(self.alloc_temp());
        }
        for (i, arg_op) in args.iter().enumerate() {
            self.load_operand_to(arg_op, temp_regs[i]);
        }
        first
    }
}
