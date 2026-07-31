use super::DeclarationTypeChecker;
use crate::builtin_constraints::{
    BUILTIN_CONSTRAINTS, BuiltinConstraint, BuiltinConstraintFunctionSignature, builtin_constraint,
    is_builtin_constraint,
};
use crate::{FunctionParameterType, PrimitiveType, SymbolKind, TypeKind};
use galfus_core::{SymbolId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn bind_builtin_constraint_symbol_types(&mut self) {
        let Some(resolution) = self.graph.resolution() else {
            return;
        };

        let symbols = resolution.symbols().to_vec();

        for symbol in symbols {
            match symbol.kind() {
                SymbolKind::Constraint => {
                    if is_builtin_constraint(symbol.name()) {
                        let ty = self.layer.table_mut().intern_named(symbol.id());
                        self.layer.bind_symbol_type(symbol.id(), ty);
                    }
                }
                SymbolKind::GenericParameter => {
                    let ty = self.layer.table_mut().intern_generic_parameter(symbol.id());
                    self.layer.bind_symbol_type(symbol.id(), ty);
                }
                _ => {}
            }
        }

        self.bind_builtin_constraint_signatures();
    }

    pub(super) fn builtin_constraint_generic_parameters(
        &self,
        constraint_symbol: SymbolId,
    ) -> Option<Vec<SymbolId>> {
        let name = self.symbol_name(constraint_symbol)?;

        let metadata = builtin_constraint(name.as_str())?;
        self.builtin_constraint_parameters_by_name(constraint_symbol, metadata.generic_parameters)
    }

    pub(super) fn iterable_item_type(&mut self, source_type: TypeId) -> Option<TypeId> {
        let source_type = self.resolve_alias_type(source_type);

        match self.layer.table().kind(source_type).cloned() {
            Some(TypeKind::Array { element }) => return Some(element),
            Some(TypeKind::Range { element }) => return Some(element),
            Some(TypeKind::Error) => return Some(source_type),
            _ => {}
        }

        let application = self.satisfied_constraint_application(source_type, "Iterable")?;
        let parameters = self.constraint_generic_parameters(application.symbol);

        let item_parameter = *parameters.get(1)?;
        let iter_parameter = *parameters.get(2)?;

        let item_type = *application.substitution.get(&item_parameter)?;
        let iter_type = *application.substitution.get(&iter_parameter)?;

        if !self.type_satisfies_iterator(iter_type, item_type) {
            return None;
        }

        Some(item_type)
    }

    fn type_satisfies_iterator(&mut self, iter_type: TypeId, item_type: TypeId) -> bool {
        let Some(application) = self.satisfied_constraint_application(iter_type, "Iterator") else {
            return false;
        };

        let parameters = self.constraint_generic_parameters(application.symbol);

        let Some(iter_parameter) = parameters.first().copied() else {
            return false;
        };

        let Some(item_parameter) = parameters.get(1).copied() else {
            return false;
        };

        let Some(declared_iter_type) = application.substitution.get(&iter_parameter).copied()
        else {
            return false;
        };

        let Some(declared_item_type) = application.substitution.get(&item_parameter).copied()
        else {
            return false;
        };

        self.is_same_type(declared_iter_type, iter_type)
            && self.is_same_type(declared_item_type, item_type)
    }

    fn is_same_type(&self, left: TypeId, right: TypeId) -> bool {
        self.is_assignable(left, right) && self.is_assignable(right, left)
    }

    fn bind_builtin_constraint_signatures(&mut self) {
        for metadata in BUILTIN_CONSTRAINTS {
            self.bind_builtin_constraint_signature(metadata);
        }
    }

    fn bind_builtin_constraint_signature(&mut self, metadata: &BuiltinConstraint) {
        let Some((constraint, parameters)) = self.builtin_constraint_with_parameters(metadata)
        else {
            return;
        };

        for function in metadata.functions {
            let Some(symbol) = self.builtin_constraint_function(constraint, function.name) else {
                continue;
            };
            let Some(function_type) =
                self.builtin_constraint_function_type(function.signature, parameters.as_slice())
            else {
                continue;
            };

            self.layer.bind_symbol_type(symbol, function_type);
        }
    }

    fn builtin_constraint_function_type(
        &mut self,
        signature: BuiltinConstraintFunctionSignature,
        parameters: &[SymbolId],
    ) -> Option<TypeId> {
        let mut parameter_type = |index| {
            parameters
                .get(index)
                .copied()
                .map(|symbol| self.layer.table_mut().intern_generic_parameter(symbol))
        };

        match signature {
            BuiltinConstraintFunctionSignature::IteratorNext => {
                let self_type = parameter_type(0)?;
                let item_type = parameter_type(1)?;
                let null_type = self.layer.table().primitive(PrimitiveType::Null);
                let return_type = self.layer.table_mut().intern_union([item_type, null_type]);
                Some(
                    self.layer
                        .table_mut()
                        .intern_function(vec![FunctionParameterType::new(self_type)], return_type),
                )
            }
            BuiltinConstraintFunctionSignature::IterableIter => {
                let self_type = parameter_type(0)?;
                let iter_type = parameter_type(2)?;
                Some(
                    self.layer
                        .table_mut()
                        .intern_function(vec![FunctionParameterType::new(self_type)], iter_type),
                )
            }
            BuiltinConstraintFunctionSignature::ComparableCompare => {
                let pattern_type = parameter_type(0)?;
                let value_type = parameter_type(1)?;
                let bool_type = self.layer.table().primitive(PrimitiveType::Bool);
                Some(self.layer.table_mut().intern_function(
                    vec![
                        FunctionParameterType::new(pattern_type),
                        FunctionParameterType::new(value_type),
                    ],
                    bool_type,
                ))
            }
        }
    }

    fn builtin_constraint_with_parameters(
        &self,
        metadata: &BuiltinConstraint,
    ) -> Option<(SymbolId, Vec<SymbolId>)> {
        let constraint = self.builtin_constraint_symbol(metadata.name)?;
        let parameters =
            self.builtin_constraint_parameters_by_name(constraint, metadata.generic_parameters)?;
        Some((constraint, parameters))
    }

    fn builtin_constraint_symbol(&self, name: &str) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;
        let builtin_scope = resolution.builtin_scope()?;
        let symbol = resolution.scope(builtin_scope)?.symbol(name)?;
        let symbol_data = resolution.symbol(symbol)?;

        if symbol_data.kind() != SymbolKind::Constraint {
            return None;
        }

        Some(symbol)
    }

    fn builtin_constraint_parameters_by_name(
        &self,
        constraint: SymbolId,
        names: &[&str],
    ) -> Option<Vec<SymbolId>> {
        let resolution = self.graph.resolution()?;
        let member_scope = resolution.member_scope(constraint)?;
        let scope = resolution.scope(member_scope)?;

        names
            .iter()
            .map(|name| {
                let symbol = scope.symbol(name)?;
                let symbol_data = resolution.symbol(symbol)?;

                if symbol_data.kind() != SymbolKind::GenericParameter {
                    return None;
                }

                Some(symbol)
            })
            .collect()
    }

    fn builtin_constraint_function(
        &self,
        constraint: SymbolId,
        function_name: &str,
    ) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;
        let member_scope = resolution.member_scope(constraint)?;
        let scope = resolution.scope(member_scope)?;
        let symbol = scope.symbol(function_name)?;
        let symbol_data = resolution.symbol(symbol)?;

        if symbol_data.kind() != SymbolKind::ConstraintFunction {
            return None;
        }

        Some(symbol)
    }
}
