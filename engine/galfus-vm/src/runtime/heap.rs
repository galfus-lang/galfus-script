use std::cmp;
use std::collections;

use crate::thread;

use super::*;

impl VirtualMachine {
    pub(super) fn to_shift_amount(&self, val: Value) -> Result<u32, VmError> {
        match val {
            Value::Int8(x) => Ok(x as u32),
            Value::Int16(x) => Ok(x as u32),
            Value::Int32(x) => Ok(x as u32),
            Value::Int64(x) => Ok(x as u32),
            Value::Uint8(x) => Ok(x as u32),
            Value::Uint16(x) => Ok(x as u32),
            Value::Uint32(x) => Ok(x),
            Value::Uint64(x) => Ok(x as u32),
            x => Err(VmError::TypeMismatch {
                expected: "integer shift amount".to_string(),
                found: format!("{:?}", x),
            }),
        }
    }

    pub(super) fn to_raw_array_index(&self, val: Value) -> Result<i128, VmError> {
        match val {
            Value::Int8(x) => Ok(i128::from(x)),
            Value::Int16(x) => Ok(i128::from(x)),
            Value::Int32(x) => Ok(i128::from(x)),
            Value::Int64(x) => Ok(i128::from(x)),
            Value::Uint8(x) => Ok(i128::from(x)),
            Value::Uint16(x) => Ok(i128::from(x)),
            Value::Uint32(x) => Ok(i128::from(x)),
            Value::Uint64(x) => Ok(i128::from(x)),
            x => Err(VmError::TypeMismatch {
                expected: "integer array index".to_string(),
                found: format!("{:?}", x),
            }),
        }
    }

    pub(super) fn resolve_raw_array_index(&self, raw_index: i128, len: usize) -> Option<usize> {
        let len = len as i128;
        let resolved = if raw_index < 0 {
            len + raw_index
        } else {
            raw_index
        };

        if resolved < 0 || resolved >= len {
            return None;
        }

        Some(resolved as usize)
    }

    pub(super) fn pow_values(&self, lhs: Value, rhs: Value) -> Result<Value, VmError> {
        let res = match (lhs, rhs) {
            (Value::Int8(l), Value::Int8(r)) => Value::Int8(l.wrapping_pow(r as u32)),
            (Value::Int16(l), Value::Int16(r)) => Value::Int16(l.wrapping_pow(r as u32)),
            (Value::Int32(l), Value::Int32(r)) => Value::Int32(l.wrapping_pow(r as u32)),
            (Value::Int64(l), Value::Int64(r)) => Value::Int64(l.wrapping_pow(r as u32)),
            (Value::Uint8(l), Value::Uint8(r)) => Value::Uint8(l.wrapping_pow(r as u32)),
            (Value::Uint16(l), Value::Uint16(r)) => Value::Uint16(l.wrapping_pow(r as u32)),
            (Value::Uint32(l), Value::Uint32(r)) => Value::Uint32(l.wrapping_pow(r)),
            (Value::Uint64(l), Value::Uint64(r)) => Value::Uint64(l.wrapping_pow(r as u32)),
            (Value::Float32(l), Value::Float32(r)) => {
                Value::Float32(galfus_core::normalize_f32(l.powf(r)))
            }
            (Value::Float64(l), Value::Float64(r)) => {
                Value::Float64(galfus_core::normalize_f64(l.powf(r)))
            }
            (l, r) => {
                return Err(VmError::TypeMismatch {
                    expected: "matching numeric types".to_string(),
                    found: format!("{:?} and {:?}", l, r),
                });
            }
        };
        Ok(res)
    }

    pub(super) fn compare_values(
        &self,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<Option<cmp::Ordering>, VmError> {
        let ord = match (lhs, rhs) {
            (Value::Bool(l), Value::Bool(r)) => Some(l.cmp(r)),
            (Value::Int8(l), Value::Int8(r)) => Some(l.cmp(r)),
            (Value::Int16(l), Value::Int16(r)) => Some(l.cmp(r)),
            (Value::Int32(l), Value::Int32(r)) => Some(l.cmp(r)),
            (Value::Int64(l), Value::Int64(r)) => Some(l.cmp(r)),
            (Value::Uint8(l), Value::Uint8(r)) => Some(l.cmp(r)),
            (Value::Uint16(l), Value::Uint16(r)) => Some(l.cmp(r)),
            (Value::Uint32(l), Value::Uint32(r)) => Some(l.cmp(r)),
            (Value::Uint64(l), Value::Uint64(r)) => Some(l.cmp(r)),
            (Value::Float32(l), Value::Float32(r)) => l.partial_cmp(r),
            (Value::Float64(l), Value::Float64(r)) => l.partial_cmp(r),
            (l, r) => {
                return Err(VmError::TypeMismatch {
                    expected: "matching comparable types".to_string(),
                    found: format!("{:?} and {:?}", l, r),
                });
            }
        };
        Ok(ord)
    }

    pub(super) fn check_value_type(
        &self,
        thread: &thread::VmThreadState,
        val: &Value,
        expected_ty: TypeIdx,
    ) -> bool {
        let Ok(image) = self.current_image(thread) else { return false; };
        let ty = match image
            .types
            .get(expected_ty.raw() as usize)
        {
            Some(t) => t,
            None => return false,
        };
        match (val, ty) {
            (Value::Null, BytecodeType::Null) => true,
            (Value::Bool(_), BytecodeType::Bool) => true,
            (Value::Int8(_), BytecodeType::Int8) => true,
            (Value::Int16(_), BytecodeType::Int16) => true,
            (Value::Int32(_), BytecodeType::Int32) => true,
            (Value::Int64(_), BytecodeType::Int64) => true,
            (Value::Uint8(_), BytecodeType::Uint8) => true,
            (Value::Uint16(_), BytecodeType::Uint16) => true,
            (Value::Uint32(_), BytecodeType::Uint32) => true,
            (Value::Uint64(_), BytecodeType::Uint64) => true,
            (Value::Float32(_), BytecodeType::Float32) => true,
            (Value::Float64(_), BytecodeType::Float64) => true,
            (Value::Null, BytecodeType::Nullable(_)) => true,
            (value, BytecodeType::Nullable(inner)) => self.check_value_type(thread, value, *inner),
            (Value::Object(obj_ref), BytecodeType::Struct(expected_layout_idx)) => {
                if let Ok(HeapObject::Struct { layout_idx, .. }) = thread.heap.get_object(*obj_ref)
                {
                    *layout_idx == *expected_layout_idx
                } else {
                    false
                }
            }
            (Value::Object(obj_ref), BytecodeType::Array(expected_el_ty)) => {
                if let Ok(HeapObject::Array { element_ty, .. }) = thread.heap.get_object(*obj_ref) {
                    self.type_idx_matches(thread, *element_ty, *expected_el_ty)
                } else {
                    false
                }
            }
            (Value::Object(obj_ref), BytecodeType::Tuple(expected_tys)) => {
                if let Ok(HeapObject::Tuple { elements }) = thread.heap.get_object(*obj_ref) {
                    if elements.len() == expected_tys.len() {
                        elements
                            .iter()
                            .zip(expected_tys.iter())
                            .all(|(v, &ty)| self.check_value_type(thread, v, ty))
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            (Value::Object(obj_ref), BytecodeType::Choice(expected_layout_idx)) => {
                if let Ok(HeapObject::Choice { layout_idx, .. }) = thread.heap.get_object(*obj_ref)
                {
                    *layout_idx == *expected_layout_idx
                } else {
                    false
                }
            }
            (
                Value::Object(obj_ref),
                BytecodeType::ChoiceVariant(expected_choice_idx, expected_variant_idx),
            ) => {
                if let Ok(HeapObject::Choice {
                    layout_idx,
                    variant_idx,
                    ..
                }) = thread.heap.get_object(*obj_ref)
                {
                    if *layout_idx == *expected_choice_idx && variant_idx == expected_variant_idx {
                        return true;
                    }

                    let Ok(image) = self.current_image(thread) else { return false; };
                    let Some(actual_layout) = image
                        .choice_layouts
                        .get(layout_idx.raw() as usize)
                    else {
                        return false;
                    };
                    let Some(expected_layout) = image
                        .choice_layouts
                        .get(expected_choice_idx.raw() as usize)
                    else {
                        return false;
                    };

                    actual_layout.name == expected_layout.name
                        && actual_layout
                            .variants
                            .get(*variant_idx as usize)
                            .map(|variant| variant.name.as_str())
                            == expected_layout
                                .variants
                                .get(*expected_variant_idx as usize)
                                .map(|variant| variant.name.as_str())
                } else {
                    false
                }
            }
            (Value::Object(obj_ref), BytecodeType::Constraint(expected_constraint)) => {
                if let Ok(HeapObject::Struct { layout_idx, .. }) = thread.heap.get_object(*obj_ref)
                {
                    let Ok(image) = self.current_image(thread) else { return false; };
                    image
                        .struct_layouts
                        .get(layout_idx.raw() as usize)
                        .is_some_and(|layout| layout.constraints.contains(expected_constraint))
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn type_idx_matches(
        &self,
        thread: &thread::VmThreadState,
        actual: TypeIdx,
        expected: TypeIdx,
    ) -> bool {
        self.type_idx_matches_inner(thread, actual, expected, &mut collections::HashSet::new())
    }

    fn type_idx_matches_inner(
        &self,
        thread: &thread::VmThreadState,
        actual: TypeIdx,
        expected: TypeIdx,
        seen: &mut collections::HashSet<(u16, u16)>,
    ) -> bool {
        if actual == expected {
            return true;
        }
        if !seen.insert((actual.raw(), expected.raw())) {
            return true;
        }

        let Ok(image) = self.current_image(thread) else { return false; };
        let Some(actual_ty) = image
            .types
            .get(actual.raw() as usize)
        else {
            return false;
        };
        let Some(expected_ty) = image
            .types
            .get(expected.raw() as usize)
        else {
            return false;
        };

        match (actual_ty, expected_ty) {
            (_, BytecodeType::Nullable(expected_inner)) => {
                self.type_idx_matches_inner(thread, actual, *expected_inner, seen)
            }
            (BytecodeType::Array(actual_el), BytecodeType::Array(expected_el)) => {
                self.type_idx_matches_inner(thread, *actual_el, *expected_el, seen)
            }
            (BytecodeType::Tuple(actual_elements), BytecodeType::Tuple(expected_elements)) => {
                actual_elements.len() == expected_elements.len()
                    && actual_elements.iter().zip(expected_elements.iter()).all(
                        |(&actual_el, &expected_el)| {
                            self.type_idx_matches_inner(thread, actual_el, expected_el, seen)
                        },
                    )
            }
            _ => actual_ty == expected_ty,
        }
    }
}
