use super::DeclarationTypeChecker;
use crate::{FunctionParameterType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::{HashMap, HashSet};

type GenericSubstitution = HashMap<SymbolId, TypeId>;

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_generic_expression_type(&mut self, node: NodeId) -> Option<TypeId> {
        let target = self.graph.syntax().child(node, 0)?;
        let arguments = self.graph.syntax().child(node, 1)?;

        let target_type = self.infer_expression_type(target);
        let target_type = target_type?;
        let argument_types = self.generic_expression_argument_types(arguments)?;
        let parameters = self.generic_expression_parameter_symbols(target, target_type);

        if parameters.len() != argument_types.len() {
            self.report_generic_argument_count_mismatch(
                node,
                parameters.len(),
                argument_types.len(),
            );

            return Some(self.layer.table_mut().error());
        }

        let substitution = parameters
            .into_iter()
            .zip(argument_types.clone())
            .collect::<GenericSubstitution>();

        self.validate_generic_substitution_bounds(node, &substitution);

        let resolved_target = self.resolve_alias_type(target_type);
        let result_type = match self.layer.table().kind(resolved_target) {
            Some(TypeKind::Named { .. }) => self
                .layer
                .table_mut()
                .intern_generic_instance(target_type, argument_types),
            _ => self.substitute_generic_expression_type(target_type, &substitution),
        };

        self.layer.bind_node_type(node, result_type);
        Some(result_type)
    }

    fn generic_expression_argument_types(&self, arguments: NodeId) -> Option<Vec<TypeId>> {
        let argument_nodes = self.graph.syntax().node(arguments)?.children().to_vec();

        let mut argument_types = Vec::new();

        for argument in argument_nodes {
            if let Some(ty) = self.layer.node_type(argument) {
                argument_types.push(ty);
                continue;
            }

            let type_node = self.first_type_child(argument)?;
            let ty = self.layer.node_type(type_node)?;
            argument_types.push(ty);
        }

        Some(argument_types)
    }

    pub(super) fn generic_expression_parameter_symbols(
        &self,
        target: NodeId,
        target_type: TypeId,
    ) -> Vec<SymbolId> {
        if let Some(symbol) = self.expression_reference_symbol(target) {
            let mut decl_item = self.function_item_for_symbol(symbol);
            if decl_item.is_none() {
                decl_item = self.choice_or_struct_item_for_symbol(symbol);
            }
            if let Some(item) = decl_item {
                let parameters =
                    self.declaration_symbols_in_node(item, &[SymbolKind::GenericParameter]);

                if !parameters.is_empty() {
                    return parameters;
                }
            }
        }

        self.generic_parameter_symbols_from_type(target_type)
    }

    fn expression_reference_symbol(&self, node: NodeId) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;

        if let Some(symbol) = resolution.reference_symbol(node) {
            return Some(symbol);
        }

        let syntax_node = self.graph.syntax().node(node)?;

        for child in syntax_node.children() {
            if let Some(symbol) = self.expression_reference_symbol(*child) {
                return Some(symbol);
            }
        }

        None
    }

    fn function_item_for_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
        let root = self.graph.syntax().root()?;
        self.find_function_item_for_symbol(root, symbol)
    }

    fn find_function_item_for_symbol(&self, node: NodeId, symbol: SymbolId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem
            && self.direct_identifier_symbol(node, SymbolKind::Function) == Some(symbol)
        {
            return Some(node);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_function_item_for_symbol(*child, symbol) {
                return Some(found);
            }
        }

        None
    }

    fn choice_or_struct_item_for_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
        let root = self.graph.syntax().root()?;
        self.find_choice_or_struct_item_for_symbol(root, symbol)
    }

    fn find_choice_or_struct_item_for_symbol(
        &self,
        node: NodeId,
        symbol: SymbolId,
    ) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if (syntax_node.kind() == SyntaxNodeKind::ChoiceItem
            && self.direct_identifier_symbol(node, SymbolKind::Choice) == Some(symbol))
            || (syntax_node.kind() == SyntaxNodeKind::StructItem
                && self.direct_identifier_symbol(node, SymbolKind::Struct) == Some(symbol))
        {
            return Some(node);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_choice_or_struct_item_for_symbol(*child, symbol) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn generic_parameter_symbols_from_type(&self, ty: TypeId) -> Vec<SymbolId> {
        let mut symbols = Vec::new();
        let mut seen = HashSet::new();

        self.collect_generic_parameter_symbols_from_type(ty, &mut symbols, &mut seen);

        symbols
    }

    fn collect_generic_parameter_symbols_from_type(
        &self,
        ty: TypeId,
        symbols: &mut Vec<SymbolId>,
        seen: &mut HashSet<SymbolId>,
    ) {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty).cloned() {
            Some(TypeKind::GenericParameter { symbol }) => {
                if seen.insert(symbol) {
                    symbols.push(symbol);
                }
            }
            Some(TypeKind::Array { element }) => {
                self.collect_generic_parameter_symbols_from_type(element, symbols, seen);
            }
            Some(TypeKind::Range { element }) => {
                self.collect_generic_parameter_symbols_from_type(element, symbols, seen);
            }
            Some(TypeKind::Tuple { elements }) => {
                for element in elements {
                    self.collect_generic_parameter_symbols_from_type(element, symbols, seen);
                }
            }
            Some(TypeKind::Union { members }) => {
                for member in members {
                    self.collect_generic_parameter_symbols_from_type(member, symbols, seen);
                }
            }
            Some(TypeKind::Function(function)) => {
                for parameter in function.parameters() {
                    self.collect_generic_parameter_symbols_from_type(parameter.ty(), symbols, seen);
                }

                self.collect_generic_parameter_symbols_from_type(
                    function.return_type(),
                    symbols,
                    seen,
                );
            }
            Some(TypeKind::GenericInstance { base, arguments }) => {
                self.collect_generic_parameter_symbols_from_type(base, symbols, seen);

                for argument in arguments {
                    self.collect_generic_parameter_symbols_from_type(argument, symbols, seen);
                }
            }
            _ => {}
        }
    }

    pub(super) fn substitute_generic_expression_type(
        &mut self,
        ty: TypeId,
        substitution: &GenericSubstitution,
    ) -> TypeId {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty).cloned() {
            Some(TypeKind::GenericParameter { symbol }) => {
                substitution.get(&symbol).copied().unwrap_or(ty)
            }
            Some(TypeKind::Array { element }) => {
                let element = self.substitute_generic_expression_type(element, substitution);
                self.layer.table_mut().intern_array(element)
            }
            Some(TypeKind::Range { element }) => {
                let element = self.substitute_generic_expression_type(element, substitution);
                self.layer.table_mut().intern_range(element)
            }
            Some(TypeKind::Tuple { elements }) => {
                let elements = elements
                    .into_iter()
                    .map(|element| self.substitute_generic_expression_type(element, substitution))
                    .collect::<Vec<_>>();

                self.layer.table_mut().intern_tuple(elements)
            }
            Some(TypeKind::Union { members }) => {
                let members = members
                    .into_iter()
                    .map(|member| self.substitute_generic_expression_type(member, substitution))
                    .collect::<Vec<_>>();

                self.layer.table_mut().intern_union(members)
            }
            Some(TypeKind::Function(function)) => {
                let parameters = function
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        let ty =
                            self.substitute_generic_expression_type(parameter.ty(), substitution);

                        if parameter.is_rest() {
                            return FunctionParameterType::rest(ty);
                        }

                        if parameter.has_default() {
                            return FunctionParameterType::with_default(ty);
                        }

                        FunctionParameterType::new(ty)
                    })
                    .collect::<Vec<_>>();

                let return_type =
                    self.substitute_generic_expression_type(function.return_type(), substitution);

                self.layer
                    .table_mut()
                    .intern_function(parameters, return_type)
            }
            Some(TypeKind::GenericInstance { base, arguments }) => {
                let base = self.substitute_generic_expression_type(base, substitution);
                let arguments = arguments
                    .into_iter()
                    .map(|argument| self.substitute_generic_expression_type(argument, substitution))
                    .collect::<Vec<_>>();

                self.layer
                    .table_mut()
                    .intern_generic_instance(base, arguments)
            }
            Some(TypeKind::Error) => ty,
            _ => ty,
        }
    }
}
