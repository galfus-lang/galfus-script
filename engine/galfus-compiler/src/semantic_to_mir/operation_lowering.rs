use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_binary_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let left = syntax.child(expression_id, 0).unwrap();
        let operator = syntax.child(expression_id, 1).unwrap();
        let right = syntax.child(expression_id, 2).unwrap();
        let left_operand = self.lower_expression(left);
        let right_operand = self.lower_expression(right);
        let left_type = match &left_operand {
            Operand::Local(local) => self
                .locals
                .iter()
                .find(|declaration| declaration.id == *local)
                .map(|declaration| declaration.ty)
                .unwrap_or_else(|| self.node_type(left).unwrap_or_else(|| TypeId::new(0))),
            _ => self.node_type(left).unwrap_or_else(|| TypeId::new(0)),
        };
        let right_type = self.node_type(right).unwrap_or_else(|| TypeId::new(0));
        let right_operand = self.insert_cast_if_needed(right_operand, right_type, left_type);
        let destination = self.declare_local(
            None,
            self.node_type(expression_id)
                .unwrap_or_else(|| TypeId::new(0)),
        );
        self.current_instructions.push((
            Instruction::Assign(
                destination,
                RValue::BinaryOp(self.lower_binary_op(operator), left_operand, right_operand),
            ),
            None,
        ));
        Operand::Local(destination)
    }

    pub(super) fn lower_unary_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let operator = syntax.child(expression_id, 0).unwrap();
        let operand = self.lower_expression(syntax.child(expression_id, 1).unwrap());
        let destination = self.declare_local(
            None,
            self.node_type(expression_id)
                .unwrap_or_else(|| TypeId::new(0)),
        );
        self.current_instructions.push((
            Instruction::Assign(
                destination,
                RValue::UnaryOp(self.lower_unary_op(operator), operand),
            ),
            None,
        ));
        Operand::Local(destination)
    }

    pub(super) fn lower_cast_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let type_node = syntax.child(expression_id, 0).unwrap();
        let value = self.lower_expression(syntax.child(expression_id, 1).unwrap());
        let type_id = self
            .node_type(expression_id)
            .or_else(|| self.node_type(type_node))
            .unwrap_or_else(|| TypeId::new(0));
        let destination = self.declare_local(None, type_id);
        self.current_instructions.push((
            Instruction::Assign(destination, RValue::Cast(value, type_id)),
            None,
        ));
        Operand::Local(destination)
    }

    pub(super) fn lower_copy_expression(&mut self, expression_id: NodeId) -> Operand {
        let value =
            self.lower_expression(self.builder.graph.syntax().child(expression_id, 0).unwrap());
        let destination = self.declare_local(
            None,
            self.node_type(expression_id)
                .unwrap_or_else(|| TypeId::new(0)),
        );
        self.current_instructions
            .push((Instruction::Assign(destination, RValue::Copy(value)), None));
        Operand::Local(destination)
    }

    pub(super) fn lower_grouped_expression(&mut self, expression_id: NodeId) -> Operand {
        self.builder
            .graph
            .syntax()
            .first_child(expression_id)
            .map(|inner| self.lower_expression(inner))
            .unwrap_or(Operand::Constant(Constant::Null))
    }
}
