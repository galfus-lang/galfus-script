use super::*;
use galfus_core::{NodeId, ScopeId, SymbolId};

impl<'a> Resolver<'a> {
    pub(super) fn resolve_function_scope_item(&mut self, item: NodeId, parent_scope: ScopeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::ExportItem => {
                if let Some(inner) = node.first_child() {
                    self.resolve_function_scope_item(inner, parent_scope);
                }
            }

            SyntaxNodeKind::FunctionItem => {
                self.resolve_function_item_scope(item, parent_scope);
            }

            _ => {}
        }
    }

    fn resolve_function_item_scope(&mut self, function: NodeId, parent_scope: ScopeId) {
        let scope_parent = self
            .function_anchor_struct_scope(function, parent_scope)
            .unwrap_or(parent_scope);
        let function_scope =
            self.resolution
                .add_scope(ScopeKind::Function, Some(scope_parent), Some(function));

        if let Some(parameters) = self
            .syntax
            .first_child_of_kind(function, SyntaxNodeKind::ParameterList)
        {
            self.resolution.bind_scope(parameters, function_scope);
            self.declare_parameter_list(parameters, function_scope);
        }
    }

    fn function_anchor_struct_scope(
        &mut self,
        function: NodeId,
        parent_scope: ScopeId,
    ) -> Option<ScopeId> {
        let anchor = self
            .syntax
            .first_child_of_kind(function, SyntaxNodeKind::FunctionAnchor)?;
        let anchor_type = self.syntax.first_child(anchor)?;
        let anchor_name = self.function_anchor_base_name(anchor_type)?;
        let symbol = self
            .resolution
            .lookup_symbol(parent_scope, NameId::intern(anchor_name.as_str()))?;
        let symbol_data = self.resolution.symbol(symbol)?;

        if symbol_data.kind() != SymbolKind::Struct {
            return None;
        }

        if let Some(scope) = self.resolution.member_scope(symbol) {
            return Some(scope);
        }

        let struct_item = self.struct_item_for_symbol(symbol)?;

        if let Some(scope) = self.resolution.node_scope(struct_item) {
            return Some(scope);
        }

        Some(
            self.resolution
                .add_scope(ScopeKind::Struct, Some(parent_scope), Some(struct_item)),
        )
    }

    fn function_anchor_base_name(&self, node: NodeId) -> Option<String> {
        let syntax_node = self.syntax.node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::NamedType | SyntaxNodeKind::Path => {
                let identifier = self
                    .syntax
                    .first_child_of_kind(node, SyntaxNodeKind::Identifier)?;
                Some(self.node_text(identifier))
            }

            SyntaxNodeKind::GenericType => {
                let base = self.syntax.first_child(node)?;
                self.function_anchor_base_name(base)
            }

            _ => None,
        }
    }

    fn struct_item_for_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
        let root = self.syntax.root()?;
        self.find_struct_item_for_symbol(root, symbol)
    }

    fn find_struct_item_for_symbol(&self, node: NodeId, symbol: SymbolId) -> Option<NodeId> {
        let syntax_node = self.syntax.node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::StructItem
            && self
                .syntax
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .and_then(|name| self.resolution.declaration_symbol(name))
                == Some(symbol)
        {
            return Some(node);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_struct_item_for_symbol(*child, symbol) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn declare_parameter_list(&mut self, parameters: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(parameters) else {
            return;
        };

        for parameter in node.children() {
            self.declare_parameter(*parameter, scope);
        }
    }

    fn declare_parameter(&mut self, parameter: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(parameter) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::Parameter => {
                self.declare_parameter_symbol(parameter, SymbolKind::Parameter, scope);
            }

            SyntaxNodeKind::RestParameter => {
                self.declare_parameter_symbol(parameter, SymbolKind::RestParameter, scope);
            }

            _ => {}
        }
    }

    fn declare_parameter_symbol(&mut self, parameter: NodeId, kind: SymbolKind, scope: ScopeId) {
        if let Some(binding) = self
            .syntax
            .first_child_of_kind(parameter, SyntaxNodeKind::BindingPattern)
        {
            self.declare_binding_pattern(binding, kind, scope);
        } else if let Some(name) = self
            .syntax
            .first_child_of_kind(parameter, SyntaxNodeKind::Identifier)
        {
            let symbol_name = self.node_text(name);
            self.declare_symbol(symbol_name, kind, name, scope);
        }
    }
}
