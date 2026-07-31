use super::DeclarationTypeChecker;
use crate::{SymbolKind, SyntaxNodeKind};
use galfus_core::{NodeId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_initializer_types(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::VarItem
            | SyntaxNodeKind::ConstItem
            | SyntaxNodeKind::VarStatement
            | SyntaxNodeKind::ConstStatement => {
                self.check_binding_initializer_type(node);
            }

            _ => {}
        }

        for child in syntax_node.children() {
            self.check_initializer_types(*child);
        }
    }

    fn check_binding_initializer_type(&mut self, node: NodeId) {
        let Some(initializer) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::Initializer)
        else {
            return;
        };

        let Some(expression) = self.graph.syntax().child(initializer, 0) else {
            return;
        };

        let Some(type_annotation) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::TypeAnnotation)
        else {
            self.infer_unannotated_binding_type(node, expression);
            return;
        };

        let Some(type_node) = self.first_type_child(type_annotation) else {
            return;
        };

        let Some(expected) = self.layer.node_type(type_node) else {
            return;
        };

        let Some(actual) = self.infer_initializer_expression_type(expression, expected) else {
            return;
        };

        if self.is_assignable(expected, actual) {
            return;
        }

        self.report_type_mismatch(expression, expected, actual);
    }

    fn infer_initializer_expression_type(
        &mut self,
        expression: NodeId,
        expected: TypeId,
    ) -> Option<TypeId> {
        let expression_node = self.graph.syntax().node(expression)?;

        if expression_node.kind() == SyntaxNodeKind::InferredStructLiteral {
            return self.infer_inferred_struct_literal_type(expression, expected);
        }

        self.infer_expression_type_with_expected(expression, Some(expected))
    }

    fn infer_unannotated_binding_type(&mut self, node: NodeId, expression: NodeId) {
        let Some(ty) = self.infer_expression_type(expression) else {
            return;
        };

        if let Some(pattern) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::BindingPattern)
        {
            self.bind_binding_pattern_type(pattern, ty);
            return;
        }

        let symbols = self.declaration_symbols_in_node(
            node,
            &[
                SymbolKind::Var,
                SymbolKind::Const,
                SymbolKind::PatternBinding,
                SymbolKind::TypePatternBinding,
            ],
        );

        for symbol in symbols {
            self.layer.bind_symbol_type(symbol, ty);
        }
    }
}
