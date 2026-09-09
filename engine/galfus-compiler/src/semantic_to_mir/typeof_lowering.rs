use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::SyntaxNodeKind;
use galfus_ir::mir::{Constant, Instruction, Operand, RValue, Terminator};

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_typeof_expression(&mut self, expr_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let Some(node) = syntax.node(expr_id) else {
            return Operand::Constant(Constant::Null);
        };
        let Some(subject) = node.child(0) else {
            return Operand::Constant(Constant::Null);
        };
        let Some(arms) = node.child(1) else {
            return Operand::Constant(Constant::Null);
        };
        let Some(subject_type) = self.typeof_subject_type(subject) else {
            return Operand::Constant(Constant::Null);
        };

        let result_type = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
        let result = self.declare_local(None, result_type);
        let end = self.builder.next_block();

        for arm in syntax
            .node(arms)
            .into_iter()
            .flat_map(|arms| arms.children())
        {
            let Some(pattern) = syntax.child(*arm, 0) else {
                continue;
            };
            let Some(body) = syntax.child(*arm, 1).and_then(|body| syntax.child(body, 0)) else {
                continue;
            };
            let is_wildcard = syntax
                .node(pattern)
                .is_some_and(|pattern| pattern.kind() == SyntaxNodeKind::WildcardPattern);
            let matches_subject = self
                .node_type(pattern)
                .is_some_and(|pattern_type| self.builder.is_same_type(pattern_type, subject_type));

            if is_wildcard || matches_subject {
                let body_operand = self.lower_narrowing_arm_body(body, result, end, result_type);
                if !self.is_terminated() {
                    self.current_instructions
                        .push((Instruction::Assign(result, RValue::Use(body_operand)), None));
                    self.close_current_block(Terminator::Jump {
                        target: end,
                        args: Vec::new(),
                    });
                }
                self.begin_block(end);
                return Operand::Local(result);
            }
        }

        self.current_instructions.push((
            Instruction::Assign(result, RValue::Use(Operand::Constant(Constant::Null))),
            None,
        ));
        self.close_current_block(Terminator::Jump {
            target: end,
            args: Vec::new(),
        });
        self.begin_block(end);
        Operand::Local(result)
    }
}
