use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    fn index_assignment_value_type(&self, target: NodeId) -> Option<TypeId> {
        let syntax = self.builder.graph.syntax();
        let target_node = syntax.node(target)?;

        if target_node.kind() != SyntaxNodeKind::IndexExpression {
            return None;
        }

        let array_node = target_node.child(0)?;
        let array_type = self.node_type(array_node)?;
        let resolved_array_type = self.builder.resolve_alias_type(array_type);

        match self
            .builder
            .type_result
            .layer()
            .table()
            .kind(resolved_array_type)
        {
            Some(TypeKind::Array { element }) => Some(*element),
            _ => self.node_type(target),
        }
    }

    pub(super) fn lower_index_assignment(
        &mut self,
        target: NodeId,
        value: NodeId,
        binary_op: Option<MirBinaryOp>,
    ) -> bool {
        let syntax = self.builder.graph.syntax();

        let Some(target_node) = syntax.node(target) else {
            return false;
        };

        if target_node.kind() != SyntaxNodeKind::IndexExpression {
            return false;
        }

        let Some(array_node) = target_node.child(0) else {
            return true;
        };

        let Some(index_node) = target_node.child(1) else {
            return true;
        };

        let array_operand = self.lower_expression(array_node);
        let index_operand = self.lower_expression(index_node);
        let value_type = self.node_type(value).unwrap_or_else(|| TypeId::new(0));
        let expected_value_type = self
            .index_assignment_value_type(target)
            .unwrap_or_else(|| TypeId::new(0));

        let value_operand = self.lower_expression(value);
        let value_operand =
            self.insert_cast_if_needed(value_operand, value_type, expected_value_type);
        let assigned_value = if let Some(binary_op) = binary_op {
            let current = self.declare_local(None, expected_value_type);
            self.current_instructions.push((
                Instruction::Assign(
                    current,
                    RValue::ArrayIndex(array_operand.clone(), index_operand.clone()),
                ),
                None,
            ));

            let result = self.declare_local(None, expected_value_type);
            self.current_instructions.push((
                Instruction::Assign(
                    result,
                    RValue::BinaryOp(binary_op, Operand::Local(current), value_operand),
                ),
                None,
            ));
            Operand::Local(result)
        } else {
            value_operand
        };

        self.current_instructions.push((
            Instruction::StoreIndex {
                arr: array_operand,
                idx: index_operand,
                val: assigned_value,
            },
            None,
        ));

        true
    }

    pub(super) fn lower_member_assignment(
        &mut self,
        target: NodeId,
        value: NodeId,
        binary_op: Option<MirBinaryOp>,
    ) -> bool {
        let syntax = self.builder.graph.syntax();
        let Some(target_node) = syntax.node(target) else {
            return false;
        };
        if target_node.kind() != SyntaxNodeKind::MemberExpression {
            return false;
        }

        let Some(obj_node) = target_node.child(0) else {
            return false;
        };
        let Some(member_node) = target_node.child(1) else {
            return false;
        };

        let obj_operand = self.lower_expression(obj_node);
        let target_ty = self.node_type(target).unwrap_or_else(|| TypeId::new(0));
        let value_ty = self.node_type(value).unwrap_or_else(|| TypeId::new(0));
        let value_operand = self.lower_expression(value);
        let value_operand = self.insert_cast_if_needed(value_operand, value_ty, target_ty);
        let field_name = self.builder.node_text(member_node).to_string();
        let assigned_value = if let Some(binary_op) = binary_op {
            let current = self.declare_local(None, target_ty);
            self.current_instructions.push((
                Instruction::Assign(
                    current,
                    RValue::MemberAccess(obj_operand.clone(), field_name.clone()),
                ),
                None,
            ));

            let result = self.declare_local(None, target_ty);
            self.current_instructions.push((
                Instruction::Assign(
                    result,
                    RValue::BinaryOp(binary_op, Operand::Local(current), value_operand),
                ),
                None,
            ));
            Operand::Local(result)
        } else {
            value_operand
        };

        self.current_instructions.push((
            Instruction::StoreField {
                obj: obj_operand,
                field_name,
                val: assigned_value,
            },
            None,
        ));

        true
    }
}
