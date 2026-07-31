use super::DeclarationTypeChecker;
use crate::{FunctionType, TypeKind};
use galfus_core::TypeId;

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn is_assignable(&self, expected: TypeId, actual: TypeId) -> bool {
        let expected = self.resolve_path_type(expected);
        let actual = self.resolve_path_type(actual);

        if expected == actual {
            return true;
        }

        let expected_kind = self.layer.table().kind(expected);
        let actual_kind = self.layer.table().kind(actual);

        if matches!(expected_kind, Some(TypeKind::Error)) {
            return true;
        }

        if matches!(actual_kind, Some(TypeKind::Error)) {
            return true;
        }

        match (expected_kind, actual_kind) {
            (Some(TypeKind::Union { members }), _) => members
                .iter()
                .copied()
                .any(|member| self.is_assignable(member, actual)),

            (_, Some(TypeKind::Union { members })) => members
                .iter()
                .copied()
                .all(|member| self.is_assignable(expected, member)),

            (_, Some(TypeKind::GenericParameter { symbol })) => {
                if let Some(arg_bound) = self.generic_parameter_bound_type(*symbol) {
                    self.is_assignable(expected, arg_bound)
                } else {
                    false
                }
            }

            (
                Some(TypeKind::Array {
                    element: expected_element,
                }),
                Some(TypeKind::Array {
                    element: actual_element,
                }),
            ) => self.is_assignable(*expected_element, *actual_element),

            (
                Some(TypeKind::Primitive(expected_primitive)),
                Some(TypeKind::Primitive(actual_primitive)),
            ) => {
                if expected_primitive == actual_primitive {
                    true
                } else {
                    (expected_primitive.is_int() && actual_primitive.is_int())
                        || (expected_primitive.is_uint() && actual_primitive.is_uint())
                        || (expected_primitive.is_float() && actual_primitive.is_float())
                }
            }

            (
                Some(TypeKind::Function(expected_function)),
                Some(TypeKind::Function(actual_function)),
            ) => self.is_function_type_assignable(expected_function, actual_function),

            _ => false,
        }
    }

    fn is_function_type_assignable(&self, expected: &FunctionType, actual: &FunctionType) -> bool {
        if expected.parameters().len() != actual.parameters().len() {
            return false;
        }

        for (expected_parameter, actual_parameter) in
            expected.parameters().iter().zip(actual.parameters().iter())
        {
            if expected_parameter.is_rest() != actual_parameter.is_rest() {
                return false;
            }

            if expected_parameter.has_default() != actual_parameter.has_default() {
                return false;
            }

            if !self.is_assignable(expected_parameter.ty(), actual_parameter.ty()) {
                return false;
            }
        }

        self.is_assignable(expected.return_type(), actual.return_type())
    }
}
