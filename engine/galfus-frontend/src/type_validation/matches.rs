use std::collections;

use super::{DeclarationTypeChecker, LoweredImportedChoice, LoweredImportedChoiceVariant};
use crate::{PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::{HashMap, HashSet};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_match_expression_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let subject = self.graph.syntax().child(node, 0)?;
        let arms = self.graph.syntax().child(node, 1)?;

        let subject_type = self.infer_expression_type(subject)?;

        let arm_nodes = self
            .graph
            .syntax()
            .node(arms)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        if arm_nodes.is_empty() {
            self.report_cannot_infer_type(node, "cannot infer type of empty match expression");

            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        }

        self.check_match_arm_order(arm_nodes.as_slice());
        self.check_choice_match_exhaustiveness(node, subject_type, arm_nodes.as_slice());

        let mut arm_types = Vec::new();

        for arm in arm_nodes.iter().copied() {
            let Some(arm_type) = self.check_match_arm_type(arm, subject_type, expected) else {
                continue;
            };

            arm_types.push((arm, arm_type));
        }

        let Some(expected) = expected.or_else(|| {
            arm_types
                .iter()
                .copied()
                .find(|(_, ty)| !self.is_error_type(*ty))
                .map(|(_, ty)| ty)
        }) else {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        };

        if arm_types.iter().all(|(_, ty)| !self.is_error_type(*ty)) {
            arm_types.clear();

            for arm in arm_nodes {
                let Some(arm_type) = self.check_match_arm_type(arm, subject_type, Some(expected))
                else {
                    continue;
                };

                arm_types.push((arm, arm_type));
            }
        }

        let mut has_error = false;

        for (arm, actual) in arm_types.iter().copied() {
            if self.is_error_type(actual) {
                has_error = true;
                continue;
            }

            if self.is_assignable(expected, actual) {
                continue;
            }

            let body = self
                .graph
                .syntax()
                .child(arm, 1)
                .and_then(|body| self.graph.syntax().child(body, 0))
                .unwrap_or(arm);
            self.report_incompatible_match_arm_type(body, expected, actual);
            has_error = true;
        }

        let ty = if has_error {
            self.layer.table_mut().error()
        } else {
            expected
        };

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn check_match_arm_order(&mut self, arms: &[NodeId]) {
        let mut catch_all_seen = false;

        for (index, arm) in arms.iter().copied().enumerate() {
            let Some(pattern) = self.graph.syntax().child(arm, 0) else {
                continue;
            };

            if catch_all_seen {
                self.report_unreachable_pattern(pattern);
                continue;
            }

            if !self.is_catch_all_match_pattern(pattern) {
                continue;
            }

            catch_all_seen = true;

            if index + 1 < arms.len() {
                self.report_catch_all_pattern_not_final(pattern);
            }
        }
    }

    fn is_catch_all_match_pattern(&self, pattern: NodeId) -> bool {
        self.graph.syntax().node(pattern).is_some_and(|node| {
            matches!(
                node.kind(),
                SyntaxNodeKind::WildcardPattern | SyntaxNodeKind::BindingPattern
            )
        })
    }

    fn check_match_arm_type(
        &mut self,
        arm: NodeId,
        subject_type: TypeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let pattern = self.graph.syntax().child(arm, 0)?;
        let body = self
            .graph
            .syntax()
            .child(arm, 1)
            .and_then(|body| self.graph.syntax().child(body, 0))?;

        self.check_match_pattern_type(pattern, subject_type);

        self.infer_match_arm_body_type(body, expected)
    }

    fn infer_match_arm_body_type(
        &mut self,
        body: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let body_node = self.graph.syntax().node(body)?;

        if body_node.kind() == SyntaxNodeKind::Block {
            return Some(
                expected.unwrap_or_else(|| self.layer.table().primitive(PrimitiveType::Null)),
            );
        }

        self.infer_expression_type_with_expected(body, expected)
    }

    fn check_match_pattern_type(&mut self, pattern: NodeId, expected: TypeId) {
        let Some(pattern_node) = self.graph.syntax().node(pattern) else {
            return;
        };

        match pattern_node.kind() {
            SyntaxNodeKind::LiteralPattern => {
                self.check_literal_match_pattern_type(pattern, expected);
            }

            SyntaxNodeKind::BindingPattern => {
                self.bind_match_binding_pattern_type(pattern, expected);
            }

            SyntaxNodeKind::VariantPattern => {
                self.layer.bind_node_type(pattern, expected);
                self.check_variant_match_pattern_type(pattern, expected);
            }

            SyntaxNodeKind::WildcardPattern => {
                self.layer.bind_node_type(pattern, expected);
            }

            SyntaxNodeKind::StructPattern => {
                self.check_struct_match_pattern_type(pattern, expected);
            }

            _ => {}
        }
    }

    fn check_literal_match_pattern_type(&mut self, pattern: NodeId, expected: TypeId) {
        let Some(literal) = self.graph.syntax().child(pattern, 0) else {
            return;
        };

        let Some(actual) = self.infer_expression_type(literal) else {
            return;
        };

        if self.is_assignable(expected, actual) {
            return;
        }

        self.report_invalid_match_pattern_type(pattern, expected, actual);
    }

    fn bind_match_binding_pattern_type(&mut self, pattern: NodeId, ty: TypeId) {
        let symbols = self.declaration_symbols_in_node(pattern, &[SymbolKind::PatternBinding]);

        for symbol in symbols {
            self.layer.bind_symbol_type(symbol, ty);
        }

        self.layer.bind_node_type(pattern, ty);
    }

    fn check_variant_match_pattern_type(&mut self, pattern: NodeId, expected: TypeId) {
        if self.check_imported_variant_match_pattern_type(pattern, expected) {
            return;
        }

        let Some((owner_symbol, variant_symbol)) = self.variant_pattern_symbols(pattern) else {
            return;
        };

        let mut owner_type = self
            .layer
            .symbol_type(owner_symbol)
            .unwrap_or_else(|| self.layer.table_mut().intern_named(owner_symbol));

        let mut expected_choice_type = expected;
        let mut generic_arguments = Vec::new();

        if let Some((base, arguments)) =
            self.extract_generic_arguments_from_expected(expected, owner_type)
        {
            expected_choice_type = base;
            generic_arguments = arguments;
        }

        if let Some(target) = self.graph.syntax().child(pattern, 0)
            && let Some(target_type) = self.infer_expression_type(target)
        {
            let resolved = self.resolve_path_type(target_type);
            if let Some(TypeKind::GenericInstance { arguments, .. }) =
                self.layer.table().kind(resolved)
            {
                owner_type = resolved;
                generic_arguments = arguments.clone();
                expected_choice_type = expected;
            } else if !generic_arguments.is_empty() {
                let generic_owner_type = self
                    .layer
                    .table_mut()
                    .intern_generic_instance(owner_type, generic_arguments.clone());
                self.layer.bind_node_type(target, generic_owner_type);
                if let Some(target_ident) = self
                    .graph
                    .syntax()
                    .first_child_of_kind(target, SyntaxNodeKind::Identifier)
                {
                    self.layer.bind_node_type(target_ident, generic_owner_type);
                }
            }
        }

        let assignable = self.is_assignable(expected_choice_type, owner_type);
        if !assignable {
            self.report_invalid_match_pattern_type(pattern, expected_choice_type, owner_type);
            return;
        }

        let Some(resolution) = self.graph.resolution() else {
            return;
        };

        let Some(variant_data) = resolution.symbol(variant_symbol) else {
            return;
        };

        match variant_data.kind() {
            SymbolKind::EnumVariant => {
                self.check_enum_variant_pattern_payload(pattern);
            }

            SymbolKind::ChoiceVariant => {
                self.check_choice_variant_pattern_payload(
                    pattern,
                    owner_symbol,
                    variant_symbol,
                    &generic_arguments,
                );
            }

            _ => {}
        }

        let mut variant_ty = self.layer.table_mut().intern_named(variant_symbol);
        if !generic_arguments.is_empty() {
            variant_ty = self
                .layer
                .table_mut()
                .intern_generic_instance(variant_ty, generic_arguments.clone());
        }

        if let Some(variant_name_node) = self.graph.syntax().child(pattern, 1) {
            self.layer.bind_node_type(variant_name_node, variant_ty);
        }
    }

    fn check_enum_variant_pattern_payload(&mut self, pattern: NodeId) {
        if self
            .graph
            .syntax()
            .first_child_of_kind(pattern, SyntaxNodeKind::VariantPatternPayload)
            .is_none()
        {
            return;
        }

        self.report_argument_count_mismatch(pattern, 0, 1);
    }

    fn check_choice_variant_pattern_payload(
        &mut self,
        pattern: NodeId,
        owner_symbol: SymbolId,
        variant_symbol: SymbolId,
        generic_arguments: &[TypeId],
    ) {
        let mut payload_types = self.choice_variant_payload_types(owner_symbol, variant_symbol);

        if !generic_arguments.is_empty() {
            let choice_type = self
                .layer
                .symbol_type(owner_symbol)
                .unwrap_or_else(|| self.layer.table_mut().intern_named(owner_symbol));
            let mut parameters = Vec::new();
            if let Some(target) = self.graph.syntax().child(pattern, 0) {
                parameters = self.generic_expression_parameter_symbols(target, choice_type);
            }
            if parameters.is_empty() {
                parameters = self.generic_parameter_symbols_from_type(choice_type);
            }
            let substitution = parameters
                .into_iter()
                .zip(generic_arguments.iter().copied())
                .collect::<collections::HashMap<SymbolId, TypeId>>();
            for payload_type in &mut payload_types {
                *payload_type =
                    self.substitute_generic_expression_type(*payload_type, &substitution);
            }
        }

        let payload_patterns = self
            .graph
            .syntax()
            .first_child_of_kind(pattern, SyntaxNodeKind::VariantPatternPayload)
            .and_then(|payload| {
                self.graph
                    .syntax()
                    .node(payload)
                    .map(|node| node.children().to_vec())
            })
            .unwrap_or_default();

        if payload_types.len() != payload_patterns.len() {
            self.report_argument_count_mismatch(
                pattern,
                payload_types.len(),
                payload_patterns.len(),
            );
            return;
        }

        for (payload_pattern, payload_type) in payload_patterns.into_iter().zip(payload_types) {
            self.check_match_pattern_type(payload_pattern, payload_type);
        }
    }

    fn check_imported_variant_match_pattern_type(
        &mut self,
        pattern: NodeId,
        expected: TypeId,
    ) -> bool {
        let Some((owner_symbol, choice, mut variant)) = self.imported_variant_pattern(pattern)
        else {
            return false;
        };

        let owner_type = self.layer.symbol_type(owner_symbol).unwrap_or_else(|| {
            self.layer
                .table_mut()
                .intern_path(owner_symbol, vec![choice.name.clone()])
        });

        let mut expected_choice_type = expected;
        let mut generic_arguments = Vec::new();

        if let Some((base, arguments)) =
            self.extract_generic_arguments_from_expected(expected, owner_type)
        {
            expected_choice_type = base;
            generic_arguments = arguments;
        }

        if !self.is_assignable(expected_choice_type, owner_type) {
            self.report_invalid_match_pattern_type(pattern, expected_choice_type, owner_type);
            return true;
        }

        if !generic_arguments.is_empty() {
            let substitution = choice
                .generic_parameters
                .iter()
                .copied()
                .zip(generic_arguments.clone())
                .collect::<HashMap<_, _>>();

            for payload_type in &mut variant.payload_types {
                *payload_type =
                    self.substitute_generic_expression_type(*payload_type, &substitution);
            }

            if let Some(target) = self.graph.syntax().child(pattern, 0) {
                self.infer_expression_type(target);
                let generic_owner_type = self
                    .layer
                    .table_mut()
                    .intern_generic_instance(owner_type, generic_arguments.clone());
                self.layer.bind_node_type(target, generic_owner_type);
                if let Some(target_ident) = self
                    .graph
                    .syntax()
                    .first_child_of_kind(target, SyntaxNodeKind::Identifier)
                {
                    self.layer.bind_node_type(target_ident, generic_owner_type);
                }
            }
        } else if let Some(target) = self.graph.syntax().child(pattern, 0) {
            self.infer_expression_type(target);
        }

        self.check_imported_choice_variant_pattern_payload(pattern, &variant);

        let mut segments = Vec::new();
        if !self.imported_symbol_choices.contains_key(&owner_symbol) {
            segments.push(choice.name.clone());
        }
        segments.push(variant.name.clone());
        let mut variant_ty = self.layer.table_mut().intern_path(owner_symbol, segments);
        if !generic_arguments.is_empty() {
            variant_ty = self
                .layer
                .table_mut()
                .intern_generic_instance(variant_ty, generic_arguments);
        }

        if let Some(variant_name_node) = self.graph.syntax().child(pattern, 1) {
            self.layer.bind_node_type(variant_name_node, variant_ty);
        }

        true
    }

    fn check_imported_choice_variant_pattern_payload(
        &mut self,
        pattern: NodeId,
        variant: &LoweredImportedChoiceVariant,
    ) {
        let payload_patterns = self
            .graph
            .syntax()
            .first_child_of_kind(pattern, SyntaxNodeKind::VariantPatternPayload)
            .and_then(|payload| {
                self.graph
                    .syntax()
                    .node(payload)
                    .map(|node| node.children().to_vec())
            })
            .unwrap_or_default();

        if variant.payload_types.len() != payload_patterns.len() {
            self.report_argument_count_mismatch(
                pattern,
                variant.payload_types.len(),
                payload_patterns.len(),
            );
            return;
        }

        for (payload_pattern, payload_type) in payload_patterns
            .into_iter()
            .zip(variant.payload_types.iter().copied())
        {
            self.check_match_pattern_type(payload_pattern, payload_type);
        }
    }

    fn imported_variant_pattern(
        &self,
        pattern: NodeId,
    ) -> Option<(
        SymbolId,
        LoweredImportedChoice,
        LoweredImportedChoiceVariant,
    )> {
        let resolution = self.graph.resolution()?;
        let owner_symbol = resolution.reference_symbol(pattern)?;
        let choice = if let Some(c) = self.imported_symbol_choices.get(&owner_symbol) {
            c
        } else {
            let root = self.graph.syntax().child(pattern, 0)?;
            self.imported_path_choices.get(&root)?
        };

        let variant_name = self.variant_pattern_variant_name(pattern)?;
        let variant = choice
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)?;

        Some((owner_symbol, choice.clone(), variant.clone()))
    }

    fn variant_pattern_variant_name(&self, pattern: NodeId) -> Option<String> {
        let variant = self.graph.syntax().child(pattern, 1)?;

        Some(self.node_text(variant))
    }

    fn variant_pattern_symbols(&self, pattern: NodeId) -> Option<(SymbolId, SymbolId)> {
        let resolution = self.graph.resolution()?;

        let owner_symbol = resolution.reference_symbol(pattern)?;
        let variant_symbol = resolution.path_reference_symbol(pattern)?;

        Some((owner_symbol, variant_symbol))
    }

    fn is_error_type(&self, ty: TypeId) -> bool {
        matches!(self.layer.table().kind(ty), Some(TypeKind::Error))
    }

    fn check_choice_match_exhaustiveness(
        &mut self,
        match_expression: NodeId,
        subject_type: TypeId,
        arms: &[NodeId],
    ) {
        let Some(choice_symbol) = self.choice_symbol_from_match_subject_type(subject_type) else {
            if self.check_enum_match_exhaustiveness(match_expression, subject_type, arms) {
                return;
            }

            self.check_imported_choice_match_exhaustiveness(match_expression, subject_type, arms);
            return;
        };

        let variants = self.choice_variant_symbols_in_order(choice_symbol);

        if variants.is_empty() {
            return;
        }

        let mut covered = HashSet::new();

        for arm in arms {
            let Some(pattern) = self.graph.syntax().child(*arm, 0) else {
                continue;
            };

            let Some(pattern_node) = self.graph.syntax().node(pattern) else {
                continue;
            };

            match pattern_node.kind() {
                SyntaxNodeKind::WildcardPattern | SyntaxNodeKind::BindingPattern => {
                    return;
                }
                SyntaxNodeKind::VariantPattern => {
                    let Some((owner_symbol, variant_symbol)) =
                        self.variant_pattern_symbols(pattern)
                    else {
                        continue;
                    };

                    if owner_symbol == choice_symbol {
                        covered.insert(variant_symbol);
                    }
                }
                _ => {}
            }
        }

        let missing = variants
            .into_iter()
            .filter(|(variant_symbol, _)| !covered.contains(variant_symbol))
            .map(|(_, variant_name)| variant_name)
            .collect::<Vec<_>>();

        if missing.is_empty() {
            return;
        }

        self.report_non_exhaustive_match(match_expression, subject_type, missing.as_slice());
    }

    fn check_enum_match_exhaustiveness(
        &mut self,
        match_expression: NodeId,
        subject_type: TypeId,
        arms: &[NodeId],
    ) -> bool {
        let Some(enum_symbol) = self.enum_symbol_from_match_subject_type(subject_type) else {
            return false;
        };

        let variants = self.enum_variant_symbols_in_order(enum_symbol);

        if variants.is_empty() {
            return true;
        }

        let mut covered = HashSet::new();

        for arm in arms {
            let Some(pattern) = self.graph.syntax().child(*arm, 0) else {
                continue;
            };

            if self.is_catch_all_match_pattern(pattern) {
                return true;
            }

            let Some(pattern_node) = self.graph.syntax().node(pattern) else {
                continue;
            };

            if pattern_node.kind() != SyntaxNodeKind::VariantPattern {
                continue;
            }

            let Some((owner_symbol, variant_symbol)) = self.variant_pattern_symbols(pattern) else {
                continue;
            };

            if owner_symbol == enum_symbol {
                covered.insert(variant_symbol);
            }
        }

        let missing = variants
            .into_iter()
            .filter(|(variant_symbol, _)| !covered.contains(variant_symbol))
            .map(|(_, variant_name)| variant_name)
            .collect::<Vec<_>>();

        if !missing.is_empty() {
            self.report_non_exhaustive_match(match_expression, subject_type, missing.as_slice());
        }

        true
    }

    fn check_imported_choice_match_exhaustiveness(
        &mut self,
        match_expression: NodeId,
        subject_type: TypeId,
        arms: &[NodeId],
    ) {
        let Some((choice_symbol, choice)) =
            self.imported_choice_from_match_subject_type(subject_type)
        else {
            return;
        };

        if choice.variants.is_empty() {
            return;
        }

        let mut covered = HashSet::new();

        for arm in arms {
            let Some(pattern) = self.graph.syntax().child(*arm, 0) else {
                continue;
            };

            let Some(pattern_node) = self.graph.syntax().node(pattern) else {
                continue;
            };

            match pattern_node.kind() {
                SyntaxNodeKind::WildcardPattern | SyntaxNodeKind::BindingPattern => {
                    return;
                }
                SyntaxNodeKind::VariantPattern => {
                    let Some(resolution) = self.graph.resolution() else {
                        continue;
                    };

                    if resolution.reference_symbol(pattern) != Some(choice_symbol) {
                        continue;
                    }

                    if let Some(variant_name) = self.variant_pattern_variant_name(pattern) {
                        covered.insert(variant_name);
                    }
                }
                _ => {}
            }
        }

        let missing = choice
            .variants
            .iter()
            .filter(|variant| !covered.contains(variant.name.as_str()))
            .map(|variant| variant.name.clone())
            .collect::<Vec<_>>();

        if missing.is_empty() {
            return;
        }

        self.report_non_exhaustive_match(match_expression, subject_type, missing.as_slice());
    }

    fn imported_choice_from_match_subject_type(
        &self,
        subject_type: TypeId,
    ) -> Option<(SymbolId, &LoweredImportedChoice)> {
        let subject_type = self.resolve_alias_type(subject_type);

        match self.layer.table().kind(subject_type).cloned() {
            Some(TypeKind::Named { symbol }) => self
                .imported_symbol_choices
                .get(&symbol)
                .map(|choice| (symbol, choice)),
            Some(TypeKind::Path { root, .. }) => self
                .imported_path_choices
                .values()
                .find(|choice| !choice.variants.is_empty())
                .map(|choice| (root, choice)),
            Some(TypeKind::GenericInstance { base, .. }) => {
                let base = self.resolve_alias_type(base);
                match self.layer.table().kind(base).cloned() {
                    Some(TypeKind::Named { symbol }) => self
                        .imported_symbol_choices
                        .get(&symbol)
                        .map(|choice| (symbol, choice)),
                    Some(TypeKind::Path { root, .. }) => self
                        .imported_path_choices
                        .values()
                        .find(|choice| !choice.variants.is_empty())
                        .map(|choice| (root, choice)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn choice_symbol_from_match_subject_type(&self, subject_type: TypeId) -> Option<SymbolId> {
        let subject_type = self.resolve_alias_type(subject_type);

        match self.layer.table().kind(subject_type).cloned() {
            Some(TypeKind::Named { symbol }) => self.choice_symbol(symbol),
            Some(TypeKind::GenericInstance { base, .. }) => {
                let base = self.resolve_alias_type(base);

                let Some(TypeKind::Named { symbol }) = self.layer.table().kind(base).cloned()
                else {
                    return None;
                };

                self.choice_symbol(symbol)
            }
            _ => None,
        }
    }

    fn enum_symbol_from_match_subject_type(&self, subject_type: TypeId) -> Option<SymbolId> {
        let subject_type = self.resolve_alias_type(subject_type);

        match self.layer.table().kind(subject_type).cloned() {
            Some(TypeKind::Named { symbol }) => self.enum_symbol(symbol),
            Some(TypeKind::GenericInstance { base, .. }) => {
                let base = self.resolve_alias_type(base);

                let Some(TypeKind::Named { symbol }) = self.layer.table().kind(base).cloned()
                else {
                    return None;
                };

                self.enum_symbol(symbol)
            }
            _ => None,
        }
    }

    fn enum_symbol(&self, symbol: SymbolId) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;
        let symbol_data = resolution.symbol(symbol)?;

        if symbol_data.kind() == SymbolKind::Enum {
            Some(symbol)
        } else {
            None
        }
    }

    fn choice_symbol(&self, symbol: SymbolId) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;
        let symbol_data = resolution.symbol(symbol)?;

        if symbol_data.kind() == SymbolKind::Choice {
            Some(symbol)
        } else {
            None
        }
    }

    fn choice_variant_symbols_in_order(&self, choice_symbol: SymbolId) -> Vec<(SymbolId, String)> {
        let Some(root) = self.graph.syntax().root() else {
            return Vec::new();
        };

        let Some(choice_item) = self.choice_item_node_for_symbol(root, choice_symbol) else {
            return Vec::new();
        };

        let Some(choice_node) = self.graph.syntax().node(choice_item) else {
            return Vec::new();
        };

        let mut variants = Vec::new();

        for child in choice_node.children() {
            self.collect_choice_variant_symbols_in_order(*child, &mut variants);
        }

        variants
    }

    fn enum_variant_symbols_in_order(&self, enum_symbol: SymbolId) -> Vec<(SymbolId, String)> {
        let Some(root) = self.graph.syntax().root() else {
            return Vec::new();
        };

        let Some(enum_item) = self.enum_item_node_for_symbol(root, enum_symbol) else {
            return Vec::new();
        };

        let Some(enum_node) = self.graph.syntax().node(enum_item) else {
            return Vec::new();
        };

        let mut variants = Vec::new();

        for child in enum_node.children() {
            self.collect_enum_variant_symbols_in_order(*child, &mut variants);
        }

        variants
    }

    fn collect_enum_variant_symbols_in_order(
        &self,
        node: NodeId,
        variants: &mut Vec<(SymbolId, String)>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::EnumVariant {
            if let Some(symbol) = self.direct_identifier_symbol(node, SymbolKind::EnumVariant) {
                let name = self.node_text(
                    self.graph
                        .syntax()
                        .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                        .unwrap_or(node),
                );

                variants.push((symbol, name));
            }

            return;
        }

        for child in syntax_node.children() {
            self.collect_enum_variant_symbols_in_order(*child, variants);
        }
    }

    fn enum_item_node_for_symbol(&self, node: NodeId, enum_symbol: SymbolId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::EnumItem
            && self.direct_identifier_symbol(node, SymbolKind::Enum) == Some(enum_symbol)
        {
            return Some(node);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.enum_item_node_for_symbol(*child, enum_symbol) {
                return Some(found);
            }
        }

        None
    }

    fn collect_choice_variant_symbols_in_order(
        &self,
        node: NodeId,
        variants: &mut Vec<(SymbolId, String)>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::ChoiceVariant {
            if let Some(symbol) = self.direct_identifier_symbol(node, SymbolKind::ChoiceVariant) {
                let name = self.node_text(
                    self.graph
                        .syntax()
                        .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                        .unwrap_or(node),
                );

                variants.push((symbol, name));
            }

            return;
        }

        for child in syntax_node.children() {
            self.collect_choice_variant_symbols_in_order(*child, variants);
        }
    }

    fn choice_item_node_for_symbol(&self, node: NodeId, choice_symbol: SymbolId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::ChoiceItem
            && self.direct_identifier_symbol(node, SymbolKind::Choice) == Some(choice_symbol)
        {
            return Some(node);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.choice_item_node_for_symbol(*child, choice_symbol) {
                return Some(found);
            }
        }

        None
    }

    fn check_struct_match_pattern_type(&mut self, pattern: NodeId, expected: TypeId) {
        let type_node = self.graph.syntax().child(pattern, 0).unwrap();

        let Some((struct_symbol, target_type, struct_name)) = self.struct_literal_target(type_node)
        else {
            let target_name = self.node_text(type_node);
            self.report_invalid_struct_literal_target(type_node, target_name.as_str());
            let err = self.layer.table_mut().error();
            self.layer.bind_node_type(pattern, err);
            return;
        };

        if !self.is_assignable(expected, target_type) {
            self.report_invalid_match_pattern_type(pattern, expected, target_type);
            let err = self.layer.table_mut().error();
            self.layer.bind_node_type(pattern, err);
            return;
        }

        let expected_fields = self.struct_fields(struct_symbol);

        let syntax = self.graph.syntax();
        let pattern_node = syntax.node(pattern).unwrap();
        // The rest of children are StructPatternField nodes
        for &field in &pattern_node.children()[1..] {
            let field_node = syntax.node(field).unwrap();
            let field_ident = syntax
                .first_child_of_kind(field, SyntaxNodeKind::Identifier)
                .unwrap();
            let field_name = self.node_text(field_ident);

            let Some(expected_field) = expected_fields
                .iter()
                .find(|candidate| candidate.name == field_name)
            else {
                self.report_unknown_struct_field(field, field_name.as_str(), struct_name.as_str());
                continue;
            };

            self.layer.bind_node_type(field, expected_field.ty);

            // If there's an alias/pattern check it, otherwise bind pattern binding
            if field_node.children().len() > 1 {
                let inner_pattern = field_node.child(1).unwrap();
                self.check_match_pattern_type(inner_pattern, expected_field.ty);
            } else {
                // Shorthand: bind the identifier to the field type
                let symbols =
                    self.declaration_symbols_in_node(field, &[SymbolKind::PatternBinding]);
                for symbol in symbols {
                    self.layer.bind_symbol_type(symbol, expected_field.ty);
                }
            }
        }

        self.layer.bind_node_type(pattern, target_type);
    }
}
