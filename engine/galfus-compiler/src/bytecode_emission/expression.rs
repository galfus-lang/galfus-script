use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::{FieldIdx, GlobalIdx, Reg};
use galfus_core::{SymbolId, TypeId};
use galfus_frontend::{PrimitiveType, SymbolKind, TypeKind};
use galfus_ir::mir::{Constant as MirConstant, MirBinaryOp, MirUnaryOp, Operand, RValue};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub fn emit_rvalue(&mut self, dest: Reg, rvalue: &RValue) {
        match rvalue {
            RValue::Use(operand) => {
                self.load_operand_to(operand, dest);

                if matches!(
                    operand,
                    Operand::Constant(
                        MirConstant::Int8(_)
                            | MirConstant::Int16(_)
                            | MirConstant::Int32(_)
                            | MirConstant::Int64(_)
                            | MirConstant::Uint8(_)
                            | MirConstant::Uint16(_)
                            | MirConstant::Uint32(_)
                            | MirConstant::Uint64(_)
                            | MirConstant::Float32(_)
                            | MirConstant::Float64(_)
                    )
                ) && let Some(local) = self
                    .func
                    .locals
                    .iter()
                    .find(|local| local.id.raw() as u16 == dest.raw())
                {
                    let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, local.ty);
                    self.instructions.push(Instruction::Cast {
                        dest,
                        src: dest,
                        type_idx,
                    });
                }
            }
            RValue::UnaryOp(op, operand) => {
                let src = self.operand_reg(operand);
                let instr = match op {
                    MirUnaryOp::Negate => Instruction::Neg { dest, src },
                    MirUnaryOp::Not => Instruction::Not { dest, src },
                    MirUnaryOp::BitwiseNot => Instruction::BitNot { dest, src },
                };
                self.instructions.push(instr);
                self.free_temp_if_operand(operand);
            }
            RValue::BinaryOp(op, lhs, rhs) => {
                let mut lhs_reg = self.operand_reg(lhs);
                let mut rhs_reg = self.operand_reg(rhs);
                let lhs_ty = self.get_operand_type(lhs);
                let rhs_ty = self.get_operand_type(rhs);

                let layer = self.ctx.type_result.layer();
                let table = layer.table();

                let is_numeric = |ty: TypeId| {
                    matches!(
                        table.kind(ty),
                        Some(TypeKind::Primitive(
                            PrimitiveType::Int8
                                | PrimitiveType::Int16
                                | PrimitiveType::Int32
                                | PrimitiveType::Int64
                                | PrimitiveType::Uint8
                                | PrimitiveType::Uint16
                                | PrimitiveType::Uint32
                                | PrimitiveType::Uint64
                                | PrimitiveType::Float16
                                | PrimitiveType::Float32
                                | PrimitiveType::Float64
                        ))
                    )
                };

                let mut cast_temp_count = 0;

                if lhs_ty != rhs_ty && is_numeric(lhs_ty) && is_numeric(rhs_ty) {
                    if matches!(lhs, Operand::Local(_)) && matches!(rhs, Operand::Constant(_)) {
                        let temp = self.alloc_temp();
                        cast_temp_count += 1;
                        let type_idx =
                            crate::bytecode_emission::types::lower_type(self.ctx, lhs_ty);
                        self.instructions.push(Instruction::Cast {
                            dest: temp,
                            src: rhs_reg,
                            type_idx,
                        });
                        rhs_reg = temp;
                    } else if matches!(rhs, Operand::Local(_))
                        && matches!(lhs, Operand::Constant(_))
                    {
                        let temp = self.alloc_temp();
                        cast_temp_count += 1;
                        let type_idx =
                            crate::bytecode_emission::types::lower_type(self.ctx, rhs_ty);
                        self.instructions.push(Instruction::Cast {
                            dest: temp,
                            src: lhs_reg,
                            type_idx,
                        });
                        lhs_reg = temp;
                    } else {
                        let temp = self.alloc_temp();
                        cast_temp_count += 1;
                        let type_idx =
                            crate::bytecode_emission::types::lower_type(self.ctx, lhs_ty);
                        self.instructions.push(Instruction::Cast {
                            dest: temp,
                            src: rhs_reg,
                            type_idx,
                        });
                        rhs_reg = temp;
                    }
                }

                let instr = match op {
                    MirBinaryOp::Add => Instruction::Add {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Subtract => Instruction::Sub {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Multiply => Instruction::Mul {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Divide => Instruction::Div {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Remainder => Instruction::Rem {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Power => Instruction::Pow {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::ShiftLeft => Instruction::Shl {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::ShiftRight => Instruction::Shr {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::BitwiseAnd => Instruction::And {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::BitwiseOr => Instruction::Or {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::BitwiseXor => Instruction::Xor {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Equal => Instruction::Eq {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::NotEqual => Instruction::Ne {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Less => Instruction::Lt {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::LessEqual => Instruction::Le {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::Greater => Instruction::Gt {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::GreaterEqual => Instruction::Ge {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::LogicalAnd => Instruction::And {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::LogicalOr => Instruction::Or {
                        dest,
                        lhs: lhs_reg,
                        rhs: rhs_reg,
                    },
                    MirBinaryOp::NullFallback => Instruction::Fallback {
                        dest,
                        src: lhs_reg,
                        fallback: rhs_reg,
                    },
                };
                self.instructions.push(instr);
                self.free_temps(cast_temp_count);
                self.free_temp_if_operand(rhs);
                self.free_temp_if_operand(lhs);
            }
            RValue::Cast(operand, ty) => {
                let src = self.operand_reg(operand);
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *ty);
                self.instructions.push(Instruction::Cast {
                    dest,
                    src,
                    type_idx,
                });
                self.free_temp_if_operand(operand);
            }
            RValue::Copy(operand) => {
                let src = self.operand_reg(operand);
                self.instructions.push(Instruction::Copy { dest, src });
                self.free_temp_if_operand(operand);
            }
            RValue::Instanceof(operand, ty) => {
                let src = self.operand_reg(operand);
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *ty);
                self.instructions.push(Instruction::Instanceof {
                    dest,
                    src,
                    type_idx,
                });
                self.free_temp_if_operand(operand);
            }
            RValue::ChoiceVariantIs(operand, variant) => {
                let src = self.operand_reg(operand);
                let type_idx =
                    crate::bytecode_emission::types::lower_choice_variant_type(self.ctx, *variant);
                self.instructions.push(Instruction::Instanceof {
                    dest,
                    src,
                    type_idx,
                });
                self.free_temp_if_operand(operand);
            }
            RValue::LoadGlobal(name) => {
                // Find global index. For simplicity, we can just use 0 if not found,
                // or we can map it to global index.
                let global_idx = self
                    .ctx
                    .graph
                    .resolution()
                    .and_then(|res| {
                        let name_id = self.ctx.string_table.get(name);
                        res.symbols()
                            .iter()
                            .position(|s| name_id.is_some() && s.name() == name_id.unwrap())
                            .map(|idx| idx as u16)
                    })
                    .unwrap_or(0);
                self.instructions.push(Instruction::LoadGlobal {
                    dest,
                    module_id: galfus_core::ModuleId::new(0),
                    global_idx: GlobalIdx(global_idx),
                });
            }
            RValue::NewStruct {
                struct_type,
                fields,
            } => {
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *struct_type);
                self.instructions
                    .push(Instruction::AllocLocal { dest, type_idx });

                // Find fields list to map field names to indices
                let _struct_symbol = self.struct_symbol_for_type(*struct_type);

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
            RValue::NewArray(element_type, elements) => {
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *element_type);
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
            RValue::NewArrayDynamic(array_type, elements) => {
                use galfus_ir::mir::ArrayLiteralElement;

                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *array_type);

                // 1. Calculate total length at runtime
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
                        ArrayLiteralElement::Single(_operand) => {
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

                // 2. Allocate the new array
                self.instructions.push(Instruction::NewArray {
                    dest,
                    type_idx,
                    len_reg: total_len_reg,
                });

                // 3. Copy elements/sub-arrays
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

                // Free total_len_reg and offset_reg
                self.free_temps(2);
            }
            RValue::NewTuple(tuple_type, elements) => {
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *tuple_type);
                let start_reg = self.alloc_temp();

                // Allocate remaining contiguous temps
                let mut temp_regs = vec![start_reg];
                for _ in 1..elements.len() {
                    temp_regs.push(self.alloc_temp());
                }

                // Load each operand into contiguous registers
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
            RValue::ArrayIndex(arr_operand, idx_operand) => {
                let arr = self.operand_reg(arr_operand);
                let idx = self.operand_reg(idx_operand);
                self.instructions
                    .push(Instruction::LoadIndex { dest, arr, idx });
                self.free_temp_if_operand(idx_operand);
                self.free_temp_if_operand(arr_operand);
            }
            RValue::MemberAccess(obj_operand, field_name) => {
                let obj = self.operand_reg(obj_operand);
                let field_idx = self.field_idx_for_member(obj_operand, field_name);

                self.instructions.push(Instruction::LoadField {
                    dest,
                    obj,
                    field: field_idx,
                });
                self.free_temp_if_operand(obj_operand);
            }
            RValue::Choice(choice_type, variant_name, payload_operand) => {
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *choice_type);
                let variant_idx =
                    if let Some(choice_symbol) = self.struct_symbol_for_type(*choice_type) {
                        let variants = crate::bytecode_emission::types::get_choice_variants(
                            self.ctx,
                            choice_symbol,
                        );
                        variants
                            .iter()
                            .position(|(name, _)| name == variant_name)
                            .unwrap_or(0)
                    } else if let Some(choice) =
                        crate::bytecode_emission::types::find_imported_choice_for_type(
                            self.ctx,
                            *choice_type,
                        )
                    {
                        choice
                            .variants
                            .iter()
                            .position(|v| v.name == *variant_name)
                            .unwrap_or(0)
                    } else {
                        0
                    };

                let payload_reg = if let Some(op) = payload_operand {
                    self.operand_reg(op)
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

                if payload_operand.is_some() {
                    self.free_temp_if_operand(payload_operand.as_ref().unwrap());
                } else {
                    self.free_temps(1);
                }
            }
            RValue::Len(operand) => {
                let src = self.operand_reg(operand);
                self.instructions.push(Instruction::Len { dest, src });
                self.free_temp_if_operand(operand);
            }
            RValue::NewArrayZeroed {
                array_type, size, ..
            } => {
                // Emit:
                //   len_reg = const(size)
                //   dest = NewArray(array_type, len_reg)
                //
                // `array_type` must lower to BytecodeType::Array.
                // The VM extracts the element type from that bytecode type and then
                // zero-initialises the backing buffer.
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *array_type);

                let size_const = crate::bytecode_emission::constants::get_or_create_constant(
                    self.ctx,
                    &MirConstant::Int32(*size as i32),
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
            RValue::NewArrayZeroedDynamic {
                array_type, length, ..
            } => {
                let type_idx = crate::bytecode_emission::types::lower_type(self.ctx, *array_type);
                let len_reg = self.operand_reg(length);

                self.instructions.push(Instruction::NewArray {
                    dest,
                    type_idx,
                    len_reg,
                });

                self.free_temp_if_operand(length);
            }
            RValue::CreateFuture {
                func,
                args,
                is_external: _,
            } => {
                let func_idx = *self.ctx.function_map.get(func).unwrap_or_else(|| {
                    panic!(
                        "missing lowered function mapping for {:?} while emitting {} ({:?})",
                        func, self.func.name, self.func.id
                    )
                });
                let start_reg = if args.is_empty() {
                    Reg(0)
                } else {
                    let first = self.alloc_temp();
                    let mut temp_regs = vec![first];
                    for _ in 1..args.len() {
                        temp_regs.push(self.alloc_temp());
                    }
                    for (i, arg_op) in args.iter().enumerate() {
                        self.load_operand_to(arg_op, temp_regs[i]);
                    }
                    first
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
                if !args.is_empty() {
                    self.free_temps(args.len() as u16);
                }
            }
            RValue::CreateIndirectFuture {
                func: func_op,
                args,
            } => {
                let func_reg = self.operand_reg(func_op);
                let start_reg = if args.is_empty() {
                    Reg(0)
                } else {
                    let first = self.alloc_temp();
                    let mut temp_regs = vec![first];
                    for _ in 1..args.len() {
                        temp_regs.push(self.alloc_temp());
                    }
                    for (i, arg_op) in args.iter().enumerate() {
                        self.load_operand_to(arg_op, temp_regs[i]);
                    }
                    first
                };

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
        }
    }

    pub fn field_idx_for_member(&self, obj_operand: &Operand, field_name: &str) -> FieldIdx {
        let obj_type = self.get_operand_type(obj_operand);
        let table = self.ctx.type_result.layer().table();
        let resolved_type = crate::bytecode_emission::types::resolve_alias_type(self.ctx, obj_type);

        let field_idx = if matches!(table.kind(resolved_type), Some(TypeKind::Tuple { .. })) {
            field_name.parse::<u16>().unwrap_or(0)
        } else if let Some(symbol) = self.struct_symbol_for_type(obj_type) {
            let struct_fields =
                crate::bytecode_emission::types::get_struct_fields(self.ctx, symbol);
            struct_fields
                .iter()
                .position(|(name, _)| name == field_name)
                .unwrap_or(0) as u16
        } else {
            0
        };

        FieldIdx(field_idx)
    }

    pub fn operand_reg(&mut self, operand: &Operand) -> Reg {
        match operand {
            Operand::Local(local_id) => Reg(local_id.raw() as u16),
            Operand::ConstRef(idx) => {
                let constant = &self.ctx.mir_constants[*idx];
                let temp = self.alloc_temp();
                match constant {
                    MirConstant::Null => {
                        self.instructions.push(Instruction::LoadNull { dest: temp })
                    }
                    _ => {
                        let const_idx = crate::bytecode_emission::constants::get_or_create_constant(
                            self.ctx, constant,
                        );
                        self.instructions.push(Instruction::LoadConst {
                            dest: temp,
                            const_idx,
                        });
                    }
                }
                temp
            }
            Operand::Constant(constant) => {
                let temp = self.alloc_temp();
                match constant {
                    MirConstant::Null => {
                        self.instructions.push(Instruction::LoadNull { dest: temp })
                    }
                    _ => {
                        let const_idx = crate::bytecode_emission::constants::get_or_create_constant(
                            self.ctx, constant,
                        );
                        self.instructions.push(Instruction::LoadConst {
                            dest: temp,
                            const_idx,
                        });
                    }
                }
                temp
            }
        }
    }

    pub fn load_operand_to(&mut self, operand: &Operand, dest: Reg) {
        match operand {
            Operand::Local(local_id) => {
                let src = Reg(local_id.raw() as u16);
                if src != dest {
                    self.instructions.push(Instruction::Move { dest, src });
                }
            }
            Operand::ConstRef(idx) => {
                let constant = &self.ctx.mir_constants[*idx];
                match constant {
                    MirConstant::Null => self.instructions.push(Instruction::LoadNull { dest }),
                    _ => {
                        let const_idx = crate::bytecode_emission::constants::get_or_create_constant(
                            self.ctx, constant,
                        );
                        self.instructions
                            .push(Instruction::LoadConst { dest, const_idx });
                    }
                }
            }
            Operand::Constant(constant) => match constant {
                MirConstant::Null => self.instructions.push(Instruction::LoadNull { dest }),
                _ => {
                    let const_idx = crate::bytecode_emission::constants::get_or_create_constant(
                        self.ctx, constant,
                    );
                    self.instructions
                        .push(Instruction::LoadConst { dest, const_idx });
                }
            },
        }
    }

    pub fn free_temp_if_operand(&mut self, operand: &Operand) {
        if matches!(operand, Operand::Constant(_)) {
            self.free_temps(1);
        }
    }

    fn future_payload_type_index(
        &mut self,
        future_ty: TypeId,
    ) -> galfus_bytecode::instruction::TypeIdx {
        let future_ty =
            crate::bytecode_emission::types::resolve_type_with_substitutions(self.ctx, future_ty);
        let table = self.ctx.type_result.layer().table();
        let TypeKind::GenericInstance { arguments, .. } = table
            .kind(future_ty)
            .unwrap_or_else(|| panic!("async function must return Future<T>"))
        else {
            panic!("async function must return Future<T>");
        };
        let payload_ty = *arguments
            .first()
            .expect("Future<T> must carry a payload type");
        crate::bytecode_emission::types::lower_type(self.ctx, payload_ty)
    }

    pub(crate) fn get_operand_type(&self, operand: &Operand) -> TypeId {
        match operand {
            Operand::Local(local_id) => {
                let local_decl = self.func.locals.iter().find(|l| l.id == *local_id).unwrap();
                local_decl.ty
            }
            Operand::ConstRef(idx) => {
                let constant = &self.ctx.mir_constants[*idx];
                let layer = self.ctx.type_result.layer();
                let table = layer.table();
                if matches!(constant, MirConstant::String(_)) {
                    let u8_ty = table.primitive(PrimitiveType::Uint8);
                    for i in 0..table.len() {
                        let ty_id = TypeId::new(i as u32);
                        if let Some(TypeKind::Array { element }) = table.kind(ty_id) {
                            if *element == u8_ty {
                                return ty_id;
                            }
                        }
                    }
                }
                let prim = match constant {
                    MirConstant::Null => PrimitiveType::Null,
                    MirConstant::Bool(_) => PrimitiveType::Bool,
                    MirConstant::Int8(_)
                    | MirConstant::Int16(_)
                    | MirConstant::Int32(_)
                    | MirConstant::Int64(_)
                    | MirConstant::Uint8(_)
                    | MirConstant::Uint16(_)
                    | MirConstant::Uint32(_)
                    | MirConstant::Uint64(_) => PrimitiveType::Int32,
                    MirConstant::Float32(_) | MirConstant::Float64(_) => PrimitiveType::Float32,
                    MirConstant::Function(_) => PrimitiveType::Null,
                    MirConstant::String(_) => unreachable!(),
                };
                table.primitive(prim)
            }
            Operand::Constant(constant) => {
                let layer = self.ctx.type_result.layer();
                let table = layer.table();
                if matches!(constant, MirConstant::String(_)) {
                    let u8_ty = table.primitive(PrimitiveType::Uint8);
                    for i in 0..table.len() {
                        let ty_id = TypeId::new(i as u32);
                        if let Some(TypeKind::Array { element }) = table.kind(ty_id) {
                            if *element == u8_ty {
                                return ty_id;
                            }
                        }
                    }
                }
                let prim = match constant {
                    MirConstant::Null => PrimitiveType::Null,
                    MirConstant::Bool(_) => PrimitiveType::Bool,
                    MirConstant::Int8(_)
                    | MirConstant::Int16(_)
                    | MirConstant::Int32(_)
                    | MirConstant::Int64(_)
                    | MirConstant::Uint8(_)
                    | MirConstant::Uint16(_)
                    | MirConstant::Uint32(_)
                    | MirConstant::Uint64(_) => PrimitiveType::Int32,
                    MirConstant::Float32(_) | MirConstant::Float64(_) => PrimitiveType::Float64,
                    MirConstant::Function(_) => PrimitiveType::Null,
                    MirConstant::String(_) => unreachable!(),
                };
                for i in 0..table.len() {
                    let ty_id = TypeId::new(i as u32);
                    if matches!(table.kind(ty_id), Some(TypeKind::Primitive(p)) if p == &prim) {
                        return ty_id;
                    }
                }
                TypeId::new(0)
            }
        }
    }

    fn struct_symbol_for_type(&self, ty: TypeId) -> Option<SymbolId> {
        let ty = crate::bytecode_emission::types::resolve_alias_type(self.ctx, ty);
        let layer = self.ctx.type_result.layer();
        let table = layer.table();
        let mut current = ty;
        loop {
            match table.kind(current) {
                Some(TypeKind::Named { symbol }) => {
                    let resolution = self.ctx.graph.resolution()?;
                    let is_struct_or_choice = resolution.symbol(*symbol).is_some_and(|sd| {
                        sd.kind() == SymbolKind::Struct || sd.kind() == SymbolKind::Choice
                    });
                    if is_struct_or_choice {
                        return Some(*symbol);
                    }
                    break;
                }
                Some(TypeKind::GenericInstance { base, .. }) => {
                    current = *base;
                }
                _ => break,
            }
        }
        None
    }
}
