use crate::thread;

use super::*;

macro_rules! impl_binary_op {
    ($self:expr, $thread:expr, $dest:expr, $lhs:expr, $rhs:expr, +) => {{
        let lhs_val = $thread.read_reg($lhs);
        let rhs_val = $thread.read_reg($rhs);
        let res = match (lhs_val, rhs_val) {
            (Value::Int8(l), Value::Int8(r)) => Value::Int8(l.wrapping_add(r)),
            (Value::Int16(l), Value::Int16(r)) => Value::Int16(l.wrapping_add(r)),
            (Value::Int32(l), Value::Int32(r)) => Value::Int32(l.wrapping_add(r)),
            (Value::Int64(l), Value::Int64(r)) => Value::Int64(l.wrapping_add(r)),
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
                return Err(VmError::TypeMismatch {
                    expected: "matching numeric types".to_string(),
                    found: format!("{:?} and {:?}", l, r),
                });
            }
        };
        $thread.write_reg($dest, res);
    }};
    ($self:expr, $thread:expr, $dest:expr, $lhs:expr, $rhs:expr, -) => {{
        let lhs_val = $thread.read_reg($lhs);
        let rhs_val = $thread.read_reg($rhs);
        let res = match (lhs_val, rhs_val) {
            (Value::Int8(l), Value::Int8(r)) => Value::Int8(l.wrapping_sub(r)),
            (Value::Int16(l), Value::Int16(r)) => Value::Int16(l.wrapping_sub(r)),
            (Value::Int32(l), Value::Int32(r)) => Value::Int32(l.wrapping_sub(r)),
            (Value::Int64(l), Value::Int64(r)) => Value::Int64(l.wrapping_sub(r)),
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
                return Err(VmError::TypeMismatch {
                    expected: "matching numeric types".to_string(),
                    found: format!("{:?} and {:?}", l, r),
                });
            }
        };
        $thread.write_reg($dest, res);
    }};
    ($self:expr, $thread:expr, $dest:expr, $lhs:expr, $rhs:expr, *) => {{
        let lhs_val = $thread.read_reg($lhs);
        let rhs_val = $thread.read_reg($rhs);
        let res = match (lhs_val, rhs_val) {
            (Value::Int8(l), Value::Int8(r)) => Value::Int8(l.wrapping_mul(r)),
            (Value::Int16(l), Value::Int16(r)) => Value::Int16(l.wrapping_mul(r)),
            (Value::Int32(l), Value::Int32(r)) => Value::Int32(l.wrapping_mul(r)),
            (Value::Int64(l), Value::Int64(r)) => Value::Int64(l.wrapping_mul(r)),
            (Value::Uint8(l), Value::Uint8(r)) => Value::Uint8(l.wrapping_mul(r)),
            (Value::Uint16(l), Value::Uint16(r)) => Value::Uint16(l.wrapping_mul(r)),
            (Value::Uint32(l), Value::Uint32(r)) => Value::Uint32(l.wrapping_mul(r)),
            (Value::Uint64(l), Value::Uint64(r)) => Value::Uint64(l.wrapping_mul(r)),
            (Value::Float32(l), Value::Float32(r)) => {
                Value::Float32(galfus_core::normalize_f32(l * r))
            }
            (Value::Float64(l), Value::Float64(r)) => {
                Value::Float64(galfus_core::normalize_f64(l * r))
            }
            (l, r) => {
                return Err(VmError::TypeMismatch {
                    expected: "matching numeric types".to_string(),
                    found: format!("{:?} and {:?}", l, r),
                });
            }
        };
        $thread.write_reg($dest, res);
    }};
}

macro_rules! impl_bitwise_op {
    ($self:expr, $thread:expr, $dest:expr, $lhs:expr, $rhs:expr, $op:tt) => {{
        let lhs_val = $thread.read_reg($lhs);
        let rhs_val = $thread.read_reg($rhs);
        let res = match (lhs_val, rhs_val) {
            (Value::Bool(l), Value::Bool(r)) => Value::Bool(l $op r),
            (Value::Int8(l), Value::Int8(r)) => Value::Int8(l $op r),
            (Value::Int16(l), Value::Int16(r)) => Value::Int16(l $op r),
            (Value::Int32(l), Value::Int32(r)) => Value::Int32(l $op r),
            (Value::Int64(l), Value::Int64(r)) => Value::Int64(l $op r),
            (Value::Uint8(l), Value::Uint8(r)) => Value::Uint8(l $op r),
            (Value::Uint16(l), Value::Uint16(r)) => Value::Uint16(l $op r),
            (Value::Uint32(l), Value::Uint32(r)) => Value::Uint32(l $op r),
            (Value::Uint64(l), Value::Uint64(r)) => Value::Uint64(l $op r),
            (l, r) => return Err(VmError::TypeMismatch {
                expected: "matching integer or boolean types".to_string(),
                found: format!("{:?} and {:?}", l, r),
            }),
        };
        $thread.write_reg($dest, res);
    }};
}

impl VirtualMachine {
    pub(super) fn execute_operator_instruction(
        &self,
        thread: &mut thread::VmThreadState,
        instr: &Instruction,
    ) -> Result<VmStep, VmError> {
        match *instr {
            // Category B: Unary & Binary Operations
            Instruction::BinaryImmediate {
                dest,
                lhs,
                operation,
                rhs,
            } => {
                let value = super::immediate_binary_value(thread.read_reg(lhs), rhs, operation)?;
                thread.write_reg(dest, value);
            }
            Instruction::Add { dest, lhs, rhs } => {
                impl_binary_op!(self, thread, dest, lhs, rhs, +);
            }
            Instruction::Sub { dest, lhs, rhs } => {
                impl_binary_op!(self, thread, dest, lhs, rhs, -);
            }
            Instruction::Mul { dest, lhs, rhs } => {
                impl_binary_op!(self, thread, dest, lhs, rhs, *);
            }
            Instruction::Div { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let res = match (lhs_val, rhs_val) {
                    (Value::Int8(l), Value::Int8(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int8(l / r)
                    }
                    (Value::Int16(l), Value::Int16(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int16(l / r)
                    }
                    (Value::Int32(l), Value::Int32(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int32(l / r)
                    }
                    (Value::Int64(l), Value::Int64(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int64(l / r)
                    }
                    (Value::Uint8(l), Value::Uint8(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint8(l / r)
                    }
                    (Value::Uint16(l), Value::Uint16(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint16(l / r)
                    }
                    (Value::Uint32(l), Value::Uint32(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint32(l / r)
                    }
                    (Value::Uint64(l), Value::Uint64(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint64(l / r)
                    }
                    (Value::Float32(l), Value::Float32(r)) => {
                        Value::Float32(galfus_core::normalize_f32(l / r))
                    }
                    (Value::Float64(l), Value::Float64(r)) => {
                        Value::Float64(galfus_core::normalize_f64(l / r))
                    }
                    (l, r) => {
                        return Err(VmError::TypeMismatch {
                            expected: "matching numeric types".to_string(),
                            found: format!("{:?} and {:?}", l, r),
                        });
                    }
                };
                thread.write_reg(dest, res);
            }
            Instruction::Rem { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let res = match (lhs_val, rhs_val) {
                    (Value::Int8(l), Value::Int8(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int8(l % r)
                    }
                    (Value::Int16(l), Value::Int16(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int16(l % r)
                    }
                    (Value::Int32(l), Value::Int32(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int32(l % r)
                    }
                    (Value::Int64(l), Value::Int64(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Int64(l % r)
                    }
                    (Value::Uint8(l), Value::Uint8(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint8(l % r)
                    }
                    (Value::Uint16(l), Value::Uint16(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint16(l % r)
                    }
                    (Value::Uint32(l), Value::Uint32(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint32(l % r)
                    }
                    (Value::Uint64(l), Value::Uint64(r)) => {
                        if r == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        Value::Uint64(l % r)
                    }
                    (l, r) => {
                        return Err(VmError::TypeMismatch {
                            expected: "matching integer types".to_string(),
                            found: format!("{:?} and {:?}", l, r),
                        });
                    }
                };
                thread.write_reg(dest, res);
            }
            Instruction::Pow { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let res = self.pow_values(lhs_val, rhs_val)?;
                thread.write_reg(dest, res);
            }
            Instruction::Neg { dest, src } => {
                let val = thread.read_reg(src);
                let res = match val {
                    Value::Int8(x) => Value::Int8(x.wrapping_neg()),
                    Value::Int16(x) => Value::Int16(x.wrapping_neg()),
                    Value::Int32(x) => Value::Int32(x.wrapping_neg()),
                    Value::Int64(x) => Value::Int64(x.wrapping_neg()),
                    Value::Float32(x) => Value::Float32(galfus_core::normalize_f32(-x)),
                    Value::Float64(x) => Value::Float64(galfus_core::normalize_f64(-x)),
                    x => {
                        return Err(VmError::TypeMismatch {
                            expected: "signed numeric type".to_string(),
                            found: format!("{:?}", x),
                        });
                    }
                };
                thread.write_reg(dest, res);
            }
            Instruction::Not { dest, src } => {
                let val = thread.read_reg(src);
                let res = match val {
                    Value::Bool(b) => Value::Bool(!b),
                    x => {
                        return Err(VmError::TypeMismatch {
                            expected: "boolean type".to_string(),
                            found: format!("{:?}", x),
                        });
                    }
                };
                thread.write_reg(dest, res);
            }
            Instruction::BitNot { dest, src } => {
                let val = thread.read_reg(src);
                let res = match val {
                    Value::Int8(x) => Value::Int8(!x),
                    Value::Int16(x) => Value::Int16(!x),
                    Value::Int32(x) => Value::Int32(!x),
                    Value::Int64(x) => Value::Int64(!x),
                    Value::Uint8(x) => Value::Uint8(!x),
                    Value::Uint16(x) => Value::Uint16(!x),
                    Value::Uint32(x) => Value::Uint32(!x),
                    Value::Uint64(x) => Value::Uint64(!x),
                    x => {
                        return Err(VmError::TypeMismatch {
                            expected: "integer type".to_string(),
                            found: format!("{:?}", x),
                        });
                    }
                };
                thread.write_reg(dest, res);
            }
            Instruction::Shl { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let shift = self.to_shift_amount(rhs_val)?;
                let res = match lhs_val {
                    Value::Int8(l) => Value::Int8(l.wrapping_shl(shift)),
                    Value::Int16(l) => Value::Int16(l.wrapping_shl(shift)),
                    Value::Int32(l) => Value::Int32(l.wrapping_shl(shift)),
                    Value::Int64(l) => Value::Int64(l.wrapping_shl(shift)),
                    Value::Uint8(l) => Value::Uint8(l.wrapping_shl(shift)),
                    Value::Uint16(l) => Value::Uint16(l.wrapping_shl(shift)),
                    Value::Uint32(l) => Value::Uint32(l.wrapping_shl(shift)),
                    Value::Uint64(l) => Value::Uint64(l.wrapping_shl(shift)),
                    l => {
                        return Err(VmError::TypeMismatch {
                            expected: "integer type for shift".to_string(),
                            found: format!("{:?}", l),
                        });
                    }
                };
                thread.write_reg(dest, res);
            }
            Instruction::Shr { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let shift = self.to_shift_amount(rhs_val)?;
                let res = match lhs_val {
                    Value::Int8(l) => Value::Int8(l.wrapping_shr(shift)),
                    Value::Int16(l) => Value::Int16(l.wrapping_shr(shift)),
                    Value::Int32(l) => Value::Int32(l.wrapping_shr(shift)),
                    Value::Int64(l) => Value::Int64(l.wrapping_shr(shift)),
                    Value::Uint8(l) => Value::Uint8(l.wrapping_shr(shift)),
                    Value::Uint16(l) => Value::Uint16(l.wrapping_shr(shift)),
                    Value::Uint32(l) => Value::Uint32(l.wrapping_shr(shift)),
                    Value::Uint64(l) => Value::Uint64(l.wrapping_shr(shift)),
                    l => {
                        return Err(VmError::TypeMismatch {
                            expected: "integer type for shift".to_string(),
                            found: format!("{:?}", l),
                        });
                    }
                };
                thread.write_reg(dest, res);
            }
            Instruction::And { dest, lhs, rhs } => {
                impl_bitwise_op!(self, thread, dest, lhs, rhs, &);
            }
            Instruction::Or { dest, lhs, rhs } => {
                impl_bitwise_op!(self, thread, dest, lhs, rhs, |);
            }
            Instruction::Xor { dest, lhs, rhs } => {
                impl_bitwise_op!(self, thread, dest, lhs, rhs, ^);
            }
            Instruction::Eq { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                thread.write_reg(dest, Value::Bool(lhs_val == rhs_val));
            }
            Instruction::Ne { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                thread.write_reg(dest, Value::Bool(lhs_val != rhs_val));
            }
            Instruction::Lt { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let cmp = self.compare_values(&lhs_val, &rhs_val)?;
                thread.write_reg(dest, Value::Bool(cmp.is_some_and(|o| o.is_lt())));
            }
            Instruction::Le { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let cmp = self.compare_values(&lhs_val, &rhs_val)?;
                thread.write_reg(dest, Value::Bool(cmp.is_some_and(|o| o.is_le())));
            }
            Instruction::Gt { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let cmp = self.compare_values(&lhs_val, &rhs_val)?;
                thread.write_reg(dest, Value::Bool(cmp.is_some_and(|o| o.is_gt())));
            }
            Instruction::Ge { dest, lhs, rhs } => {
                let lhs_val = thread.read_reg(lhs);
                let rhs_val = thread.read_reg(rhs);
                let cmp = self.compare_values(&lhs_val, &rhs_val)?;
                thread.write_reg(dest, Value::Bool(cmp.is_some_and(|o| o.is_ge())));
            }
            Instruction::Fallback {
                dest,
                src,
                fallback,
            } => {
                let src_val = thread.read_reg(src);
                let fallback_val = thread.read_reg(fallback);
                let val = match src_val {
                    Value::Null => fallback_val,
                    _ => src_val,
                };
                thread.write_reg(dest, val);
            }

            _ => unreachable!("instruction routed to the wrong runtime handler"),
        }

        Ok(VmStep::Continue)
    }
}
