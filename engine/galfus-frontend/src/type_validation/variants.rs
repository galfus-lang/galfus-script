use std::collections;

use super::DeclarationTypeChecker;
use crate::{PathReferenceKind, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct VariantPayload {
    variant_name: String,
    owner_symbol: SymbolId,
    owner_type: TypeId,
    payload_types: Vec<TypeId>,
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_path_variant_expression_type(&mut self, node: NodeId) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;
        let Some(kind) = resolution.path_reference_kind(node) else {
            return self.infer_value_anchor_path_type(node);
        };

        match kind {
            PathReferenceKind::EnumVariant => self.infer_enum_variant_path_type(node),
            PathReferenceKind::ChoiceVariant => self.infer_choice_variant_path_type(node),
            PathReferenceKind::AnchorFunction => self.infer_anchor_function_path_type(node),
            PathReferenceKind::ConstraintMember => self.infer_constraint_member_path_type(node),
            PathReferenceKind::LocalMember => {
                let target = self.graph.syntax().child(node, 0)?;
                let member = self.graph.syntax().child(node, 1)?;
                let target_type = self.infer_expression_type(target);
                let target_type = target_type?;
                let member_name = self.node_text(member);
                self.member_type_for_target_type(target_type, member_name.as_str())
            }
        }
    }

    pub(super) fn infer_choice_variant_call_type(
        &mut self,
        call: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let target = self.graph.syntax().child(call, 0)?;
        let arguments = self.graph.syntax().child(call, 1)?;

        let mut payload = self.choice_variant_payload(target)?;

        if payload.payload_types.is_empty() {
            self.report_choice_payload_not_allowed(target, payload.variant_name.as_str());

            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(call, error);

            return Some(error);
        }

        let argument_nodes = self.call_argument_nodes(arguments);

        self.check_variant_argument_count(call, payload.payload_types.len(), argument_nodes.len());

        self.specialize_choice_variant_payload_from_expected(target, expected, &mut payload);
        self.specialize_choice_variant_payload_from_arguments(
            target,
            argument_nodes.as_slice(),
            &mut payload,
        );

        for (index, argument) in argument_nodes.iter().copied().enumerate() {
            let Some(expected) = payload.payload_types.get(index).copied() else {
                continue;
            };

            let Some(actual) = self.infer_expression_type_with_expected(argument, Some(expected))
            else {
                continue;
            };

            if self.is_assignable(expected, actual) {
                continue;
            }

            self.report_type_mismatch(argument, expected, actual);
        }

        self.layer.bind_node_type(call, payload.owner_type);
        Some(payload.owner_type)
    }

    pub(super) fn is_choice_variant_call_target(&self, target: NodeId) -> bool {
        let Some(resolution) = self.graph.resolution() else {
            return false;
        };

        matches!(
            resolution.path_reference_kind(target),
            Some(PathReferenceKind::ChoiceVariant)
        )
    }

    fn infer_enum_variant_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;
        let variant_symbol = resolution.path_reference_symbol(node)?;
        let enum_symbol = self.owner_symbol_for_member(variant_symbol, SymbolKind::Enum)?;

        let ty = self
            .layer
            .symbol_type(enum_symbol)
            .unwrap_or_else(|| self.layer.table_mut().intern_named(enum_symbol));

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn infer_choice_variant_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let payload = self.choice_variant_payload(node)?;

        if !payload.payload_types.is_empty() {
            self.report_choice_payload_required(node, payload.variant_name.as_str());

            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        }

        self.layer.bind_node_type(node, payload.owner_type);
        Some(payload.owner_type)
    }

    fn infer_anchor_function_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;
        let function_symbol = resolution.path_reference_symbol(node)?;
        let ty = self.layer.symbol_type(function_symbol)?;

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn infer_constraint_member_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;
        let member_symbol = resolution.path_reference_symbol(node)?;
        let ty = self.layer.symbol_type(member_symbol)?;

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn infer_value_anchor_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let target = self.graph.syntax().child(node, 0)?;
        let member = self.graph.syntax().child(node, 1)?;
        let target_type = self.infer_expression_type(target)?;
        let member_name = self.node_text(member);
        let member_type =
            self.constraint_function_type_for_value_anchor(target_type, member_name.as_str())?;

        self.layer.bind_node_type(node, member_type);
        Some(member_type)
    }

    fn constraint_function_type_for_value_anchor(
        &self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<TypeId> {
        let target_type = self.resolve_alias_type(target_type);
        let TypeKind::Named { symbol } = self.layer.table().kind(target_type)? else {
            return None;
        };

        let resolution = self.graph.resolution()?;
        let symbol_data = resolution.symbol(*symbol)?;

        if symbol_data.kind() != SymbolKind::Constraint {
            return None;
        }

        let member_scope = resolution.member_scope(*symbol)?;
        let member_symbol = resolution
            .scope(member_scope)
            .and_then(|scope| scope.symbol(member_name))?;
        let member_symbol_data = resolution.symbol(member_symbol)?;

        if member_symbol_data.kind() != SymbolKind::ConstraintFunction {
            return None;
        }

        self.layer.symbol_type(member_symbol)
    }

    fn choice_variant_payload(&mut self, node: NodeId) -> Option<VariantPayload> {
        let resolution = self.graph.resolution()?;

        if resolution.path_reference_kind(node) != Some(PathReferenceKind::ChoiceVariant) {
            return None;
        }

        let variant_symbol = resolution.path_reference_symbol(node)?;
        let owner_symbol = self.owner_symbol_for_member(variant_symbol, SymbolKind::Choice)?;

        let mut owner_type = self
            .layer
            .symbol_type(owner_symbol)
            .unwrap_or_else(|| self.layer.table_mut().intern_named(owner_symbol));

        let variant_name = resolution.symbol(variant_symbol)?.name().to_string();

        let mut payload_types = self.choice_variant_payload_types(owner_symbol, variant_symbol);

        if let Some(target) = self.graph.syntax().child(node, 0)
            && let Some(target_type) = self.infer_expression_type(target)
        {
            let resolved = self.resolve_alias_type(target_type);
            if let Some(TypeKind::GenericInstance { arguments, .. }) =
                self.layer.table().kind(resolved)
            {
                owner_type = resolved;
                let choice_type = self.layer.symbol_type(owner_symbol).unwrap_or(owner_type);
                let parameters = self.generic_expression_parameter_symbols(target, choice_type);
                let substitution = parameters
                    .into_iter()
                    .zip(arguments.clone())
                    .collect::<collections::HashMap<SymbolId, TypeId>>();
                for payload_type in &mut payload_types {
                    *payload_type =
                        self.substitute_generic_expression_type(*payload_type, &substitution);
                }
            }
        }

        Some(VariantPayload {
            variant_name,
            owner_symbol,
            owner_type,
            payload_types,
        })
    }

    fn specialize_choice_variant_payload_from_expected(
        &mut self,
        target: NodeId,
        expected: Option<TypeId>,
        payload: &mut VariantPayload,
    ) {
        let Some(expected) = expected else {
            return;
        };

        let expected = self.resolve_alias_type(expected);
        let Some(TypeKind::GenericInstance { base, arguments }) =
            self.layer.table().kind(expected).cloned()
        else {
            return;
        };

        let base = self.resolve_alias_type(base);
        let Some(TypeKind::Named { symbol }) = self.layer.table().kind(base) else {
            return;
        };

        if *symbol != payload.owner_symbol {
            return;
        }

        self.apply_choice_variant_generic_arguments(target, arguments, payload);
    }

    fn specialize_choice_variant_payload_from_arguments(
        &mut self,
        target: NodeId,
        argument_nodes: &[NodeId],
        payload: &mut VariantPayload,
    ) {
        let parameters = self.choice_variant_generic_parameters(target, payload.owner_symbol);
        if parameters.is_empty() {
            return;
        }

        let mut substitutions = HashMap::new();

        for (index, argument) in argument_nodes.iter().copied().enumerate() {
            let Some(expected_payload) = payload.payload_types.get(index).copied() else {
                continue;
            };

            let contextual_payload =
                self.substitute_generic_expression_type(expected_payload, &substitutions);
            let Some(actual) =
                self.infer_expression_type_with_expected(argument, Some(contextual_payload))
            else {
                continue;
            };

            self.infer_substitutions_from_types(
                parameters.as_slice(),
                expected_payload,
                actual,
                &mut substitutions,
            );
        }

        if substitutions.is_empty() {
            return;
        }

        let mut arguments = Vec::new();
        for parameter in &parameters {
            let Some(argument) = substitutions.get(parameter).copied() else {
                return;
            };
            arguments.push(argument);
        }

        self.validate_generic_substitution_bounds(target, &substitutions);
        self.apply_choice_variant_generic_arguments(target, arguments, payload);
    }

    fn apply_choice_variant_generic_arguments(
        &mut self,
        target: NodeId,
        arguments: Vec<TypeId>,
        payload: &mut VariantPayload,
    ) {
        let choice_type = self
            .layer
            .symbol_type(payload.owner_symbol)
            .unwrap_or_else(|| self.layer.table_mut().intern_named(payload.owner_symbol));
        let parameters = self.choice_variant_generic_parameters(target, payload.owner_symbol);
        let substitution = parameters
            .into_iter()
            .zip(arguments.iter().copied())
            .collect::<HashMap<SymbolId, TypeId>>();

        payload.owner_type = self
            .layer
            .table_mut()
            .intern_generic_instance(choice_type, arguments);

        for payload_type in &mut payload.payload_types {
            *payload_type = self.substitute_generic_expression_type(*payload_type, &substitution);
        }
    }

    fn choice_variant_generic_parameters(
        &mut self,
        target: NodeId,
        owner_symbol: SymbolId,
    ) -> Vec<SymbolId> {
        let choice_type = self
            .layer
            .symbol_type(owner_symbol)
            .unwrap_or_else(|| self.layer.table_mut().intern_named(owner_symbol));

        if let Some(target) = self.graph.syntax().child(target, 0) {
            let parameters = self.generic_expression_parameter_symbols(target, choice_type);
            if !parameters.is_empty() {
                return parameters;
            }
        }

        self.generic_parameter_symbols_from_type(choice_type)
    }

    pub(super) fn choice_variant_payload_types(
        &self,
        owner_symbol: SymbolId,
        variant_symbol: SymbolId,
    ) -> Vec<TypeId> {
        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let Some(owner_data) = resolution.symbol(owner_symbol) else {
            return Vec::new();
        };

        let Some(variant_data) = resolution.symbol(variant_symbol) else {
            return Vec::new();
        };

        let Some(variant_node) =
            self.choice_variant_node_by_name(owner_data.name(), variant_data.name())
        else {
            return Vec::new();
        };

        let Some(payload) =
            self.find_descendant_of_kind(variant_node, SyntaxNodeKind::ChoicePayload)
        else {
            return Vec::new();
        };

        let Some(payload_node) = self.graph.syntax().node(payload) else {
            return Vec::new();
        };

        payload_node
            .children()
            .iter()
            .filter_map(|child| {
                let type_node = self.first_type_child(*child).unwrap_or(*child);
                self.layer.node_type(type_node)
            })
            .collect()
    }

    fn owner_symbol_for_member(
        &self,
        member_symbol: SymbolId,
        owner_kind: SymbolKind,
    ) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;

        for symbol in resolution.symbols() {
            if symbol.kind() != owner_kind {
                continue;
            }

            let Some(member_scope) = resolution.member_scope(symbol.id()) else {
                continue;
            };

            let Some(scope) = resolution.scope(member_scope) else {
                continue;
            };

            if scope
                .symbol(symbol.name())
                .is_some_and(|candidate| candidate == member_symbol)
            {
                return Some(symbol.id());
            }

            if scope
                .symbols()
                .iter()
                .any(|(_, candidate)| *candidate == member_symbol)
            {
                return Some(symbol.id());
            }
        }

        None
    }

    fn check_variant_argument_count(&mut self, call: NodeId, expected: usize, actual: usize) {
        if expected == actual {
            return;
        }

        self.report_argument_count_mismatch(call, expected, actual);
    }

    fn find_descendant_of_kind(&self, node: NodeId, kind: SyntaxNodeKind) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        for child in syntax_node.children() {
            let child_node = self.graph.syntax().node(*child)?;

            if child_node.kind() == kind {
                return Some(*child);
            }

            if let Some(found) = self.find_descendant_of_kind(*child, kind) {
                return Some(found);
            }
        }

        None
    }

    fn choice_variant_node_by_name(&self, choice_name: &str, variant_name: &str) -> Option<NodeId> {
        let root = self.graph.syntax().root()?;
        let choice_item = self.choice_item_node_by_name(root, choice_name)?;

        self.find_choice_variant_node_by_name(choice_item, variant_name)
    }

    fn choice_item_node_by_name(&self, node: NodeId, choice_name: &str) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::ChoiceItem {
            let identifier = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)?;

            if self.node_text(identifier) == choice_name {
                return Some(node);
            }
        }

        for child in syntax_node.children() {
            if let Some(found) = self.choice_item_node_by_name(*child, choice_name) {
                return Some(found);
            }
        }

        None
    }

    fn find_choice_variant_node_by_name(&self, node: NodeId, variant_name: &str) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::ChoiceVariant {
            let identifier = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)?;

            if self.node_text(identifier) == variant_name {
                return Some(node);
            }
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_choice_variant_node_by_name(*child, variant_name) {
                return Some(found);
            }
        }

        None
    }
}
