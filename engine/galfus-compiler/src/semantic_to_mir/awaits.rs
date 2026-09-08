use super::function::FunctionBuilder;
use galfus_core::NodeId;
use galfus_frontend::SyntaxNodeKind;
use galfus_ir::mir::Operand;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_await_futures_list(&mut self, target_id: NodeId) -> Vec<Operand> {
        let syntax = self.builder.graph.syntax();
        let inner_id = if let Some(target_node) = syntax.node(target_id) {
            if target_node.kind() == SyntaxNodeKind::GroupedExpression {
                target_node.first_child().unwrap_or(target_id)
            } else {
                target_id
            }
        } else {
            target_id
        };

        let Some(inner_node) = syntax.node(inner_id) else {
            return vec![self.lower_expression(target_id)];
        };
        if inner_node.kind() == SyntaxNodeKind::TupleExpression {
            inner_node
                .children()
                .iter()
                .map(|&child_id| self.lower_expression(child_id))
                .collect()
        } else {
            vec![self.lower_expression(inner_id)]
        }
    }
}
