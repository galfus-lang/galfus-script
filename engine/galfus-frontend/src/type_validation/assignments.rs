use super::DeclarationTypeChecker;
use crate::{PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_assignment_types(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::AssignmentStatement {
            self.check_assignment_statement_type(node);
        }

        for child in syntax_node.children() {
            self.check_assignment_types(*child);
        }
    }

    fn check_assignment_statement_type(&mut self, assignment: NodeId) {
        let Some(target) = self.graph.syntax().child(assignment, 0) else {
            return;
        };

        let Some(operator) = self.graph.syntax().child(assignment, 1) else {
            return;
        };

        let Some(value) = self.graph.syntax().child(assignment, 2) else {
            return;
        };

        self.check_assignment_target_mutability(target);

        let Some(expected) = self.assignment_target_type(target) else {
            return;
        };

        let Some(actual) = self.infer_assignment_value_type(operator, expected, value) else {
            return;
        };

        if self.is_assignable(expected, actual) {
            return;
        }

        self.report_type_mismatch(value, expected, actual);
    }

    fn check_assignment_target_mutability(&mut self, target: NodeId) {
        if self.check_member_assignment_target_mutability(target) {
            return;
        }

        let Some(symbol) = self.assignment_target_symbol(target) else {
            return;
        };

        let Some(resolution) = self.graph.resolution() else {
            return;
        };

        let Some(symbol_data) = resolution.symbol(symbol) else {
            return;
        };

        if !matches!(
            symbol_data.kind(),
            SymbolKind::Const
                | SymbolKind::Parameter
                | SymbolKind::RestParameter
                | SymbolKind::ForBinding
        ) {
            return;
        }

        self.report_assignment_to_immutable(target, symbol_data.name());
    }

    fn assignment_target_type(&mut self, target: NodeId) -> Option<TypeId> {
        let target_node = self.graph.syntax().node(target)?;

        match target_node.kind() {
            SyntaxNodeKind::NameExpression => self.infer_expression_type(target),
            SyntaxNodeKind::MemberExpression => self.infer_member_expression_type(target, false),
            _ => None,
        }
    }

    fn check_member_assignment_target_mutability(&mut self, target: NodeId) -> bool {
        let Some(target_node) = self.graph.syntax().node(target) else {
            return false;
        };

        if target_node.kind() != SyntaxNodeKind::MemberExpression {
            return false;
        }

        let Some(receiver) = self.graph.syntax().child(target, 0) else {
            return true;
        };

        let Some(member) = self.graph.syntax().child(target, 1) else {
            return true;
        };

        let Some(receiver_type) = self.infer_expression_type(receiver) else {
            return true;
        };

        let member_name = self.node_text(member);

        let mut immutable_field = false;

        for target_type in self.non_null_member_target_types(receiver_type) {
            let Some(member_symbol) =
                self.member_symbol_for_target_type(target_type, member_name.as_str())
            else {
                continue;
            };

            let Some(resolution) = self.graph.resolution() else {
                continue;
            };

            if resolution.symbol(member_symbol).is_none() {
                continue;
            }

            let Some(field_node) =
                self.struct_field_node_for_member_target(target_type, member_name.as_str())
            else {
                continue;
            };

            if !self.node_contains_kind(field_node, SyntaxNodeKind::StructFieldConst) {
                continue;
            }

            immutable_field = true;
            break;
        }

        if immutable_field {
            self.report_assignment_to_immutable(member, member_name.as_str());
        }

        true
    }

    fn struct_field_node_for_member_target(
        &self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<NodeId> {
        let target_type = self.resolve_alias_type(target_type);
        let symbol = match self.layer.table().kind(target_type)? {
            TypeKind::Named { symbol } => *symbol,
            TypeKind::GenericInstance { base, .. } => {
                let TypeKind::Named { symbol } = self.layer.table().kind(*base)? else {
                    return None;
                };
                *symbol
            }
            _ => return None,
        };

        let resolution = self.graph.resolution()?;
        let struct_name = resolution.symbol(symbol)?.name();
        let root = self.graph.syntax().root()?;
        let struct_item = self.struct_item_node_by_name(root, struct_name)?;

        self.find_struct_field_node_by_name(struct_item, member_name)
    }

    fn assignment_target_symbol(&self, target: NodeId) -> Option<SymbolId> {
        let target_node = self.graph.syntax().node(target)?;

        if target_node.kind() != SyntaxNodeKind::NameExpression {
            return None;
        }

        let resolution = self.graph.resolution()?;

        resolution.reference_symbol(target).or_else(|| {
            let identifier = self
                .graph
                .syntax()
                .first_child_of_kind(target, SyntaxNodeKind::Identifier)?;

            resolution.reference_symbol(identifier)
        })
    }

    fn infer_assignment_value_type(
        &mut self,
        operator: NodeId,
        target_type: TypeId,
        value: NodeId,
    ) -> Option<TypeId> {
        let value_type = self.infer_expression_type_with_expected(value, Some(target_type))?;
        let operator_text = self.node_text(operator);

        match operator_text.as_str() {
            "=" => Some(value_type),

            "+=" | "-=" | "*=" | "/=" | "%=" | "**=" => Some(
                self.check_numeric_compound_assignment_operator(operator, target_type, value_type),
            ),

            "&=" | "|=" | "^=" => Some(self.check_integer_compound_assignment_operator(
                operator,
                target_type,
                value_type,
            )),

            "<<=" | ">>=" => Some(self.check_shift_compound_assignment_operator(
                operator,
                target_type,
                value_type,
            )),

            "??=" => Some(self.check_null_fallback_assignment_operator(
                operator,
                target_type,
                value_type,
            )),

            _ => {
                self.report_unsupported_operator(operator, operator_text.as_str());
                Some(self.layer.table_mut().error())
            }
        }
    }

    fn check_numeric_compound_assignment_operator(
        &mut self,
        operator: NodeId,
        target: TypeId,
        value: TypeId,
    ) -> TypeId {
        if self.is_same_numeric_type(target, value) {
            return target;
        }

        self.report_operator_type_error(
            operator,
            "numeric operands of the same type",
            target,
            value,
        );

        self.layer.table_mut().error()
    }

    fn check_integer_compound_assignment_operator(
        &mut self,
        operator: NodeId,
        target: TypeId,
        value: TypeId,
    ) -> TypeId {
        if self.is_same_integer_type(target, value) {
            return target;
        }

        self.report_operator_type_error(
            operator,
            "integer operands of the same type",
            target,
            value,
        );

        self.layer.table_mut().error()
    }

    fn check_shift_compound_assignment_operator(
        &mut self,
        operator: NodeId,
        target: TypeId,
        value: TypeId,
    ) -> TypeId {
        if self.is_integer_type(target) && self.is_integer_type(value) {
            return target;
        }

        self.report_operator_type_error(operator, "integer operands", target, value);

        self.layer.table_mut().error()
    }

    fn check_null_fallback_assignment_operator(
        &mut self,
        operator: NodeId,
        target: TypeId,
        value: TypeId,
    ) -> TypeId {
        if self.is_nullable_type(target) {
            return value;
        }

        self.report_operator_type_error(operator, "nullable target", target, value);

        self.layer.table_mut().error()
    }

    fn is_nullable_type(&self, ty: TypeId) -> bool {
        let null_type = self.layer.table().primitive(PrimitiveType::Null);

        match self.layer.table().kind(ty) {
            Some(TypeKind::Union { members }) => members.contains(&null_type),
            Some(TypeKind::Primitive(PrimitiveType::Null)) => true,
            Some(TypeKind::Error) => true,
            _ => false,
        }
    }
}
