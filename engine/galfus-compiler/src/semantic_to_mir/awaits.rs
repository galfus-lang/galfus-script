use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::SyntaxNodeKind;
use galfus_ir::mir::*;

enum AwaitCollectionKind {
    All,
    Race,
}

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_await_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let target = syntax.child(expression_id, 0).unwrap();
        let future = self.lower_expression(target);
        let destination = self.declare_local(
            None,
            self.node_type(expression_id)
                .unwrap_or_else(|| TypeId::new(0)),
        );
        let drop_future = syntax
            .node(target)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::CallExpression);
        self.current_instructions.push((
            Instruction::Await {
                future,
                destination,
                drop_future,
            },
            None,
        ));
        Operand::Local(destination)
    }

    pub(super) fn lower_await_all_expression(&mut self, expression_id: NodeId) -> Operand {
        self.lower_await_collection(expression_id, AwaitCollectionKind::All)
    }

    pub(super) fn lower_await_race_expression(&mut self, expression_id: NodeId) -> Operand {
        self.lower_await_collection(expression_id, AwaitCollectionKind::Race)
    }

    fn lower_await_collection(
        &mut self,
        expression_id: NodeId,
        kind: AwaitCollectionKind,
    ) -> Operand {
        let target = self.builder.graph.syntax().child(expression_id, 0).unwrap();
        let futures = self.lower_await_futures_list(target);
        let destination = self.declare_local(
            None,
            self.node_type(expression_id)
                .unwrap_or_else(|| TypeId::new(0)),
        );
        let instruction = match kind {
            AwaitCollectionKind::All => Instruction::AwaitAll {
                futures,
                destination,
            },
            AwaitCollectionKind::Race => Instruction::AwaitRace {
                futures,
                destination,
            },
        };
        self.current_instructions.push((instruction, None));
        Operand::Local(destination)
    }

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
