use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{PathReferenceKind, SymbolKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_name_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let Some(resolution) = self.builder.graph.resolution() else {
            return Operand::Constant(Constant::Null);
        };
        let symbol = resolution.reference_symbol(expression_id).or_else(|| {
            let identifier = syntax
                .first_child_of_kind(expression_id, galfus_frontend::SyntaxNodeKind::Identifier)?;
            resolution.reference_symbol(identifier)
        });
        let Some(symbol) = symbol else {
            return Operand::Constant(Constant::Null);
        };
        if let Some(local) = self.symbol_to_local.get(&symbol).copied() {
            return Operand::Local(local);
        }
        let Some(symbol_data) = resolution.symbol(symbol) else {
            return Operand::Constant(Constant::Null);
        };
        if symbol_data.kind() == SymbolKind::Function {
            return Operand::Constant(Constant::Function(
                self.function_id_for_symbol(symbol, expression_id),
            ));
        }
        if !matches!(
            symbol_data.kind(),
            SymbolKind::Var | SymbolKind::Const | SymbolKind::ImportBinding
        ) {
            return Operand::Constant(Constant::Null);
        }
        let name = self
            .builder
            .string_table
            .resolve(symbol_data.name())
            .unwrap_or("")
            .to_owned();
        let type_id = self
            .builder
            .type_result
            .layer()
            .symbol_type(symbol)
            .unwrap_or_else(|| TypeId::new(0));
        let destination = self.declare_local(None, type_id);
        self.current_instructions.push((
            Instruction::Assign(destination, RValue::LoadGlobal(name)),
            None,
        ));
        Operand::Local(destination)
    }

    pub(super) fn lower_path_expression(&mut self, expression_id: NodeId) -> Operand {
        if let Some((variant_name, owner_type, _)) = self.get_choice_variant_payload(expression_id)
        {
            let expression_type = self
                .builder
                .type_result
                .layer()
                .node_type(expression_id)
                .unwrap_or(owner_type);
            let destination = self.declare_local(None, expression_type);
            self.current_instructions.push((
                Instruction::Assign(
                    destination,
                    RValue::Choice(expression_type, variant_name, None),
                ),
                None,
            ));
            return Operand::Local(destination);
        }

        let Some(resolution) = self.builder.graph.resolution() else {
            return Operand::Constant(Constant::Null);
        };
        if matches!(
            resolution.path_reference_kind(expression_id),
            Some(PathReferenceKind::EnumVariant)
        ) && let Some(symbol) = resolution.path_reference_symbol(expression_id)
        {
            return Operand::Constant(Constant::Int32(self.get_enum_variant_value(symbol) as i32));
        }
        if let Some(symbol) = resolution.path_reference_symbol(expression_id)
            && self
                .owner_symbol_for_member(symbol, SymbolKind::Enum)
                .is_some()
        {
            return Operand::Constant(Constant::Int32(self.get_enum_variant_value(symbol) as i32));
        }
        let syntax = self.builder.graph.syntax();
        if let Some(root) = syntax.child(expression_id, 0)
            && let Some(owner) = resolution.reference_symbol(root)
            && let Some(member) = syntax.child(expression_id, 1)
            && let Some(values) = self
                .builder
                .type_result
                .imported_symbol_enum_values
                .get(&owner)
            && let Some((_, value)) = values
                .iter()
                .find(|(name, _)| name == self.builder.node_text(member))
        {
            return Operand::Constant(Constant::Int32(*value as i32));
        }
        Operand::Constant(Constant::Null)
    }
}
