

use galfus_core::{NodeId, SymbolId, TypeId};
use crate::{SymbolKind, SyntaxNodeKind, TypeKind};
use super::ExpressionInferrer;

impl<'a> ExpressionInferrer<'a> {
    pub(super) fn infer_inferred_struct_literal_type(
        &mut self,
        node: NodeId,
        expected: TypeId,
    ) -> Option<TypeId> {
        let expected = self.resolve_alias_type(expected);

        let Some((struct_symbol, struct_name)) = self.expected_struct_target(expected) else {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        };

        let Some(fields) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::StructLiteralFieldList)
        else {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        };

        let ty =
            self.check_struct_literal_fields(node, fields, struct_symbol, expected, &struct_name);

        self.layer.bind_node_type(node, ty);

        Some(ty)
    }

    fn expected_struct_target(&self, ty: TypeId) -> Option<(SymbolId, String)> {
        let symbol = match self.layer.table().kind(ty).cloned()? {
            TypeKind::Named { symbol } => symbol,
            TypeKind::GenericInstance { base, .. } => {
                let TypeKind::Named { symbol } = self.layer.table().kind(base).cloned()? else {
                    return None;
                };

                symbol
            }
            _ => return None,
        };

        let resolution = self.graph.resolution()?;
        let symbol_data = resolution.symbol(symbol)?;

        if symbol_data.kind() != SymbolKind::Struct {
            return None;
        }

        Some((symbol, symbol_data.name().to_string()))
    }
}
