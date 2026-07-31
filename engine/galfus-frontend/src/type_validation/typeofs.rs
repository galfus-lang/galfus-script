use std::collections;

use super::DeclarationTypeChecker;
use crate::{PrimitiveType, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_typeof_expression_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let subject = self.graph.syntax().child(node, 0)?;
        let arms = self.graph.syntax().child(node, 1)?;

        let subject_type = self.infer_expression_type(subject)?;
        let subject_generic = self.generic_parameter_symbol(subject_type);

        let arm_nodes = self
            .graph
            .syntax()
            .node(arms)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        if arm_nodes.is_empty() {
            self.report_cannot_infer_type(node, "cannot infer type of empty typeof expression");

            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        }

        self.check_typeof_arm_order(arm_nodes.as_slice());

        let mut arm_types = Vec::new();
        let mut remaining_members = self.typeof_possible_members(subject_type);

        for arm in arm_nodes {
            let Some((arm_type, pattern_type)) = self.check_typeof_arm_type(
                arm,
                subject_type,
                &mut remaining_members,
                subject_generic,
                expected,
            ) else {
                continue;
            };

            arm_types.push((arm, arm_type, pattern_type));
        }

        self.check_typeof_exhaustiveness(node, subject_type, remaining_members.as_ref());

        if let Some(expected) = expected {
            return self.typeof_expected_result(
                node,
                expected,
                subject_generic,
                arm_types.as_slice(),
            );
        }

        self.typeof_inferred_result(node, arm_types.as_slice())
    }

    fn check_typeof_arm_order(&mut self, arms: &[NodeId]) {
        let mut catch_all_seen = false;

        for (index, arm) in arms.iter().copied().enumerate() {
            let Some(pattern) = self.graph.syntax().child(arm, 0) else {
                continue;
            };

            if catch_all_seen {
                self.report_unreachable_pattern(pattern);
                continue;
            }

            if !self.is_typeof_wildcard_pattern(pattern) {
                continue;
            }

            catch_all_seen = true;

            if index + 1 < arms.len() {
                self.report_catch_all_pattern_not_final(pattern);
            }
        }
    }

    fn check_typeof_exhaustiveness(
        &mut self,
        typeof_expression: NodeId,
        subject_type: TypeId,
        remaining_members: Option<&Vec<TypeId>>,
    ) {
        let missing = match remaining_members {
            Some(members) => members
                .iter()
                .copied()
                .map(|member| self.describe_type_for_diagnostic(member))
                .collect::<Vec<_>>(),
            None => vec!["_".to_string()],
        };

        if missing.is_empty() {
            return;
        }

        self.report_non_exhaustive_typeof(typeof_expression, subject_type, missing.as_slice());
    }

    fn check_typeof_arm_type(
        &mut self,
        arm: NodeId,
        subject_type: TypeId,
        remaining_members: &mut Option<Vec<TypeId>>,
        subject_generic: Option<SymbolId>,
        expected: Option<TypeId>,
    ) -> Option<(TypeId, Option<TypeId>)> {
        let pattern = self.graph.syntax().child(arm, 0)?;
        let body = self
            .graph
            .syntax()
            .child(arm, 1)
            .and_then(|body| self.graph.syntax().child(body, 0))?;

        let pattern_type =
            self.check_typeof_pattern_type(pattern, subject_type, remaining_members)?;
        let arm_expected = expected
            .zip(pattern_type)
            .map(|(expected, pattern_type)| {
                self.branch_expected_type(expected, subject_generic, pattern_type)
            })
            .or(expected);

        let arm_type =
            self.infer_typeof_arm_body_type(body, subject_generic, pattern_type, arm_expected)?;

        Some((arm_type, pattern_type))
    }

    fn infer_typeof_arm_body_type(
        &mut self,
        body: NodeId,
        subject_generic: Option<SymbolId>,
        pattern_type: Option<TypeId>,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let body_node = self.graph.syntax().node(body)?;

        if body_node.kind() == SyntaxNodeKind::Block {
            return Some(self.layer.table().primitive(PrimitiveType::Null));
        }

        let mut pushed = false;

        if let Some((subject_generic, pattern_type)) = subject_generic.zip(pattern_type) {
            let mut substitution = collections::HashMap::new();
            substitution.insert(subject_generic, pattern_type);
            self.active_type_substitutions.push(substitution);
            pushed = true;
        }

        let ty = self.infer_expression_type_with_expected(body, expected);

        if pushed {
            self.active_type_substitutions.pop();
        }

        ty
    }

    fn check_typeof_pattern_type(
        &mut self,
        pattern: NodeId,
        subject_type: TypeId,
        remaining_members: &mut Option<Vec<TypeId>>,
    ) -> Option<Option<TypeId>> {
        if self.is_typeof_wildcard_pattern(pattern) {
            let remaining_type = remaining_members
                .as_ref()
                .and_then(|members| self.typeof_remaining_type(members.as_slice()));

            if let Some(members) = remaining_members {
                members.clear();
            }

            if let Some(remaining_type) = remaining_type {
                self.layer.bind_node_type(pattern, remaining_type);
            }

            return Some(remaining_type);
        }

        let pattern_type = self.layer.node_type(pattern)?;

        if !self.is_typeof_pattern_compatible(
            subject_type,
            pattern_type,
            remaining_members.as_ref(),
        ) {
            self.report_invalid_typeof_arm_type(pattern, subject_type, pattern_type);
            return None;
        }

        if let Some(members) = remaining_members {
            members.retain(|member| !self.typeof_type_matches(pattern_type, *member));
        }

        self.layer.bind_node_type(pattern, pattern_type);
        Some(Some(pattern_type))
    }

    fn typeof_possible_members(&self, subject_type: TypeId) -> Option<Vec<TypeId>> {
        let subject_type = self.resolve_alias_type(subject_type);

        match self.layer.table().kind(subject_type) {
            Some(TypeKind::GenericParameter { symbol }) => self
                .generic_parameter_bound_type(*symbol)
                .and_then(|bound| self.typeof_possible_members(bound)),
            Some(TypeKind::Union { members }) => Some(members.clone()),
            Some(TypeKind::Error) => Some(Vec::new()),
            _ => Some(vec![subject_type]),
        }
    }

    fn is_typeof_pattern_compatible(
        &self,
        subject_type: TypeId,
        pattern_type: TypeId,
        remaining_members: Option<&Vec<TypeId>>,
    ) -> bool {
        if let Some(members) = remaining_members {
            return members
                .iter()
                .copied()
                .any(|member| self.typeof_type_matches(pattern_type, member));
        }

        self.typeof_type_matches(pattern_type, subject_type)
            || self.generic_parameter_symbol(subject_type).is_some()
    }

    fn typeof_type_matches(&self, pattern_type: TypeId, member_type: TypeId) -> bool {
        let pattern_type = self.resolve_alias_type(pattern_type);
        let member_type = self.resolve_alias_type(member_type);

        if pattern_type == member_type {
            return true;
        }

        match self.layer.table().kind(pattern_type) {
            Some(TypeKind::Union { members }) => members
                .iter()
                .copied()
                .any(|member| self.typeof_type_matches(member, member_type)),
            Some(TypeKind::Error) => true,
            _ => match self.layer.table().kind(member_type) {
                Some(TypeKind::Union { members }) => members
                    .iter()
                    .copied()
                    .any(|member| self.typeof_type_matches(pattern_type, member)),
                Some(TypeKind::Error) => true,
                _ => false,
            },
        }
    }

    fn typeof_remaining_type(&mut self, remaining_members: &[TypeId]) -> Option<TypeId> {
        match remaining_members {
            [] => None,
            [member] => Some(*member),
            members => Some(self.layer.table_mut().intern_union(members.iter().copied())),
        }
    }

    fn typeof_expected_result(
        &mut self,
        node: NodeId,
        expected: TypeId,
        subject_generic: Option<SymbolId>,
        arm_types: &[(NodeId, TypeId, Option<TypeId>)],
    ) -> Option<TypeId> {
        let mut has_error = false;

        for (arm, actual, pattern_type) in arm_types.iter().copied() {
            if self.is_typeof_error_type(actual) {
                has_error = true;
                continue;
            }

            let arm_expected = pattern_type
                .map(|pattern_type| {
                    self.branch_expected_type(expected, subject_generic, pattern_type)
                })
                .unwrap_or(expected);

            if self.is_assignable(arm_expected, actual) {
                continue;
            }

            let body = self
                .graph
                .syntax()
                .child(arm, 1)
                .and_then(|body| self.graph.syntax().child(body, 0))
                .unwrap_or(arm);
            self.report_incompatible_typeof_arm_type(body, arm_expected, actual);
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

    fn typeof_inferred_result(
        &mut self,
        node: NodeId,
        arm_types: &[(NodeId, TypeId, Option<TypeId>)],
    ) -> Option<TypeId> {
        let Some((_, expected, _)) = arm_types
            .iter()
            .copied()
            .find(|(_, ty, _)| !self.is_typeof_error_type(*ty))
        else {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        };

        let mut has_error = false;

        for (arm, actual, _) in arm_types.iter().copied() {
            if self.is_typeof_error_type(actual) {
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
            self.report_incompatible_typeof_arm_type(body, expected, actual);
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

    fn is_typeof_wildcard_pattern(&self, pattern: NodeId) -> bool {
        self.graph
            .syntax()
            .node(pattern)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::WildcardPattern)
    }

    fn is_typeof_error_type(&self, ty: TypeId) -> bool {
        matches!(self.layer.table().kind(ty), Some(TypeKind::Error))
    }
}
