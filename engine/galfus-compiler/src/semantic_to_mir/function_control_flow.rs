use super::function::{FunctionBuilder, LoopTargets};
use galfus_core::NodeId;
use galfus_frontend::SyntaxNodeKind;
use galfus_ir::mir::{Instruction, LocalId};

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn loop_target_name(&self, loop_node: NodeId) -> Option<String> {
        let syntax = self.builder.graph.syntax();
        let metadata =
            syntax.first_child_of_kind(loop_node, SyntaxNodeKind::KeywordMetadataList)?;

        for item in syntax.node(metadata)?.children() {
            let pair = syntax.node(*item)?;
            if pair.kind() != SyntaxNodeKind::KeywordMetadataPair {
                continue;
            }

            let key = pair.child(0)?;
            if self.builder.node_text(key) == "name" {
                return pair
                    .child(1)
                    .map(|value| self.builder.node_text(value).to_string());
            }
        }

        None
    }

    pub(super) fn loop_target_for(&self, statement: NodeId) -> Option<&LoopTargets> {
        let syntax = self.builder.graph.syntax();
        let label = syntax
            .first_child_of_kind(statement, SyntaxNodeKind::Identifier)
            .map(|node| self.builder.node_text(node));

        self.loop_targets.iter().rev().find(|target| match label {
            Some(label) => target.name.as_deref() == Some(label),
            None => true,
        })
    }

    pub(super) fn emit_control_flow_exit(
        &mut self,
        target_scope_depth: usize,
        ret_local: Option<LocalId>,
    ) {
        for i in (target_scope_depth..self.scopes.len()).rev() {
            for &local_id in self.scopes[i].iter().rev() {
                if Some(local_id) == ret_local {
                    continue;
                }
                if let Some(decl) = self.locals.iter().find(|local| local.id == local_id)
                    && self.builder.is_owned_type(decl.ty)
                {
                    self.current_instructions
                        .push((Instruction::Drop(local_id), None));
                }
            }
        }
    }
}
