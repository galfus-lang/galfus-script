use super::function::FunctionBuilder;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::TypeKind;
use galfus_ir::mir::{LocalDecl, LocalId};

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn node_type(&self, node: NodeId) -> Option<TypeId> {
        self.builder
            .type_result
            .layer()
            .node_type(node)
            .map(|type_id| self.substitute_type(type_id))
    }

    pub(super) fn symbol_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.builder
            .type_result
            .layer()
            .symbol_type(symbol)
            .map(|type_id| self.substitute_type(type_id))
    }

    pub(super) fn substitute_type(&self, type_id: TypeId) -> TypeId {
        let type_id = self.builder.resolve_alias_type(type_id);
        match self.builder.type_result.layer().table().kind(type_id) {
            Some(TypeKind::GenericParameter { symbol }) => self
                .type_substitutions
                .get(symbol)
                .copied()
                .unwrap_or(type_id),
            _ => type_id,
        }
    }

    pub(super) fn declare_local(&mut self, symbol: Option<SymbolId>, type_id: TypeId) -> LocalId {
        let local = self.builder.next_local();
        self.locals.push(LocalDecl {
            id: local,
            ty: type_id,
            is_owned: self.builder.is_owned_type(type_id),
        });
        if let Some(symbol) = symbol {
            self.symbol_to_local.insert(symbol, local);
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(local);
        }
        local
    }

    pub(super) fn collect_declaration_symbols(&self, node: NodeId) -> Vec<SymbolId> {
        let mut symbols = Vec::new();
        self.collect_symbols_recursive(node, &mut symbols);
        symbols
    }

    pub(super) fn collect_symbols_recursive(&self, node: NodeId, symbols: &mut Vec<SymbolId>) {
        if let Some(symbol) = self
            .builder
            .graph
            .resolution()
            .and_then(|resolution| resolution.declaration_symbol(node))
        {
            symbols.push(symbol);
        }
        if let Some(syntax_node) = self.builder.graph.syntax().node(node) {
            for &child in syntax_node.children() {
                self.collect_symbols_recursive(child, symbols);
            }
        }
    }
}
