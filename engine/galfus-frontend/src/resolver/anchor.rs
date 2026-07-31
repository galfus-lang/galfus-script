use super::*;
use galfus_core::{Diagnostic, NodeId, SymbolId};

impl<'a> Resolver<'a> {
    pub(super) fn validate_function_anchor_item(&mut self, item: NodeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::ExportItem => {
                if let Some(inner) = node.first_child() {
                    self.validate_function_anchor_item(inner);
                }
            }

            SyntaxNodeKind::FunctionItem => {
                self.validate_function_anchor(item);
            }

            _ => {}
        }
    }

    fn validate_function_anchor(&mut self, function: NodeId) {
        let Some(anchor) = self
            .syntax
            .first_child_of_kind(function, SyntaxNodeKind::FunctionAnchor)
        else {
            return;
        };

        let Some(anchor_type) = self.syntax.first_child(anchor) else {
            return;
        };

        let Some(symbol) = self.anchor_type_symbol(anchor_type) else {
            return;
        };

        let Some(symbol_data) = self.resolution.symbol(symbol) else {
            return;
        };

        match symbol_data.kind() {
            SymbolKind::Struct | SymbolKind::ImportBinding => {}

            _ => {
                let anchor_name = self.node_text(anchor);
                self.report_invalid_function_anchor(anchor, anchor_name);
            }
        }
    }

    fn anchor_type_symbol(&self, node: NodeId) -> Option<SymbolId> {
        let syntax_node = self.syntax.node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::NamedType => self.resolution.type_reference_symbol(node),

            SyntaxNodeKind::Path => self
                .resolution
                .type_path_reference_symbol(node)
                .or_else(|| self.resolution.type_reference_symbol(node)),

            SyntaxNodeKind::GenericType => {
                let base = self.syntax.first_child(node)?;
                self.anchor_type_symbol(base)
            }

            _ => None,
        }
    }

    fn report_invalid_function_anchor(&mut self, anchor: NodeId, anchor_name: String) {
        let Some(anchor_node) = self.syntax.node(anchor) else {
            return;
        };

        self.diagnostics.push(Diagnostic::error_with_message(
            ResolverDiagnosticCode::InvalidFunctionAnchor,
            format!("function anchor `{anchor_name}` must be a struct"),
            anchor_node.span(),
        ));
    }
}
