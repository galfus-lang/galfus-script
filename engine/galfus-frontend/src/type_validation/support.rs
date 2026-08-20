use super::DeclarationTypeChecker;
use crate::{SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn direct_identifier_symbol(
        &self,
        node: NodeId,
        kind: SymbolKind,
    ) -> Option<SymbolId> {
        self.direct_identifier_symbol_any(node, &[kind])
    }

    pub(super) fn direct_identifier_symbol_any(
        &self,
        node: NodeId,
        kinds: &[SymbolKind],
    ) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;
        let syntax_node = self.graph.syntax().node(node)?;

        for child in syntax_node.children() {
            let child_node = self.graph.syntax().node(*child)?;

            if child_node.kind() != SyntaxNodeKind::Identifier {
                continue;
            }

            let Some(symbol) = resolution.declaration_symbol(*child) else {
                continue;
            };

            let Some(symbol_data) = resolution.symbol(symbol) else {
                continue;
            };

            if kinds.contains(&symbol_data.kind()) {
                return Some(symbol);
            }
        }

        None
    }

    pub(super) fn declaration_symbols_in_node(
        &self,
        node: NodeId,
        kinds: &[SymbolKind],
    ) -> Vec<SymbolId> {
        let mut symbols = Vec::new();
        self.collect_declaration_symbols_in_node(node, kinds, &mut symbols);
        symbols
    }

    fn collect_declaration_symbols_in_node(
        &self,
        node: NodeId,
        kinds: &[SymbolKind],
        symbols: &mut Vec<SymbolId>,
    ) {
        let Some(resolution) = self.graph.resolution() else {
            return;
        };

        if let Some(symbol) = resolution.declaration_symbol(node)
            && let Some(symbol_data) = resolution.symbol(symbol)
            && kinds.contains(&symbol_data.kind())
        {
            symbols.push(symbol);
        }

        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        for child in syntax_node.children() {
            self.collect_declaration_symbols_in_node(*child, kinds, symbols);
        }
    }

    pub(super) fn first_type_child(&self, node: NodeId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        for child in syntax_node.children() {
            if self.is_type_node(*child) {
                return Some(*child);
            }

            if let Some(found) = self.first_type_child(*child) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn last_direct_type_child(&self, node: NodeId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        syntax_node
            .children()
            .iter()
            .rev()
            .copied()
            .find(|child| self.is_type_node(*child))
    }

    pub(super) fn is_type_node(&self, node: NodeId) -> bool {
        self.graph
            .syntax()
            .node(node)
            .map(|node| self.is_type_node_kind(node.kind()))
            .unwrap_or(false)
    }

    pub(super) fn is_type_node_kind(&self, kind: SyntaxNodeKind) -> bool {
        matches!(
            kind,
            SyntaxNodeKind::TypeNull
                | SyntaxNodeKind::NamedType
                | SyntaxNodeKind::Path
                | SyntaxNodeKind::ArrayType
                | SyntaxNodeKind::TupleType
                | SyntaxNodeKind::GroupedType
                | SyntaxNodeKind::UnionType
                | SyntaxNodeKind::GenericType
                | SyntaxNodeKind::FunctionType
        )
    }

    pub(super) fn node_text(&self, node: NodeId) -> String {
        let Some(node) = self.graph.syntax().node(node) else {
            return String::new();
        };

        self.source.slice(node.span()).unwrap_or("").to_string()
    }

    pub(super) fn resolve_alias_type(&self, ty: TypeId) -> TypeId {
        let mut visited = Vec::new();

        self.resolve_alias_type_with_visited(ty, &mut visited)
    }

    fn resolve_alias_type_with_visited(&self, ty: TypeId, visited: &mut Vec<SymbolId>) -> TypeId {
        let Some(TypeKind::Named { symbol }) = self.layer.table().kind(ty).cloned() else {
            return ty;
        };

        let Some(resolution) = self.graph.resolution() else {
            return ty;
        };

        let Some(symbol_data) = resolution.symbol(symbol) else {
            return ty;
        };

        if symbol_data.kind() != SymbolKind::TypeAlias
            && symbol_data.kind() != SymbolKind::ImportBinding
        {
            return ty;
        }

        if visited.contains(&symbol) {
            return ty;
        }

        visited.push(symbol);

        let Some(target) = self.layer.symbol_type(symbol) else {
            return ty;
        };

        if target == ty {
            return ty;
        }

        self.resolve_alias_type_with_visited(target, visited)
    }

    pub(super) fn resolve_path_type(&self, ty: TypeId) -> TypeId {
        let ty = self.resolve_alias_type(ty);

        let Some(TypeKind::Path { root, segments }) = self.layer.table().kind(ty).cloned() else {
            return ty;
        };

        let Some(resolution) = self.graph.resolution() else {
            return ty;
        };

        if root == galfus_core::SymbolId::new(0) {
            let mut current_scope = resolution.module_scope();
            let mut resolved_symbol = None;
            for (i, segment) in segments.iter().enumerate() {
                if let Some(id) = self.string_table.get(segment)
                    && let Some(symbol) = resolution.lookup_symbol(current_scope, id)
                {
                    if i == segments.len() - 1 {
                        resolved_symbol = Some(symbol);
                        break;
                    }
                    if let Some(scope) = resolution.member_scope(symbol) {
                        current_scope = scope;
                    } else {
                        return ty;
                    }
                } else {
                    return ty;
                }
            }
            if let Some(sym) = resolved_symbol
                && let Some(target_ty) = self.layer.symbol_type(sym)
            {
                return self.resolve_alias_type(target_ty);
            }
        }

        ty
    }

    pub(super) fn extract_generic_arguments_from_expected(
        &self,
        expected: TypeId,
        owner_type: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        let resolved = self.resolve_alias_type(expected);
        match self.layer.table().kind(resolved).cloned() {
            Some(TypeKind::GenericInstance { base, arguments }) => {
                if self.is_assignable(base, owner_type) {
                    return Some((base, arguments));
                }

                let base_resolved = self.resolve_path_type(base);
                let owner_resolved = self.resolve_path_type(owner_type);

                let base_sym = match self.layer.table().kind(base_resolved).cloned() {
                    Some(TypeKind::Named { symbol }) => Some(symbol),
                    Some(TypeKind::Path { root, .. }) => Some(root),
                    _ => None,
                };

                let owner_sym = match self.layer.table().kind(owner_resolved).cloned() {
                    Some(TypeKind::Named { symbol }) => Some(symbol),
                    Some(TypeKind::Path { root, .. }) => Some(root),
                    _ => None,
                };

                if let (Some(a), Some(b)) = (base_sym, owner_sym)
                    && a == b
                {
                    return Some((base, arguments));
                }
            }
            Some(TypeKind::Union { members }) => {
                for member in members {
                    if let Some(res) =
                        self.extract_generic_arguments_from_expected(member, owner_type)
                    {
                        return Some(res);
                    }
                }
            }
            _ => {}
        }
        None
    }
}
