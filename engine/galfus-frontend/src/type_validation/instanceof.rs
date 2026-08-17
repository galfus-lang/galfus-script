use std::collections;

use super::DeclarationTypeChecker;
use crate::{PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, TypeId};

struct InstanceofArmContext<'a> {
    subject_type: TypeId,
    remaining_members: &'a mut Vec<TypeId>,
    subject_symbol: Option<galfus_core::SymbolId>,
    subject_text: &'a str,
    subject_generic: Option<galfus_core::SymbolId>,
    expected: Option<TypeId>,
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_instanceof_expression_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let subject = self.graph.syntax().child(node, 0)?;
        let arms = self.graph.syntax().child(node, 1)?;

        let subject_type = self.infer_expression_type(subject)?;
        let subject_symbol = self.instanceof_subject_reference_symbol(subject);
        let subject_text = self.node_text(subject);
        let subject_generic = self.generic_parameter_symbol(subject_type);

        let arm_nodes = self
            .graph
            .syntax()
            .node(arms)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        if arm_nodes.is_empty() {
            self.report_cannot_infer_type(node, "cannot infer type of empty instanceof expression");

            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        }

        self.check_instanceof_arm_order(arm_nodes.as_slice());

        let mut arm_types = Vec::new();
        let mut remaining_members = self.instanceof_possible_members(subject_type);

        for arm in arm_nodes {
            let context = InstanceofArmContext {
                subject_type,
                remaining_members: &mut remaining_members,
                subject_symbol,
                subject_text: subject_text.as_str(),
                subject_generic,
                expected,
            };
            let Some(arm_type) = self.check_instanceof_arm_type(arm, context) else {
                continue;
            };

            arm_types.push((arm, arm_type));
        }

        self.check_instanceof_exhaustiveness(node, subject_type, remaining_members.as_slice());

        if let Some(expected) = expected {
            let mut has_error = false;

            for (arm, actual) in arm_types.iter().copied() {
                if self.is_instanceof_error_type(actual) {
                    has_error = true;
                    continue;
                }

                let pattern = self.graph.syntax().child(arm, 0).unwrap();
                let pattern_type = self.layer.node_type(pattern).unwrap_or(subject_type);
                let arm_expected =
                    self.branch_expected_type(expected, subject_generic, pattern_type);

                if self.is_assignable(arm_expected, actual) {
                    continue;
                }

                let body = self
                    .graph
                    .syntax()
                    .child(arm, 1)
                    .and_then(|body| self.graph.syntax().child(body, 0))
                    .unwrap_or(arm);
                self.report_incompatible_instanceof_arm_type(body, arm_expected, actual);
                has_error = true;
            }

            let ty = if has_error {
                self.layer.table_mut().error()
            } else {
                expected
            };

            self.layer.bind_node_type(node, ty);
            return Some(ty);
        }

        let Some((_, expected_inferred)) = arm_types
            .iter()
            .copied()
            .find(|(_, ty)| !self.is_instanceof_error_type(*ty))
        else {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        };

        let mut has_error = false;

        for (arm, actual) in arm_types.iter().copied() {
            if self.is_instanceof_error_type(actual) {
                has_error = true;
                continue;
            }

            if self.is_assignable(expected_inferred, actual) {
                continue;
            }

            let body = self
                .graph
                .syntax()
                .child(arm, 1)
                .and_then(|body| self.graph.syntax().child(body, 0))
                .unwrap_or(arm);
            self.report_incompatible_instanceof_arm_type(body, expected_inferred, actual);
            has_error = true;
        }

        let ty = if has_error {
            self.layer.table_mut().error()
        } else {
            expected_inferred
        };

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn check_instanceof_arm_order(&mut self, arms: &[NodeId]) {
        let mut catch_all_seen = false;

        for (index, arm) in arms.iter().copied().enumerate() {
            let Some(pattern) = self.graph.syntax().child(arm, 0) else {
                continue;
            };

            if catch_all_seen {
                self.report_unreachable_pattern(pattern);
                continue;
            }

            if !self.is_catch_all_instanceof_pattern(pattern) {
                continue;
            }

            catch_all_seen = true;

            if index + 1 < arms.len() {
                self.report_catch_all_pattern_not_final(pattern);
            }
        }
    }

    fn check_instanceof_exhaustiveness(
        &mut self,
        instanceof_expression: NodeId,
        subject_type: TypeId,
        remaining_members: &[TypeId],
    ) {
        let missing = remaining_members
            .iter()
            .copied()
            .map(|member| self.describe_type_for_diagnostic(member))
            .collect::<Vec<_>>();

        if missing.is_empty() {
            return;
        }

        self.report_non_exhaustive_instanceof(
            instanceof_expression,
            subject_type,
            missing.as_slice(),
        );
    }

    fn instanceof_possible_members(&self, subject_type: TypeId) -> Vec<TypeId> {
        let subject_type = self.resolve_alias_type(subject_type);

        match self.layer.table().kind(subject_type) {
            Some(TypeKind::Union { members }) => members
                .iter()
                .copied()
                .flat_map(|member| self.instanceof_possible_members(member))
                .collect(),
            Some(TypeKind::GenericParameter { symbol }) => self
                .generic_parameter_bound_type(*symbol)
                .map(|bound| self.instanceof_possible_members(bound))
                .unwrap_or_default(),
            Some(TypeKind::Error) => Vec::new(),
            _ => vec![subject_type],
        }
    }

    fn is_catch_all_instanceof_pattern(&self, pattern: NodeId) -> bool {
        self.graph.syntax().node(pattern).is_some_and(|node| {
            matches!(
                node.kind(),
                SyntaxNodeKind::WildcardPattern | SyntaxNodeKind::BindingPattern
            )
        })
    }

    fn check_instanceof_arm_type(
        &mut self,
        arm: NodeId,
        context: InstanceofArmContext<'_>,
    ) -> Option<TypeId> {
        let pattern = self.graph.syntax().child(arm, 0)?;
        let body = self
            .graph
            .syntax()
            .child(arm, 1)
            .and_then(|body| self.graph.syntax().child(body, 0))?;

        let narrowed_type = self.check_instanceof_pattern_type(
            pattern,
            context.subject_type,
            context.remaining_members,
        );
        let arm_expected = context
            .expected
            .zip(narrowed_type)
            .map(|(expected, pattern_type)| {
                self.branch_expected_type(expected, context.subject_generic, pattern_type)
            })
            .or(context.expected);

        self.infer_instanceof_arm_body_type_with_narrowing(
            body,
            context.subject_symbol,
            context.subject_text,
            narrowed_type,
            context.subject_generic,
            arm_expected,
        )
    }

    fn infer_instanceof_arm_body_type(
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

    fn check_instanceof_pattern_type(
        &mut self,
        pattern: NodeId,
        subject_type: TypeId,
        remaining_members: &mut Vec<TypeId>,
    ) -> Option<TypeId> {
        let pattern_node = self.graph.syntax().node(pattern)?;

        match pattern_node.kind() {
            SyntaxNodeKind::TypePattern => {
                self.check_type_instanceof_pattern_type(pattern, subject_type, remaining_members)
            }

            SyntaxNodeKind::BindingPattern => {
                let remaining_type = self.instanceof_remaining_type(remaining_members.as_slice());
                self.bind_instanceof_binding_pattern_type(pattern, remaining_type);
                remaining_members.clear();
                Some(remaining_type)
            }

            SyntaxNodeKind::WildcardPattern => {
                let remaining_type = self.instanceof_remaining_type(remaining_members.as_slice());
                self.layer.bind_node_type(pattern, remaining_type);
                remaining_members.clear();
                Some(remaining_type)
            }

            _ => None,
        }
    }

    fn check_type_instanceof_pattern_type(
        &mut self,
        pattern: NodeId,
        subject_type: TypeId,
        remaining_members: &mut Vec<TypeId>,
    ) -> Option<TypeId> {
        let type_node = self.first_type_child(pattern)?;
        let pattern_type = self.layer.node_type(type_node)?;

        if !self.is_instanceof_pattern_compatible(pattern_type, remaining_members) {
            self.report_invalid_instanceof_pattern_type(pattern, subject_type, pattern_type);
            return None;
        }

        let matching_members = self.matching_instanceof_members(pattern_type, remaining_members);
        let narrowed_type = if remaining_members.is_empty() {
            pattern_type
        } else {
            self.instanceof_remaining_type(matching_members.as_slice())
        };

        remaining_members.retain(|member| !self.instanceof_type_matches(pattern_type, *member));

        self.bind_instanceof_type_pattern_binding(pattern, narrowed_type);
        self.layer.bind_node_type(pattern, narrowed_type);

        Some(narrowed_type)
    }

    fn infer_instanceof_arm_body_type_with_narrowing(
        &mut self,
        body: NodeId,
        subject_symbol: Option<galfus_core::SymbolId>,
        subject_text: &str,
        narrowed_type: Option<TypeId>,
        subject_generic: Option<galfus_core::SymbolId>,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let Some(subject_symbol) = subject_symbol else {
            return self.infer_instanceof_arm_body_type(body, expected);
        };
        let Some(narrowed_type) = narrowed_type else {
            return self.infer_instanceof_arm_body_type(body, expected);
        };

        if self.instanceof_subject_reference_symbol(body) == Some(subject_symbol)
            || self.node_text(body) == subject_text
        {
            self.layer.bind_node_type(body, narrowed_type);
            return Some(narrowed_type);
        }

        let previous = self.layer.symbol_type(subject_symbol);
        self.layer.bind_symbol_type(subject_symbol, narrowed_type);

        let mut pushed = false;
        if let Some(subject_generic) = subject_generic {
            let mut substitution = collections::HashMap::new();
            substitution.insert(subject_generic, narrowed_type);
            self.active_type_substitutions.push(substitution);
            pushed = true;
        }

        let ty = self.infer_instanceof_arm_body_type(body, expected);

        if pushed {
            self.active_type_substitutions.pop();
        }

        if let Some(previous) = previous {
            self.layer.bind_symbol_type(subject_symbol, previous);
        }

        ty
    }

    fn instanceof_subject_reference_symbol(
        &self,
        expression: NodeId,
    ) -> Option<galfus_core::SymbolId> {
        let resolution = self.graph.resolution()?;

        resolution.reference_symbol(expression).or_else(|| {
            let identifier = self
                .graph
                .syntax()
                .first_child_of_kind(expression, SyntaxNodeKind::Identifier)?;

            resolution.reference_symbol(identifier)
        })
    }

    fn bind_instanceof_type_pattern_binding(&mut self, pattern: NodeId, ty: TypeId) {
        let Some(binding) = self
            .graph
            .syntax()
            .first_child_of_kind(pattern, SyntaxNodeKind::TypePatternBinding)
        else {
            return;
        };

        let symbols = self.declaration_symbols_in_node(binding, &[SymbolKind::TypePatternBinding]);

        for symbol in symbols {
            self.layer.bind_symbol_type(symbol, ty);
        }

        self.layer.bind_node_type(binding, ty);
    }

    fn bind_instanceof_binding_pattern_type(&mut self, pattern: NodeId, ty: TypeId) {
        let symbols = self.declaration_symbols_in_node(pattern, &[SymbolKind::PatternBinding]);

        for symbol in symbols {
            self.layer.bind_symbol_type(symbol, ty);
        }

        self.layer.bind_node_type(pattern, ty);
    }

    fn is_instanceof_pattern_compatible(
        &self,
        pattern_type: TypeId,
        remaining_members: &[TypeId],
    ) -> bool {
        if remaining_members.is_empty() {
            return true;
        }

        remaining_members
            .iter()
            .copied()
            .any(|member| self.instanceof_type_matches(pattern_type, member))
    }

    fn matching_instanceof_members(
        &self,
        pattern_type: TypeId,
        remaining_members: &[TypeId],
    ) -> Vec<TypeId> {
        remaining_members
            .iter()
            .copied()
            .filter(|member| self.instanceof_type_matches(pattern_type, *member))
            .collect()
    }

    fn instanceof_type_matches(&self, pattern_type: TypeId, member_type: TypeId) -> bool {
        let pattern_type = self.resolve_alias_type(pattern_type);
        let member_type = self.resolve_alias_type(member_type);

        if pattern_type == member_type {
            return true;
        }

        match self.layer.table().kind(pattern_type) {
            Some(TypeKind::Union { members }) => members
                .iter()
                .copied()
                .any(|member| self.instanceof_type_matches(member, member_type)),

            Some(TypeKind::Error) => true,

            _ => match self.layer.table().kind(member_type) {
                Some(TypeKind::Union { members }) => members
                    .iter()
                    .copied()
                    .any(|member| self.instanceof_type_matches(pattern_type, member)),

                Some(TypeKind::Error) => true,

                _ => false,
            },
        }
    }

    fn instanceof_remaining_type(&mut self, remaining_members: &[TypeId]) -> TypeId {
        match remaining_members {
            [] => self.layer.table_mut().error(),
            [member] => *member,
            members => self.layer.table_mut().intern_union(members.iter().copied()),
        }
    }

    fn is_instanceof_error_type(&self, ty: TypeId) -> bool {
        matches!(self.layer.table().kind(ty), Some(TypeKind::Error))
    }

    pub(super) fn generic_parameter_symbol(&self, ty: TypeId) -> Option<galfus_core::SymbolId> {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty) {
            Some(TypeKind::GenericParameter { symbol }) => Some(*symbol),
            Some(TypeKind::Union { members }) => members
                .iter()
                .copied()
                .find_map(|member| self.generic_parameter_symbol(member)),
            _ => None,
        }
    }

    pub(super) fn branch_expected_type(
        &mut self,
        expected: TypeId,
        subject_generic: Option<galfus_core::SymbolId>,
        pattern_type: TypeId,
    ) -> TypeId {
        let Some(subject_generic) = subject_generic else {
            return expected;
        };

        let mut substitution = collections::HashMap::new();
        substitution.insert(subject_generic, pattern_type);

        self.substitute_generic_expression_type(expected, &substitution)
    }

    pub(super) fn apply_active_type_substitutions(&mut self, ty: TypeId) -> TypeId {
        if self.active_type_substitutions.is_empty() {
            return ty;
        }

        let mut substitution = collections::HashMap::new();

        for active in &self.active_type_substitutions {
            substitution.extend(active.iter().map(|(symbol, ty)| (*symbol, *ty)));
        }

        self.substitute_generic_expression_type(ty, &substitution)
    }
}
