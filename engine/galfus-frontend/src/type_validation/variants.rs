use std::collections;

use super::constraints::ConstraintApplication;
use super::{DeclarationTypeChecker, LoweredImportedChoice, LoweredImportedConstraint};
use crate::{
    FunctionType, ImportedMemberKey, PathReferenceKind, SymbolKind, SyntaxNodeKind, TypeKind,
};
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
    pub(super) fn infer_path_variant_expression_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;
        let Some(kind) = resolution.path_reference_kind(node) else {
            return self.infer_value_anchor_path_type(node);
        };

        match kind {
            PathReferenceKind::EnumVariant => self.infer_enum_variant_path_type(node),
            PathReferenceKind::ChoiceVariant => self.infer_choice_variant_path_type(node, expected),
            PathReferenceKind::AnchorFunction => self.infer_bound_anchor_function_path_type(node),
            PathReferenceKind::ConstraintMember => {
                self.infer_bound_constraint_member_path_type(node)
            }
            PathReferenceKind::LocalMember => {
                let target = self.graph.syntax().child(node, 0)?;
                let member = self.graph.syntax().child(node, 1)?;
                let target_type = self.infer_expression_type(target);
                let target_type = target_type?;
                let member_name = self.node_text(member);
                if self.is_imported_payloadless_choice_variant(target_type, member_name.as_str()) {
                    return Some(target_type);
                }
                let ty = self.member_type_for_target_type(target_type, member_name.as_str());
                if let Some(ty) = ty
                    && let Some(TypeKind::Named { symbol }) = self.layer.table().kind(ty)
                {
                    let resolution = self.graph.resolution()?;
                    if let Some(symbol_data) = resolution.symbol(*symbol)
                        && symbol_data.kind() == SymbolKind::Struct
                        && self.is_opaque_struct_handle(*symbol)
                    {
                        self.report_opaque_handle_not_exportable_as_value(node);
                        let error = self.layer.table_mut().error();
                        self.layer.bind_node_type(node, error);
                        return Some(error);
                    }
                }
                ty
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

        // Multi-value choice payloads are represented by a tuple at runtime.
        // Materialize that tuple type while the type table is still mutable so
        // MIR and bytecode lowering can reference the same payload layout.
        if payload.payload_types.len() > 1 {
            self.layer
                .table_mut()
                .intern_tuple(payload.payload_types.clone());
        }

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
            self.choice_variant_path_node(target)
                .and_then(|target| resolution.path_reference_kind(target)),
            Some(PathReferenceKind::ChoiceVariant)
        ) || self.imported_choice_variant_path(target).is_some()
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

    fn infer_choice_variant_path_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let mut payload = self.choice_variant_payload(node)?;

        if !payload.payload_types.is_empty() {
            self.report_choice_payload_required(node, payload.variant_name.as_str());

            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        }

        self.specialize_choice_variant_payload_from_expected(node, expected, &mut payload);

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

    fn infer_bound_anchor_function_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let ty = self.infer_anchor_function_path_type(node)?;
        self.bind_value_anchor_receiver(node, ty)
    }

    fn infer_constraint_member_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;
        let member_symbol = resolution.path_reference_symbol(node)?;
        let ty = self.layer.symbol_type(member_symbol)?;

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn infer_bound_constraint_member_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let ty = self.infer_constraint_member_path_type(node)?;
        self.bind_value_anchor_receiver(node, ty)
    }

    fn bind_value_anchor_receiver(&mut self, node: NodeId, member_type: TypeId) -> Option<TypeId> {
        let target = self.graph.syntax().child(node, 0)?;
        let target_type = self.infer_expression_type(target)?;
        let is_static_target = self
            .graph
            .resolution()
            .and_then(|resolution| resolution.reference_symbol(target))
            .and_then(|symbol| self.graph.resolution()?.symbol(symbol))
            .is_some_and(|symbol| {
                matches!(
                    symbol.kind(),
                    SymbolKind::Struct | SymbolKind::ImportNamespace
                )
            })
            || self
                .graph
                .resolution()
                .and_then(|resolution| resolution.reference_symbol(target))
                .is_some_and(|symbol| self.imported_struct_fields.contains_key(&symbol))
            || (self.graph.syntax().node(target).is_some_and(|node| {
                matches!(
                    node.kind(),
                    SyntaxNodeKind::PathExpression | SyntaxNodeKind::GenericExpression
                )
            }) && matches!(
                self.layer
                    .table()
                    .kind(self.resolve_alias_type(target_type)),
                Some(TypeKind::Path { .. })
            ));

        if is_static_target {
            return Some(member_type);
        }

        let TypeKind::Function(function) = self
            .layer
            .table()
            .kind(self.resolve_alias_type(member_type))?
            .clone()
        else {
            return Some(member_type);
        };

        let (_, parameters) = function.parameters().split_first()?;
        let bound_type = self.bound_function_type(&function, parameters.to_vec());
        self.layer.bind_node_type(node, bound_type);
        Some(bound_type)
    }

    fn bound_function_type(
        &mut self,
        function: &FunctionType,
        parameters: Vec<crate::FunctionParameterType>,
    ) -> TypeId {
        self.layer.table_mut().intern_function(
            parameters,
            function.return_type(),
            function.is_external(),
        )
    }

    fn infer_value_anchor_path_type(&mut self, node: NodeId) -> Option<TypeId> {
        let target = self.graph.syntax().child(node, 0)?;
        let member = self.graph.syntax().child(node, 1)?;
        let target_type = self.infer_expression_type(target)?;
        let member_name = self.node_text(member);
        if self.is_imported_payloadless_choice_variant(target_type, member_name.as_str()) {
            self.layer.bind_node_type(node, target_type);
            return Some(target_type);
        }
        let mut member_type =
            self.constraint_function_type_for_value_anchor(target_type, member_name.as_str());
        if member_type.is_none() {
            member_type =
                self.struct_function_type_for_value_anchor(target_type, member_name.as_str());
        }
        if member_type.is_none() {
            member_type =
                self.imported_function_type_for_value_anchor(target_type, member_name.as_str());
        }
        let Some(member_type) = member_type else {
            let error = self.layer.table_mut().error();
            if !matches!(self.layer.table().kind(target_type), Some(TypeKind::Error)) {
                self.report_unknown_member(member, member_name.as_str(), target_type);
            }
            self.layer.bind_node_type(node, error);
            return Some(error);
        };
        let member_type = self.bind_value_anchor_receiver(node, member_type);
        let member_type = member_type?;

        self.layer.bind_node_type(node, member_type);
        Some(member_type)
    }

    fn imported_function_type_for_value_anchor(
        &self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<TypeId> {
        let target_type = self.resolve_alias_type(target_type);
        let key = match self.layer.table().kind(target_type)? {
            TypeKind::Named { symbol } => ImportedMemberKey::new(*symbol, "", member_name),
            TypeKind::GenericInstance { base, .. } => {
                let TypeKind::Named { symbol } = self.layer.table().kind(*base)? else {
                    return None;
                };
                ImportedMemberKey::new(*symbol, "", member_name)
            }
            TypeKind::Path { root, segments } => {
                ImportedMemberKey::new(*root, segments.join("::"), member_name)
            }
            _ => return None,
        };

        self.imported_member_types.get(&key).copied()
    }

    fn is_imported_payloadless_choice_variant(
        &self,
        target_type: TypeId,
        member_name: &str,
    ) -> bool {
        let target_type = self.resolve_alias_type(target_type);
        let choice = match self.layer.table().kind(target_type) {
            Some(TypeKind::Named { symbol }) => self.imported_symbol_choices.get(symbol),
            Some(TypeKind::GenericInstance { base, .. }) => {
                let Some(TypeKind::Named { symbol }) = self.layer.table().kind(*base) else {
                    return false;
                };
                self.imported_symbol_choices.get(symbol)
            }
            Some(TypeKind::Path { root, segments }) => self
                .imported_namespace_choices
                .get(&(*root, segments.join("::"))),
            _ => None,
        };

        choice
            .and_then(|choice| {
                choice
                    .variants
                    .iter()
                    .find(|variant| variant.name == member_name)
            })
            .is_some_and(|variant| variant.payload_types.is_empty())
    }

    fn struct_function_type_for_value_anchor(
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

        let mut current_symbol = *symbol;
        let mut current_symbol_data = symbol_data;

        while current_symbol_data.kind() == SymbolKind::ImportBinding
            || current_symbol_data.kind() == SymbolKind::TypeAlias
        {
            if let Some(aliased_type) = self.layer.symbol_type(current_symbol)
                && let Some(TypeKind::Named {
                    symbol: next_symbol,
                }) = self.layer.table().kind(aliased_type)
            {
                if *next_symbol == current_symbol {
                    break;
                }
                if let Some(next_symbol_data) = resolution.symbol(*next_symbol) {
                    current_symbol = *next_symbol;
                    current_symbol_data = next_symbol_data;
                    continue;
                }
            }
            break;
        }

        if current_symbol_data.kind() != SymbolKind::Struct {
            return None;
        }

        let anchored_name = format!(
            "{}::{member_name}",
            self.string_table
                .resolve(current_symbol_data.name())
                .unwrap_or("")
        );
        let anchored_name_id = self.string_table.get(&anchored_name)?;
        let module_scope = current_symbol_data.scope();
        let scope = resolution.scope(module_scope)?;
        let member_symbol = scope.symbol(anchored_name_id)?;

        let member_symbol_data = resolution.symbol(member_symbol)?;
        if member_symbol_data.kind() != SymbolKind::Function {
            return None;
        }

        self.layer.symbol_type(member_symbol)
    }

    fn constraint_function_type_for_value_anchor(
        &mut self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<TypeId> {
        let target_type = self.resolve_alias_type(target_type);
        let resolution = self.graph.resolution()?;

        let constraint_function = |constraint_symbol| {
            let member_scope = resolution.member_scope(constraint_symbol)?;
            let member_symbol = resolution.scope(member_scope).and_then(|scope| {
                self.string_table
                    .get(member_name)
                    .and_then(|id| scope.symbol(id))
            })?;
            (resolution.symbol(member_symbol)?.kind() == SymbolKind::ConstraintFunction)
                .then_some(member_symbol)
        };

        let direct_constraint = match self.layer.table().kind(target_type).cloned() {
            Some(TypeKind::Named { symbol }) => Some((symbol, target_type, Vec::new())),
            Some(TypeKind::GenericInstance { base, arguments }) => {
                match self.layer.table().kind(base).cloned() {
                    Some(TypeKind::Named { symbol }) => Some((symbol, base, arguments)),
                    _ => None,
                }
            }
            _ => None,
        };

        if let Some(TypeKind::GenericParameter { symbol }) = self.layer.table().kind(target_type)
            && let Some(application) = self.generic_parameter_constraint_application(*symbol)
        {
            return self.constraint_application_function_type(application, member_name);
        }

        if let Some((constraint_symbol, _, arguments)) = direct_constraint.as_ref()
            && resolution
                .symbol(*constraint_symbol)
                .is_some_and(|symbol| symbol.kind() == SymbolKind::Constraint)
        {
            let member_symbol = constraint_function(*constraint_symbol)?;
            let member_type = self.layer.symbol_type(member_symbol)?;
            if arguments.is_empty() {
                return Some(member_type);
            }

            let substitution = self
                .constraint_generic_parameters(*constraint_symbol)
                .into_iter()
                .zip(arguments.iter().copied())
                .collect();
            return Some(self.substitute_type(member_type, &substitution));
        }

        if let Some((constraint_symbol, base_type, arguments)) = direct_constraint.as_ref()
            && let Some((generic_parameters, member_type)) = self
                .imported_constraint_for_base_type(*constraint_symbol, *base_type)
                .and_then(|constraint| {
                    constraint
                        .functions
                        .iter()
                        .find(|function| function.name == member_name)
                        .map(|function| (constraint.generic_parameters.clone(), function.ty))
                })
        {
            let substitution = generic_parameters
                .into_iter()
                .zip(arguments.iter().copied())
                .collect();
            return Some(self.substitute_type(member_type, &substitution));
        }

        for application in self.imported_struct_constraint_applications(target_type) {
            if let Some(member_type) =
                self.constraint_application_function_type(application, member_name)
            {
                return Some(member_type);
            }
        }

        let application = self
            .constraint_applications_for_type(target_type)
            .into_iter()
            .find(|application| constraint_function(application.symbol).is_some())?;
        let member_symbol = constraint_function(application.symbol)?;
        let member_type = self.layer.symbol_type(member_symbol)?;
        Some(self.substitute_type(member_type, &application.substitution))
    }

    fn constraint_application_function_type(
        &mut self,
        application: ConstraintApplication,
        member_name: &str,
    ) -> Option<TypeId> {
        if let Some(constraint) = application.imported_constraint {
            let member_type = constraint
                .functions
                .iter()
                .find(|function| function.name == member_name)
                .map(|function| function.ty)?;
            return Some(self.substitute_type(member_type, &application.substitution));
        }

        let resolution = self.graph.resolution()?;
        let member_scope = resolution.member_scope(application.symbol)?;
        let member_symbol = resolution.scope(member_scope).and_then(|scope| {
            self.string_table
                .get(member_name)
                .and_then(|id| scope.symbol(id))
        })?;
        if resolution.symbol(member_symbol)?.kind() != SymbolKind::ConstraintFunction {
            return None;
        }
        let member_type = self.layer.symbol_type(member_symbol)?;
        Some(self.substitute_type(member_type, &application.substitution))
    }

    fn imported_struct_constraint_applications(
        &self,
        target_type: TypeId,
    ) -> Vec<ConstraintApplication> {
        let target_type = self.resolve_alias_type(target_type);
        let symbol = match self.layer.table().kind(target_type) {
            Some(TypeKind::Named { symbol }) => *symbol,
            Some(TypeKind::GenericInstance { base, .. }) => {
                let Some(TypeKind::Named { symbol }) = self.layer.table().kind(*base) else {
                    return Vec::new();
                };
                *symbol
            }
            _ => return Vec::new(),
        };
        let constraints = self
            .imported_struct_constraints
            .get(&symbol)
            .cloned()
            .unwrap_or_default();

        constraints
            .into_iter()
            .filter_map(|constraint_type| {
                self.imported_constraint_application_for_type(constraint_type)
            })
            .collect()
    }

    fn imported_constraint_application_for_type(
        &self,
        constraint_type: TypeId,
    ) -> Option<ConstraintApplication> {
        let constraint_type = self.resolve_alias_type(constraint_type);
        let (symbol, base_type, arguments) = match self.layer.table().kind(constraint_type)? {
            TypeKind::Named { symbol } => (*symbol, constraint_type, Vec::new()),
            TypeKind::GenericInstance { base, arguments } => {
                let TypeKind::Named { symbol } = self.layer.table().kind(*base)? else {
                    return None;
                };
                (*symbol, *base, arguments.clone())
            }
            _ => return None,
        };
        let constraint = self
            .imported_constraint_for_base_type(symbol, base_type)?
            .clone();
        let substitution = constraint
            .generic_parameters
            .iter()
            .copied()
            .zip(arguments)
            .collect();

        Some(ConstraintApplication {
            symbol,
            constraint_name: constraint.name.clone(),
            substitution,
            imported_constraint: Some(constraint),
        })
    }

    fn imported_constraint_for_base_type(
        &self,
        symbol: SymbolId,
        base_type: TypeId,
    ) -> Option<&LoweredImportedConstraint> {
        self.imported_symbol_constraints.get(&symbol).or_else(|| {
            let base_type = self.resolve_alias_type(base_type);

            self.imported_symbol_constraints
                .iter()
                .find_map(|(import_symbol, constraint)| {
                    let imported_type = self.layer.symbol_type(*import_symbol)?;
                    (self.resolve_alias_type(imported_type) == base_type).then_some(constraint)
                })
        })
    }

    fn choice_variant_payload(&mut self, node: NodeId) -> Option<VariantPayload> {
        let resolution = self.graph.resolution()?;

        let path_node = self.choice_variant_path_node(node)?;

        if let Some((owner_symbol, choice, variant_name)) = self.imported_choice_variant_path(node)
        {
            let variant = choice
                .variants
                .iter()
                .find(|variant| variant.name == variant_name)?;
            let owner_type = if self.imported_symbol_choices.contains_key(&owner_symbol) {
                self.layer
                    .symbol_type(owner_symbol)
                    .unwrap_or_else(|| self.layer.table_mut().intern_named(owner_symbol))
            } else {
                self.layer
                    .table_mut()
                    .intern_path(owner_symbol, vec![choice.name.clone()])
            };
            let mut payload = VariantPayload {
                variant_name,
                owner_symbol,
                owner_type,
                payload_types: variant.payload_types.clone(),
            };

            if let Some(arguments) = self.explicit_choice_variant_generic_arguments(node) {
                self.apply_choice_variant_generic_arguments(path_node, arguments, &mut payload);
            }

            return Some(payload);
        }

        if resolution.path_reference_kind(path_node) != Some(PathReferenceKind::ChoiceVariant) {
            return None;
        }

        let variant_symbol = resolution.path_reference_symbol(path_node)?;
        let owner_symbol = self.owner_symbol_for_member(variant_symbol, SymbolKind::Choice)?;

        let mut owner_type = self
            .layer
            .symbol_type(owner_symbol)
            .unwrap_or_else(|| self.layer.table_mut().intern_named(owner_symbol));

        let variant_name = self
            .string_table
            .resolve(resolution.symbol(variant_symbol)?.name())
            .unwrap_or("")
            .to_string();

        let mut payload_types = self.choice_variant_payload_types(owner_symbol, variant_symbol);

        if let Some(target) = self.graph.syntax().child(path_node, 0)
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

        let mut payload = VariantPayload {
            variant_name,
            owner_symbol,
            owner_type,
            payload_types,
        };

        if let Some(arguments) = self.explicit_choice_variant_generic_arguments(node) {
            self.apply_choice_variant_generic_arguments(path_node, arguments, &mut payload);
        }

        Some(payload)
    }

    fn choice_variant_path_node(&self, node: NodeId) -> Option<NodeId> {
        match self.graph.syntax().node(node)?.kind() {
            SyntaxNodeKind::GenericExpression => self
                .graph
                .syntax()
                .child(node, 0)
                .and_then(|target| self.choice_variant_path_node(target)),
            SyntaxNodeKind::PathExpression => Some(node),
            _ => None,
        }
    }

    fn imported_choice_variant_path(
        &self,
        node: NodeId,
    ) -> Option<(SymbolId, LoweredImportedChoice, String)> {
        let path_node = self.choice_variant_path_node(node)?;
        let resolution = self.graph.resolution()?;
        let owner = self.graph.syntax().child(path_node, 0)?;
        let owner_symbol = resolution.reference_symbol(owner)?;
        let choice = self
            .imported_path_choices
            .get(&owner)
            .cloned()
            .or_else(|| self.imported_namespace_choice_for_path(owner_symbol, owner))
            .or_else(|| self.imported_symbol_choices.get(&owner_symbol).cloned())?;
        let variant_name = self.node_text(self.graph.syntax().child(path_node, 1)?);

        choice
            .variants
            .iter()
            .any(|variant| variant.name == variant_name)
            .then_some((owner_symbol, choice, variant_name))
    }

    fn imported_namespace_choice_for_path(
        &self,
        namespace: SymbolId,
        node: NodeId,
    ) -> Option<LoweredImportedChoice> {
        let segments = self.path_segments(node);
        (segments.len() > 1)
            .then(|| segments[1..].join("::"))
            .and_then(|name| {
                self.imported_namespace_choices
                    .get(&(namespace, name))
                    .cloned()
            })
    }

    fn path_segments(&self, node: NodeId) -> Vec<String> {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return Vec::new();
        };

        match syntax_node.kind() {
            SyntaxNodeKind::NameExpression => self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .map(|identifier| vec![self.node_text(identifier)])
                .unwrap_or_default(),
            SyntaxNodeKind::PathExpression => {
                let Some(target) = self.graph.syntax().child(node, 0) else {
                    return Vec::new();
                };
                let Some(member) = self.graph.syntax().child(node, 1) else {
                    return Vec::new();
                };
                let mut segments = self.path_segments(target);
                segments.push(self.node_text(member));
                segments
            }
            SyntaxNodeKind::GenericExpression => self
                .graph
                .syntax()
                .child(node, 0)
                .map(|target| self.path_segments(target))
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn explicit_choice_variant_generic_arguments(&self, node: NodeId) -> Option<Vec<TypeId>> {
        if self.graph.syntax().node(node)?.kind() != SyntaxNodeKind::GenericExpression {
            return None;
        }

        let arguments = self.graph.syntax().child(node, 1)?;
        self.generic_expression_argument_types(arguments)
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

        payload.owner_type = expected;
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
            let expected = if self
                .generic_parameter_symbols_from_type(contextual_payload)
                .is_empty()
            {
                Some(contextual_payload)
            } else {
                None
            };
            let Some(actual) = self.infer_expression_type_with_expected(argument, expected) else {
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
        if let Some(choice) = self.imported_symbol_choices.get(&owner_symbol) {
            let variant_name = self
                .graph
                .resolution()
                .and_then(|resolution| resolution.symbol(variant_symbol))
                .and_then(|variant| self.string_table.resolve(variant.name()));
            if let Some(variant_name) = variant_name
                && let Some(variant) = choice
                    .variants
                    .iter()
                    .find(|variant| variant.name == variant_name)
            {
                return variant.payload_types.clone();
            }
        }

        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let Some(owner_data) = resolution.symbol(owner_symbol) else {
            return Vec::new();
        };

        let Some(variant_data) = resolution.symbol(variant_symbol) else {
            return Vec::new();
        };

        let Some(variant_node) = self.choice_variant_node_by_name(
            self.string_table.resolve(owner_data.name()).unwrap_or(""),
            self.string_table.resolve(variant_data.name()).unwrap_or(""),
        ) else {
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
