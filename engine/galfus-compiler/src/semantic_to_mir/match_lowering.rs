use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_ir::mir::{Constant, Instruction, Operand, RValue, Terminator};

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_match_expression(&mut self, expr_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let Some(node) = syntax.node(expr_id) else {
            return Operand::Constant(Constant::Null);
        };
        let Some(subject_node) = node.child(0) else {
            return Operand::Constant(Constant::Null);
        };
        let Some(arms_node) = node.child(1) else {
            return Operand::Constant(Constant::Null);
        };

        let match_type = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
        let subject_op = self.lower_expression(subject_node);
        let mut subject_type = self
            .node_type(subject_node)
            .unwrap_or_else(|| TypeId::new(0));
        if subject_type.raw() == 0
            && let Operand::Local(local_id) = &subject_op
            && let Some(local_decl) = self.locals.iter().find(|local| local.id == *local_id)
        {
            subject_type = local_decl.ty;
        }

        let subject_temp = self.declare_local(None, subject_type);
        self.current_instructions.push((
            Instruction::Assign(subject_temp, RValue::Use(subject_op)),
            None,
        ));
        let subject_local_op = Operand::Local(subject_temp);
        let match_result = self.declare_local(None, match_type);

        let Some(arms_syntax_node) = syntax.node(arms_node) else {
            return Operand::Constant(Constant::Null);
        };
        let arm_nodes = arms_syntax_node.children().to_vec();
        let match_end = self.builder.next_block();
        let mut next_condition_block = None;

        for arm_node in arm_nodes {
            if let Some(block) = next_condition_block {
                self.begin_block(block);
            }
            let Some(pattern_node) = syntax.child(arm_node, 0) else {
                continue;
            };
            let Some(body_node) = syntax
                .child(arm_node, 1)
                .and_then(|body| syntax.child(body, 0))
            else {
                continue;
            };

            let arm_body_block = self.builder.next_block();
            let next_arm_block = self.builder.next_block();
            self.lower_pattern_check(
                pattern_node,
                &subject_local_op,
                arm_body_block,
                next_arm_block,
            );
            self.begin_block(arm_body_block);

            let body_op =
                self.lower_narrowing_arm_body(body_node, match_result, match_end, match_type);
            if !self.is_terminated() {
                self.current_instructions.push((
                    Instruction::Assign(match_result, RValue::Use(body_op)),
                    None,
                ));
                self.close_current_block(Terminator::Jump {
                    target: match_end,
                    args: Vec::new(),
                });
            }
            next_condition_block = Some(next_arm_block);
        }

        if let Some(block) = next_condition_block {
            self.begin_block(block);
            self.close_current_block(Terminator::Panic(
                "non-exhaustive match expression".to_string(),
            ));
        }
        self.begin_block(match_end);
        Operand::Local(match_result)
    }
}
