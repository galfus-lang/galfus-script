use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_function_expression(&mut self, expression_id: NodeId) -> Operand {
        let type_id = self
            .node_type(expression_id)
            .unwrap_or_else(|| TypeId::new(0));
        let next_local = self.builder.next_local_id;
        let next_block = self.builder.next_block_id;
        let result = self
            .builder
            .build_function_expression(expression_id, type_id)
            .map(|function| {
                let function_id = function.id;
                self.builder.specialized_functions.push(function);
                Operand::Constant(Constant::Function(function_id))
            });
        self.builder.next_local_id = next_local;
        self.builder.next_block_id = next_block;
        result.unwrap_or(Operand::Constant(Constant::Null))
    }
}
