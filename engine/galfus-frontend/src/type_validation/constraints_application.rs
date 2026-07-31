use super::constraints::{ConstraintApplication, ConstraintApplicationError, TypeSubstitution};
use super::{DeclarationTypeChecker, LoweredImportedConstraint};
use crate::{FunctionParameterType, PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn constraint_application(
        &mut self,
        type_node: NodeId,
    ) -> Result<ConstraintApplication, ConstraintApplicationError> {
        let Some(base) = self.constraint_application_base_node(type_node) else {
            return Err(ConstraintApplicationError::InvalidTarget);
        };

        if let Some(constraint_symbol) = self.constraint_symbol_from_base_node(base) {
            let Some(constraint_name) = self.symbol_name(constraint_symbol) else {
                return Err(ConstraintApplicationError::InvalidTarget);
            };

            let generic_parameters = self.constraint_generic_parameters(constraint_symbol);
            let explicit_arguments = self.constraint_application_argument_types(type_node);

            if generic_parameters.len() != explicit_arguments.len() {
                return Err(ConstraintApplicationError::GenericArgumentCountMismatch {
                    constraint_name,
                    expected: generic_parameters.len(),
                    actual: explicit_arguments.len(),
                });
            }

            let substitution = generic_parameters
                .into_iter()
                .zip(explicit_arguments)
                .collect::<TypeSubstitution>();

            return Ok(ConstraintApplication {
                symbol: constraint_symbol,
                constraint_name,
                substitution,
                imported_constraint: None,
            });
        }

        self.imported_constraint_application(base, type_node)
    }

    pub(super) fn constraint_application_base_node(&self, type_node: NodeId) -> Option<NodeId> {
        let generic_type = self.find_generic_type_node(type_node);

        if let Some(generic_type) = generic_type {
            let syntax = self.graph.syntax();
            let node = syntax.node(generic_type)?;

            return node.children().iter().copied().find(|child| {
                syntax
                    .node(*child)
                    .is_some_and(|child_node| child_node.kind() != SyntaxNodeKind::TypeArgumentList)
            });
        }

        Some(type_node)
    }

    fn constraint_symbol_from_base_node(&self, base: NodeId) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;

        if let Some(symbol) = resolution.type_reference_symbol(base) {
            return self.ensure_constraint_symbol(symbol);
        }

        if let Some(symbol) = resolution.type_path_reference_symbol(base) {
            return self.ensure_constraint_symbol(symbol);
        }

        let base_name = self.constraint_base_name(base)?;

        resolution
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == base_name && symbol.kind() == SymbolKind::Constraint)
            .map(|symbol| symbol.id())
    }

    fn ensure_constraint_symbol(&self, symbol: SymbolId) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;
        let symbol_data = resolution.symbol(symbol)?;

        if symbol_data.kind() != SymbolKind::Constraint {
            return None;
        }

        Some(symbol)
    }

    fn imported_constraint_application(
        &self,
        base: NodeId,
        type_node: NodeId,
    ) -> Result<ConstraintApplication, ConstraintApplicationError> {
        let Some((symbol, constraint)) = self.imported_constraint_for_base_node(base) else {
            return Err(ConstraintApplicationError::InvalidTarget);
        };

        let explicit_arguments = self.constraint_application_argument_types(type_node);

        if constraint.generic_parameters.len() != explicit_arguments.len() {
            return Err(ConstraintApplicationError::GenericArgumentCountMismatch {
                constraint_name: constraint.name.clone(),
                expected: constraint.generic_parameters.len(),
                actual: explicit_arguments.len(),
            });
        }

        let substitution = constraint
            .generic_parameters
            .iter()
            .copied()
            .zip(explicit_arguments)
            .collect::<TypeSubstitution>();

        Ok(ConstraintApplication {
            symbol,
            constraint_name: constraint.name.clone(),
            substitution,
            imported_constraint: Some(constraint.clone()),
        })
    }

    fn imported_constraint_for_base_node(
        &self,
        base: NodeId,
    ) -> Option<(SymbolId, &LoweredImportedConstraint)> {
        let resolution = self.graph.resolution()?;

        let type_ref = resolution.type_reference_symbol(base);
        if let Some(symbol) = type_ref
            && let Some(constraint) = self.imported_symbol_constraints.get(&symbol)
        {
            return Some((symbol, constraint));
        }

        if let Some(symbol) = resolution.type_path_reference_symbol(base)
            && let Some(constraint) = self.imported_symbol_constraints.get(&symbol)
        {
            return Some((symbol, constraint));
        }

        if let Some(constraint) = self.imported_path_constraints.get(&base) {
            let ty = self.layer.node_type(base)?;

            if let Some(TypeKind::Path { root, .. }) = self.layer.table().kind(ty) {
                return Some((*root, constraint));
            }
        }

        None
    }

    pub(super) fn constraint_base_name(&self, base: NodeId) -> Option<String> {
        let syntax = self.graph.syntax();
        let node = syntax.node(base)?;

        match node.kind() {
            SyntaxNodeKind::Identifier => Some(self.node_text(base)),

            SyntaxNodeKind::NamedType | SyntaxNodeKind::Path => {
                let identifier = syntax.first_child_of_kind(base, SyntaxNodeKind::Identifier)?;
                Some(self.node_text(identifier))
            }

            _ => None,
        }
    }

    pub(super) fn constraint_application_argument_types(&self, type_node: NodeId) -> Vec<TypeId> {
        let Some(generic_type) = self.find_generic_type_node(type_node) else {
            return Vec::new();
        };

        let Some(ty) = self.layer.node_type(generic_type) else {
            return Vec::new();
        };

        match self.layer.table().kind(ty) {
            Some(TypeKind::GenericInstance { arguments, .. }) => arguments.clone(),
            _ => Vec::new(),
        }
    }

    fn find_generic_type_node(&self, node: NodeId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::GenericType {
            return Some(node);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_generic_type_node(*child) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn constraint_generic_parameters(
        &self,
        constraint_symbol: SymbolId,
    ) -> Vec<SymbolId> {
        if let Some(parameters) = self.builtin_constraint_generic_parameters(constraint_symbol) {
            return parameters;
        }

        let Some(constraint_item) = self.constraint_item_for_symbol(constraint_symbol) else {
            return Vec::new();
        };

        self.declaration_symbols_in_node(constraint_item, &[SymbolKind::GenericParameter])
    }

    fn constraint_item_for_symbol(&self, constraint_symbol: SymbolId) -> Option<NodeId> {
        let constraint_name = self.symbol_name(constraint_symbol)?;
        let root = self.graph.syntax().root()?;

        self.find_constraint_item_by_name(root, constraint_name.as_str())
    }

    fn find_constraint_item_by_name(&self, node: NodeId, constraint_name: &str) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::ConstraintItem {
            let name = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)?;

            if self.node_text(name) == constraint_name {
                return Some(node);
            }

            return None;
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_constraint_item_by_name(*child, constraint_name) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn substitute_type(
        &mut self,
        ty: TypeId,
        substitution: &TypeSubstitution,
    ) -> TypeId {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty).cloned() {
            Some(TypeKind::GenericParameter { symbol }) => {
                substitution.get(&symbol).copied().unwrap_or(ty)
            }

            Some(TypeKind::Array { element }) => {
                let element = self.substitute_type(element, substitution);
                self.layer.table_mut().intern_array(element)
            }

            Some(TypeKind::Union { members }) => {
                let members = members
                    .into_iter()
                    .map(|member| self.substitute_type(member, substitution))
                    .collect::<Vec<_>>();

                self.layer.table_mut().intern_union(members)
            }

            Some(TypeKind::Tuple { elements }) => {
                let elements = elements
                    .into_iter()
                    .map(|element| self.substitute_type(element, substitution))
                    .collect::<Vec<_>>();

                self.layer.table_mut().intern_tuple(elements)
            }

            Some(TypeKind::Function(function)) => {
                let parameters = function
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        let ty = self.substitute_type(parameter.ty(), substitution);

                        if parameter.is_rest() {
                            return FunctionParameterType::rest(ty);
                        }

                        if parameter.has_default() {
                            return FunctionParameterType::with_default(ty);
                        }

                        FunctionParameterType::new(ty)
                    })
                    .collect::<Vec<_>>();

                let return_type = self.substitute_type(function.return_type(), substitution);

                self.layer
                    .table_mut()
                    .intern_function(parameters, return_type)
            }

            Some(TypeKind::Error) => ty,

            _ => ty,
        }
    }

    pub(super) fn satisfied_constraint_application(
        &mut self,
        ty: TypeId,
        constraint_name: &str,
    ) -> Option<ConstraintApplication> {
        let ty = self.resolve_alias_type(ty);

        let (symbol, struct_substitution) = match self.layer.table().kind(ty).cloned()? {
            TypeKind::Named { symbol } => (symbol, TypeSubstitution::new()),
            TypeKind::GenericInstance { base, arguments } => {
                let TypeKind::Named { symbol } = self.layer.table().kind(base).cloned()? else {
                    return None;
                };
                let struct_item = self.type_item_for_symbol(symbol)?;
                let parameters =
                    self.declaration_symbols_in_node(struct_item, &[SymbolKind::GenericParameter]);

                if parameters.len() != arguments.len() {
                    return None;
                }

                (symbol, parameters.into_iter().zip(arguments).collect())
            }
            _ => return None,
        };

        let resolution = self.graph.resolution()?;
        let symbol_data = resolution.symbol(symbol)?;

        if symbol_data.kind() != SymbolKind::Struct {
            return None;
        }

        let struct_item = self.type_item_for_symbol(symbol)?;
        let satisfies = self
            .graph
            .syntax()
            .first_child_of_kind(struct_item, SyntaxNodeKind::SatisfiesClause)?;

        let constraints = self
            .graph
            .syntax()
            .node(satisfies)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        for constraint_type in constraints {
            let Ok(application) = self.constraint_application(constraint_type) else {
                continue;
            };

            if application.constraint_name == constraint_name {
                let substitution = application
                    .substitution
                    .iter()
                    .map(|(parameter, value)| {
                        (
                            *parameter,
                            self.substitute_type(*value, &struct_substitution),
                        )
                    })
                    .collect();

                return Some(ConstraintApplication {
                    substitution,
                    ..application
                });
            }
        }

        None
    }

    pub(super) fn type_item_for_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
        let resolution = self.graph.resolution()?;
        let member_scope = resolution.member_scope(symbol)?;
        let scope = resolution.scope(member_scope)?;
        scope.owner()
    }

    pub(super) fn check_generic_parameter_constraint(&mut self, constraint: NodeId) {
        let Some(constraint_type) = self.first_constraint_type_child(constraint) else {
            return;
        };

        let target_name = self.node_text(constraint_type);

        match self.constraint_application(constraint_type) {
            Ok(_) => (),
            Err(ConstraintApplicationError::InvalidTarget) => {
                if self.is_valid_generic_bound_type(constraint_type) {
                    return;
                }

                self.report_invalid_satisfies_target(constraint_type, target_name.as_str());
            }
            Err(ConstraintApplicationError::GenericArgumentCountMismatch {
                constraint_name,
                expected,
                actual,
            }) => {
                self.report_constraint_generic_argument_count_mismatch(
                    constraint_type,
                    constraint_name.as_str(),
                    expected,
                    actual,
                );
            }
        }
    }

    pub(super) fn check_generic_parameter_bounds(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem {
            // Unbounded generic parameters are allowed (e.g. `fn arrayIter<T>(arr: [T])`).
        }

        for child in syntax_node.children() {
            self.check_generic_parameter_bounds(*child);
        }
    }

    fn is_valid_generic_bound_type(&mut self, type_node: NodeId) -> bool {
        let Some(ty) = self.layer.node_type(type_node) else {
            return false;
        };

        self.is_valid_generic_bound_type_id(ty)
    }

    fn is_valid_generic_bound_type_id(&mut self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty).cloned() {
            Some(TypeKind::Primitive(_)) => true,
            Some(TypeKind::Array { element }) => self.is_valid_generic_bound_type_id(element),
            Some(TypeKind::Union { members }) => members
                .into_iter()
                .all(|member| self.is_valid_generic_bound_type_id(member)),
            Some(TypeKind::Named { symbol }) => self
                .graph
                .resolution()
                .and_then(|resolution| resolution.symbol(symbol))
                .is_some_and(|symbol| symbol.kind() == SymbolKind::Constraint),
            Some(TypeKind::GenericInstance { .. }) => false,
            Some(TypeKind::Error) => true,
            _ => false,
        }
    }

    pub(super) fn validate_generic_substitution_bounds(
        &mut self,
        target: NodeId,
        substitution: &TypeSubstitution,
    ) {
        for (&parameter, &argument) in substitution {
            let sat = self.generic_argument_satisfies_bound(parameter, argument);
            if sat {
                continue;
            }

            let parameter_name = self
                .symbol_name(parameter)
                .unwrap_or_else(|| "T".to_string());
            let bound = self
                .generic_parameter_bound_type(parameter)
                .unwrap_or_else(|| self.layer.table_mut().error());

            self.report_generic_argument_bound_mismatch(
                target,
                parameter_name.as_str(),
                bound,
                argument,
            );
        }
    }

    fn generic_argument_satisfies_bound(&mut self, parameter: SymbolId, argument: TypeId) -> bool {
        let Some(bound_node) = self.generic_parameter_bound_type_node(parameter) else {
            return true;
        };

        if let Ok(application) = self.constraint_application(bound_node) {
            return self
                .satisfied_constraint_application(argument, application.constraint_name.as_str())
                .is_some();
        }

        let Some(bound) = self.layer.node_type(bound_node) else {
            return false;
        };

        self.type_satisfies_generic_bound(argument, bound)
    }

    pub(super) fn type_satisfies_generic_bound(&mut self, argument: TypeId, bound: TypeId) -> bool {
        let bound = self.resolve_alias_type(bound);
        let resolved_arg = self.resolve_alias_type(argument);

        if let Some(TypeKind::GenericParameter { symbol }) =
            self.layer.table().kind(resolved_arg).cloned()
            && let Some(arg_bound) = self.generic_parameter_bound_type(symbol)
            && self.is_assignable(bound, arg_bound)
        {
            return true;
        }

        match self.layer.table().kind(bound).cloned() {
            Some(TypeKind::Union { members }) => members
                .into_iter()
                .any(|member| self.type_satisfies_generic_bound(argument, member)),
            Some(TypeKind::Named { symbol }) if self.is_constraint_symbol(symbol) => {
                let Some(constraint_name) = self.symbol_name(symbol) else {
                    return false;
                };

                self.satisfied_constraint_application(argument, constraint_name.as_str())
                    .is_some()
            }
            Some(TypeKind::Error) => true,
            _ => self.is_assignable(bound, argument),
        }
    }

    pub(super) fn is_constraint_symbol(&self, symbol: SymbolId) -> bool {
        self.graph
            .resolution()
            .and_then(|resolution| resolution.symbol(symbol))
            .is_some_and(|symbol| symbol.kind() == SymbolKind::Constraint)
    }

    pub(super) fn generic_parameter_bound_type(&self, parameter: SymbolId) -> Option<TypeId> {
        let bound_node = self.generic_parameter_bound_type_node(parameter)?;
        self.layer.node_type(bound_node)
    }

    fn generic_parameter_bound_type_node(&self, parameter: SymbolId) -> Option<NodeId> {
        let root = self.graph.syntax().root()?;
        self.find_generic_parameter_bound_type_node(root, parameter)
    }

    fn find_generic_parameter_bound_type_node(
        &self,
        node: NodeId,
        parameter: SymbolId,
    ) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::GenericParameter
            && self.direct_identifier_symbol(node, SymbolKind::GenericParameter) == Some(parameter)
        {
            let constraint = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::GenericParameterConstraint)?;
            return self.first_constraint_type_child(constraint);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_generic_parameter_bound_type_node(*child, parameter) {
                return Some(found);
            }
        }

        None
    }

    fn first_constraint_type_child(&self, node: NodeId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        for child in syntax_node.children() {
            let Some(child_node) = self.graph.syntax().node(*child) else {
                continue;
            };

            if matches!(
                child_node.kind(),
                SyntaxNodeKind::NamedType
                    | SyntaxNodeKind::Path
                    | SyntaxNodeKind::GenericType
                    | SyntaxNodeKind::ArrayType
                    | SyntaxNodeKind::UnionType
                    | SyntaxNodeKind::TypeNull
            ) {
                return Some(*child);
            }

            if matches!(
                child_node.kind(),
                SyntaxNodeKind::BasicConstraint | SyntaxNodeKind::GenericParameterConstraint
            ) && let Some(found) = self.first_constraint_type_child(*child)
            {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn is_defaultable_or_nullable(&self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);
        match self.layer.table().kind(ty) {
            Some(TypeKind::Primitive(_)) => true,
            Some(TypeKind::Array { .. })
            | Some(TypeKind::Range { .. })
            | Some(TypeKind::Tuple { .. })
            | Some(TypeKind::Function(_)) => true,
            Some(TypeKind::Union { members }) => members.iter().any(|&m| {
                let resolved_m = self.resolve_alias_type(m);
                matches!(
                    self.layer.table().kind(resolved_m),
                    Some(TypeKind::Primitive(PrimitiveType::Null))
                )
            }),
            Some(TypeKind::Named { symbol }) => {
                if let Some(resolution) = self.graph.resolution()
                    && let Some(sym) = resolution.symbol(*symbol)
                {
                    return sym.kind() == SymbolKind::Choice;
                }
                false
            }
            Some(TypeKind::Path { root, .. }) => {
                if let Some(resolution) = self.graph.resolution()
                    && let Some(sym) = resolution.symbol(*root)
                {
                    return sym.kind() == SymbolKind::Choice;
                }
                false
            }
            Some(TypeKind::GenericInstance { base, .. }) => self.is_defaultable_or_nullable(*base),
            Some(TypeKind::GenericParameter { .. }) => true,
            _ => false,
        }
    }
}
