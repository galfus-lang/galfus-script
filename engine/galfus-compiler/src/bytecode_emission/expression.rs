use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::Reg;
use galfus_core::TypeId;
use galfus_frontend::{PrimitiveType, TypeKind};
use galfus_ir::mir::{MirBinaryOp, Operand, RValue};

impl<'a, 'b> FnEmitter<'a, 'b> {
    pub fn emit_rvalue(&mut self, dest: Reg, rvalue: &RValue) {
        self.known_immediates.remove(&dest);
        match rvalue {
            RValue::Use(operand) => self.emit_use(dest, operand),
            RValue::UnaryOp(op, operand) => self.emit_unary_operation(dest, op, operand),
            RValue::BinaryOp(op, lhs, rhs) => {
                let lhs_ty = self.get_operand_type(lhs);
                let rhs_ty = self.get_operand_type(rhs);
                let immediate = (lhs_ty == rhs_ty)
                    .then(|| self.immediate_value(rhs))
                    .flatten()
                    .zip(self.immediate_binary_operation(op));
                let mut lhs_reg = self.operand_reg(lhs);
                let mut rhs_reg = if immediate.is_none() {
                    self.operand_reg(rhs)
                } else {
                    Reg(0)
                };

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

                let table = self.ctx.type_result.layer().table();
                let lhs_ty = crate::bytecode_emission::types::resolve_type_with_substitutions(
                    self.ctx, lhs_ty,
                );
                let rhs_ty = crate::bytecode_emission::types::resolve_type_with_substitutions(
                    self.ctx, rhs_ty,
                );
                let lhs_kind = table.kind(lhs_ty);
                let rhs_kind = table.kind(rhs_ty);

                let is_i32 = matches!(
                    (lhs_kind, rhs_kind),
                    (
                        Some(TypeKind::Primitive(PrimitiveType::Int32)),
                        Some(TypeKind::Primitive(PrimitiveType::Int32))
                    )
                );
                let is_i64 = matches!(
                    (lhs_kind, rhs_kind),
                    (
                        Some(TypeKind::Primitive(PrimitiveType::Int64)),
                        Some(TypeKind::Primitive(PrimitiveType::Int64))
                    )
                );
                let is_f32 = matches!(
                    (lhs_kind, rhs_kind),
                    (
                        Some(TypeKind::Primitive(PrimitiveType::Float32)),
                        Some(TypeKind::Primitive(PrimitiveType::Float32))
                    )
                );
                let is_f64 = matches!(
                    (lhs_kind, rhs_kind),
                    (
                        Some(TypeKind::Primitive(PrimitiveType::Float64)),
                        Some(TypeKind::Primitive(PrimitiveType::Float64))
                    )
                );

                let instr = if let Some((rhs, operation)) = immediate {
                    Instruction::BinaryImmediate {
                        dest,
                        lhs: lhs_reg,
                        operation,
                        rhs,
                    }
                } else if is_i32 {
                    match op {
                        MirBinaryOp::Add => Instruction::AddI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Subtract => Instruction::SubI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Multiply => Instruction::MulI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Divide => Instruction::DivI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Remainder => Instruction::RemI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Equal => Instruction::EqI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::NotEqual => Instruction::NeI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Less => Instruction::LtI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::LessEqual => Instruction::LeI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Greater => Instruction::GtI32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::GreaterEqual => Instruction::GeI32 {
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
                    }
                } else if is_i64 {
                    match op {
                        MirBinaryOp::Add => Instruction::AddI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Subtract => Instruction::SubI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Multiply => Instruction::MulI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Divide => Instruction::DivI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Remainder => Instruction::RemI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Equal => Instruction::EqI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::NotEqual => Instruction::NeI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Less => Instruction::LtI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::LessEqual => Instruction::LeI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Greater => Instruction::GtI64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::GreaterEqual => Instruction::GeI64 {
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
                    }
                } else if is_f32 {
                    match op {
                        MirBinaryOp::Add => Instruction::AddF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Subtract => Instruction::SubF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Multiply => Instruction::MulF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Divide => Instruction::DivF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Remainder => Instruction::RemF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Equal => Instruction::EqF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::NotEqual => Instruction::NeF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Less => Instruction::LtF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::LessEqual => Instruction::LeF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Greater => Instruction::GtF32 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::GreaterEqual => Instruction::GeF32 {
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
                    }
                } else if is_f64 {
                    match op {
                        MirBinaryOp::Add => Instruction::AddF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Subtract => Instruction::SubF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Multiply => Instruction::MulF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Divide => Instruction::DivF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Remainder => Instruction::RemF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Equal => Instruction::EqF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::NotEqual => Instruction::NeF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Less => Instruction::LtF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::LessEqual => Instruction::LeF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::Greater => Instruction::GtF64 {
                            dest,
                            lhs: lhs_reg,
                            rhs: rhs_reg,
                        },
                        MirBinaryOp::GreaterEqual => Instruction::GeF64 {
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
                    }
                } else {
                    match op {
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
                    }
                };
                self.instructions.push(instr);
                self.free_temps(cast_temp_count);
                if immediate.is_none() {
                    self.free_temp_if_operand(rhs);
                }
                self.free_temp_if_operand(lhs);
            }
            RValue::Cast(operand, ty) => self.emit_cast(dest, operand, *ty),
            RValue::Copy(operand) => self.emit_copy(dest, operand),
            RValue::Instanceof(operand, ty) => self.emit_instanceof(dest, operand, *ty),
            RValue::ChoiceVariantIs(operand, variant) => {
                self.emit_choice_variant_is(dest, operand, *variant)
            }
            RValue::ImportedChoiceVariantIs(operand, choice_name, variant_name) => {
                self.emit_imported_choice_variant_is(dest, operand, choice_name, variant_name)
            }
            RValue::LoadGlobal(name) => self.emit_load_global(dest, name),
            RValue::NewStruct {
                struct_type,
                fields,
            } => self.emit_new_struct(dest, *struct_type, fields),
            RValue::NewArray(element_type, elements) => {
                self.emit_new_array(dest, *element_type, elements)
            }
            RValue::NewArrayDynamic(array_type, elements) => {
                self.emit_new_array_dynamic(dest, *array_type, elements)
            }
            RValue::NewTuple(tuple_type, elements) => {
                self.emit_new_tuple(dest, *tuple_type, elements)
            }
            RValue::ArrayIndex(arr_operand, idx_operand) => {
                self.emit_array_index(dest, arr_operand, idx_operand)
            }
            RValue::MemberAccess(obj_operand, field_name) => {
                self.emit_member_access(dest, obj_operand, field_name)
            }
            RValue::Choice(choice_type, variant_name, payload_operand) => {
                self.emit_choice(dest, *choice_type, variant_name, payload_operand.as_ref())
            }
            RValue::Len(operand) => self.emit_len(dest, operand),
            RValue::NewArrayZeroed {
                array_type, size, ..
            } => self.emit_new_array_zeroed(dest, *array_type, *size),
            RValue::NewArrayZeroedDynamic {
                array_type, length, ..
            } => self.emit_new_array_zeroed_dynamic(dest, *array_type, length),
            RValue::CreateFuture { func, args, .. } => self.emit_create_future(dest, *func, args),
            RValue::CreateIndirectFuture { func, args } => {
                self.emit_create_indirect_future(dest, func, args)
            }
        }
    }
}
