use std::mem;

use super::function::FunctionBuilder;
use galfus_core::NodeId;
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn is_terminated(&self) -> bool {
        self.is_block_terminated(self.current_block)
    }

    pub(super) fn is_block_terminated(&self, block: BlockId) -> bool {
        let index = self
            .blocks
            .iter()
            .position(|item| item.id == block)
            .unwrap();
        !matches!(self.blocks[index].terminator.0, Terminator::Return(None))
    }

    pub(super) fn lower_block(&mut self, block_node: NodeId) {
        let syntax = self.builder.graph.syntax();
        let Some(block) = syntax.node(block_node) else {
            self.flush_current_instructions();
            if !self.is_terminated() {
                let index = self
                    .blocks
                    .iter()
                    .position(|item| item.id == self.current_block)
                    .unwrap();
                self.blocks[index].terminator = (Terminator::Return(None), None);
            }
            return;
        };
        self.scopes.push(Vec::new());
        for &statement in block.children() {
            self.lower_statement(statement);
            if self.is_terminated() {
                break;
            }
        }
        if let Some(locals) = self.scopes.pop()
            && !self.is_terminated()
        {
            self.drop_owned_locals(locals);
        }
    }

    pub(super) fn flush_current_instructions(&mut self) {
        let instructions = mem::take(&mut self.current_instructions);
        let index = self
            .blocks
            .iter()
            .position(|item| item.id == self.current_block)
            .unwrap();
        self.blocks[index].instructions.extend(instructions);
    }

    pub(super) fn close_current_block(&mut self, terminator: Terminator) {
        self.flush_current_instructions();
        let index = self
            .blocks
            .iter()
            .position(|item| item.id == self.current_block)
            .unwrap();
        self.blocks[index].terminator = (terminator, None);
    }

    pub(super) fn begin_block(&mut self, id: BlockId) {
        self.blocks.push(BasicBlock {
            id,
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: (Terminator::Return(None), None),
        });
        self.current_block = id;
    }

    fn drop_owned_locals(&mut self, locals: Vec<LocalId>) {
        for local in locals {
            if let Some(declaration) = self.locals.iter().find(|item| item.id == local)
                && self.builder.is_owned_type(declaration.ty)
            {
                self.current_instructions
                    .push((Instruction::Drop(local), None));
            }
        }
    }
}
