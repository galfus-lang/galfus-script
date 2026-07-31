use super::*;
use crate::SyntaxNodeKind;
use galfus_core::{ExportId, NodeId, SymbolId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRecord {
    id: ExportId,
    name: String,
    kind: SymbolKind,

    export_node: NodeId,
    item_node: NodeId,
    declaration: NodeId,

    symbol: SymbolId,
}

impl ExportRecord {
    pub fn new(
        id: ExportId,
        name: String,
        kind: SymbolKind,
        export_node: NodeId,
        item_node: NodeId,
        declaration: NodeId,
        symbol: SymbolId,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            export_node,
            item_node,
            declaration,
            symbol,
        }
    }

    pub fn id(&self) -> ExportId {
        self.id
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn kind(&self) -> SymbolKind {
        self.kind
    }

    pub fn export_node(&self) -> NodeId {
        self.export_node
    }

    pub fn item_node(&self) -> NodeId {
        self.item_node
    }

    pub fn declaration(&self) -> NodeId {
        self.declaration
    }

    pub fn symbol(&self) -> SymbolId {
        self.symbol
    }
}

impl<'a> Resolver<'a> {
    pub(super) fn build_export_surface_item(&mut self, item: NodeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        if node.kind() != SyntaxNodeKind::ExportItem {
            return;
        }

        let Some(inner) = node.first_child() else {
            return;
        };

        self.export_top_level_item(item, inner);
    }

    fn export_top_level_item(&mut self, export_node: NodeId, item: NodeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::FunctionItem => {
                if let Some(name) = self.function_name(item) {
                    self.export_function_declaration(export_node, item, name);
                }
            }

            SyntaxNodeKind::TypeAliasItem
            | SyntaxNodeKind::StructItem
            | SyntaxNodeKind::EnumItem
            | SyntaxNodeKind::ChoiceItem
            | SyntaxNodeKind::ConstraintItem => {
                if let Some(name) = self
                    .syntax
                    .first_child_of_kind(item, SyntaxNodeKind::Identifier)
                {
                    self.export_declaration(export_node, item, name);
                }
            }

            SyntaxNodeKind::VarItem | SyntaxNodeKind::ConstItem => {
                if let Some(binding) = self
                    .syntax
                    .first_child_of_kind(item, SyntaxNodeKind::BindingPattern)
                {
                    self.export_binding_pattern(export_node, item, binding);
                }
            }

            _ => {}
        }
    }

    fn export_function_declaration(
        &mut self,
        export_node: NodeId,
        item_node: NodeId,
        declaration: NodeId,
    ) {
        let Some(symbol) = self.resolution.declaration_symbol(declaration) else {
            return;
        };

        let Some(symbol_data) = self.resolution.symbol(symbol) else {
            return;
        };

        self.resolution.add_export(
            self.function_symbol_name(item_node, declaration),
            symbol_data.kind(),
            export_node,
            item_node,
            declaration,
            symbol,
        );
    }

    fn export_binding_pattern(&mut self, export_node: NodeId, item: NodeId, pattern: NodeId) {
        let Some(node) = self.syntax.node(pattern) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::BindingPattern => {
                if let Some(inner) = node.first_child() {
                    self.export_binding_pattern(export_node, item, inner);
                }
            }

            SyntaxNodeKind::Identifier => {
                self.export_declaration(export_node, item, pattern);
            }

            SyntaxNodeKind::StructBindingPattern => {
                for field in node.children() {
                    self.export_binding_pattern(export_node, item, *field);
                }
            }

            SyntaxNodeKind::StructBindingField => match node.child_count() {
                0 => {}

                1 => {
                    if let Some(name) = node.first_child() {
                        self.export_declaration(export_node, item, name);
                    }
                }

                _ => {
                    if let Some(alias_pattern) = node.child(1) {
                        self.export_binding_pattern(export_node, item, alias_pattern);
                    }
                }
            },

            SyntaxNodeKind::TupleBindingPattern | SyntaxNodeKind::ArrayBindingPattern => {
                for child in node.children() {
                    self.export_binding_pattern(export_node, item, *child);
                }
            }

            SyntaxNodeKind::RestBindingPattern => {
                if let Some(inner) = node.first_child() {
                    self.export_binding_pattern(export_node, item, inner);
                }
            }

            _ => {}
        }
    }

    fn export_declaration(&mut self, export_node: NodeId, item: NodeId, declaration: NodeId) {
        let Some(symbol) = self.resolution.declaration_symbol(declaration) else {
            return;
        };

        let Some(symbol_data) = self.resolution.symbol(symbol) else {
            return;
        };

        let name = symbol_data.name().to_string();
        let kind = symbol_data.kind();

        self.resolution
            .add_export(name, kind, export_node, item, declaration, symbol);
    }
}
