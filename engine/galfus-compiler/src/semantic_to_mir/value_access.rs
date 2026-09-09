use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::TypeKind;
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_member_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let object_node = syntax.child(expression_id, 0).unwrap();
        let member_node = syntax.child(expression_id, 1).unwrap();
        let member_name = self.builder.node_text(member_node).to_owned();
        let object = self.lower_expression(object_node);
        let object_type = match object {
            Operand::Local(local) => self
                .locals
                .iter()
                .find(|declaration| declaration.id == local)
                .map(|declaration| declaration.ty)
                .unwrap_or_else(|| {
                    self.node_type(object_node)
                        .unwrap_or_else(|| TypeId::new(0))
                }),
            _ => self
                .node_type(object_node)
                .unwrap_or_else(|| TypeId::new(0)),
        };
        let object_type = self.resolve_alias_type(object_type);
        let value = if member_name == "length"
            && matches!(
                self.builder.type_result.layer().table().kind(object_type),
                Some(TypeKind::Array { .. })
            ) {
            RValue::Len(object)
        } else {
            RValue::MemberAccess(object, member_name)
        };
        let destination = self.declare_local(
            None,
            self.node_type(expression_id)
                .unwrap_or_else(|| TypeId::new(0)),
        );
        self.current_instructions
            .push((Instruction::Assign(destination, value), None));
        Operand::Local(destination)
    }

    pub(super) fn lower_index_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let target_node = syntax.child(expression_id, 0).unwrap();
        let index_node = syntax.child(expression_id, 1).unwrap();
        let target = self.lower_expression(target_node);
        let index = self.lower_expression(index_node);
        let target_type = self
            .node_type(target_node)
            .map(|ty| self.resolve_alias_type(ty))
            .unwrap_or_else(|| TypeId::new(0));
        let value = if matches!(
            self.builder.type_result.layer().table().kind(target_type),
            Some(TypeKind::Tuple { .. })
        ) {
            RValue::MemberAccess(target, tuple_index_member_name(index))
        } else {
            RValue::ArrayIndex(target, index)
        };
        let destination = self.declare_local(
            None,
            self.node_type(expression_id)
                .unwrap_or_else(|| TypeId::new(0)),
        );
        self.current_instructions
            .push((Instruction::Assign(destination, value), None));
        Operand::Local(destination)
    }
}

fn tuple_index_member_name(index: Operand) -> String {
    match index {
        Operand::Constant(Constant::Int8(value)) => value.to_string(),
        Operand::Constant(Constant::Int16(value)) => value.to_string(),
        Operand::Constant(Constant::Int32(value)) => value.to_string(),
        Operand::Constant(Constant::Int64(value)) => value.to_string(),
        Operand::Constant(Constant::Uint8(value)) => value.to_string(),
        Operand::Constant(Constant::Uint16(value)) => value.to_string(),
        Operand::Constant(Constant::Uint32(value)) => value.to_string(),
        Operand::Constant(Constant::Uint64(value)) => value.to_string(),
        _ => "0".to_owned(),
    }
}
