use super::function::{FunctionBuilder, NarrowingReturnTarget};
use galfus_core::{NodeId, TypeId};
use galfus_frontend::SyntaxNodeKind;
use galfus_ir::mir::{BlockId, Constant, LocalId, Operand};

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_narrowing_arm_body(
        &mut self,
        body: NodeId,
        result: LocalId,
        end: BlockId,
        result_type: TypeId,
    ) -> Operand {
        self.narrowing_return_targets.push(NarrowingReturnTarget {
            result,
            end,
            result_type,
            scope_depth: self.scopes.len(),
        });
        let syntax = self.builder.graph.syntax();
        let operand = if syntax
            .node(body)
            .is_some_and(|body| body.kind() == SyntaxNodeKind::Block)
        {
            self.lower_block(body);
            Operand::Constant(Constant::Null)
        } else {
            self.lower_expression(body)
        };
        self.narrowing_return_targets.pop();
        operand
    }
}
