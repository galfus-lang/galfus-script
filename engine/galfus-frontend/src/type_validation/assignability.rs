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

        if self.imported_struct_satisfies(actual, expected) {
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

            (
                Some(TypeKind::GenericInstance {
                    base: expected_base,
                    arguments: expected_arguments,
                }),
                Some(TypeKind::GenericInstance {
                    base: actual_base,
                    arguments: actual_arguments,
                }),
            ) => {
                expected_arguments.len() == actual_arguments.len()
                    && (self.is_assignable(*expected_base, *actual_base)
                        || (self.is_future_type_base(*expected_base)
                            && self.is_future_type_base(*actual_base)))
                    && expected_arguments
                        .iter()
                        .zip(actual_arguments)
                        .all(|(expected, actual)| self.is_assignable(*expected, *actual))
            }

            _ => false,
        }
    }

    fn imported_struct_satisfies(&self, actual: TypeId, expected: TypeId) -> bool {
        let actual = self.resolve_alias_type(actual);
        let constraints = match self.layer.table().kind(actual) {
            Some(TypeKind::Named { symbol }) => *symbol,
            Some(TypeKind::GenericInstance { base, .. }) => {
                let Some(TypeKind::Named { symbol }) = self.layer.table().kind(*base) else {
                    return self.imported_path_struct_satisfies(actual, expected);
                };
                *symbol
            }
            _ => return self.imported_path_struct_satisfies(actual, expected),
        };

        self.imported_struct_constraints
            .get(&constraints)
            .is_some_and(|constraints| constraints.iter().any(|constraint| *constraint == expected))
    }

    fn imported_path_struct_satisfies(&self, actual: TypeId, expected: TypeId) -> bool {
        let Some(TypeKind::Path { segments, .. }) = self.layer.table().kind(actual) else {
            return false;
        };
        let Some(name) = segments.last() else {
            return false;
        };
        self.imported_struct_constraints_by_name
            .get(name)
            .is_some_and(|constraints| constraints.iter().any(|constraint| *constraint == expected))
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
