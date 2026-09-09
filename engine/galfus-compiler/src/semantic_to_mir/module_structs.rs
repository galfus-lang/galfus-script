use std::collections::HashSet;

use super::MirBuilder;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::{ImportedStructFieldDefault, SymbolKind, SyntaxNodeKind, TypeKind};

impl<'a> MirBuilder<'a> {
    pub(super) fn get_struct_fields(&self, struct_symbol: SymbolId) -> Vec<(String, TypeId)> {
        if let Some(fields) = self.type_result.imported_struct_fields.get(&struct_symbol) {
            return fields
                .iter()
                .map(|field| (field.name.clone(), field.ty))
                .collect();
        }
        let mut visited = HashSet::new();
        self.get_struct_fields_internal(struct_symbol, &mut visited)
    }

    pub(super) fn imported_struct_field_default(
        &self,
        struct_symbol: SymbolId,
        field_name: &str,
    ) -> Option<ImportedStructFieldDefault> {
        self.type_result
            .imported_struct_fields
            .get(&struct_symbol)?
            .iter()
            .find(|field| field.name == field_name)
            .and_then(|field| field.default_value)
    }

    pub(super) fn get_struct_fields_internal(
        &self,
        struct_symbol: SymbolId,
        visited: &mut HashSet<SymbolId>,
    ) -> Vec<(String, TypeId)> {
        if !visited.insert(struct_symbol) {
            return Vec::new();
        }

        let resolution = match self.graph.resolution() {
            Some(resolution) => resolution,
            None => return Vec::new(),
        };
        let struct_symbol_data = match resolution.symbol(struct_symbol) {
            Some(data) => data,
            None => return Vec::new(),
        };

        let mut fields = Vec::new();
        let root = self.graph.syntax().root().unwrap();
        if let Some(item_node) = self.find_struct_item_by_name(
            root,
            self.string_table
                .resolve(struct_symbol_data.name())
                .unwrap_or(""),
        ) {
            let syntax = self.graph.syntax();
            let field_children = syntax
                .first_child_of_kind(item_node, SyntaxNodeKind::StructFieldList)
                .and_then(|field_list| syntax.node(field_list))
                .map(|node| node.children())
                .unwrap_or(&[]);

            for &field_child in field_children {
                let node_kind = syntax.node(field_child).map(|node| node.kind());
                if node_kind == Some(SyntaxNodeKind::StructExpansion) {
                    let target_sym = syntax
                        .child(field_child, 0)
                        .and_then(|target| self.type_result.layer().node_type(target))
                        .and_then(|target_ty| self.struct_symbol_for_type(target_ty));
                    if let Some(target_sym) = target_sym {
                        for (expanded_name, expanded_ty) in
                            self.get_struct_fields_internal(target_sym, visited)
                        {
                            if !fields.iter().any(|(name, _)| *name == expanded_name) {
                                fields.push((expanded_name, expanded_ty));
                            }
                        }
                    }
                } else if node_kind == Some(SyntaxNodeKind::StructField)
                    && let Some(identifier) =
                        syntax.first_child_of_kind(field_child, SyntaxNodeKind::Identifier)
                {
                    let name = self.node_text(identifier).to_string();
                    let field_ty = resolution
                        .declaration_symbol(identifier)
                        .and_then(|symbol| self.type_result.layer().symbol_type(symbol))
                        .or_else(|| self.type_result.layer().node_type(field_child));
                    if let Some(ty) = field_ty
                        && !fields.iter().any(|(field_name, _)| *field_name == name)
                    {
                        fields.push((name, ty));
                    }
                }
            }
        }
        fields
    }

    pub(super) fn find_struct_item_by_name(
        &self,
        node: NodeId,
        struct_name: &str,
    ) -> Option<NodeId> {
        let syntax = self.graph.syntax();
        let syntax_node = syntax.node(node)?;
        if syntax_node.kind() == SyntaxNodeKind::StructItem {
            let has_matching_identifier = syntax
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .is_some_and(|identifier| self.node_text(identifier) == struct_name);
            if has_matching_identifier {
                return Some(node);
            }
        }
        for &child in syntax_node.children() {
            if let Some(found) = self.find_struct_item_by_name(child, struct_name) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn struct_symbol_for_type(&self, ty: TypeId) -> Option<SymbolId> {
        let layer = self.type_result.layer();
        let table = layer.table();
        let mut current = ty;
        loop {
            match table.kind(current) {
                Some(TypeKind::Named { symbol }) => {
                    let resolution = self.graph.resolution()?;
                    let is_struct = resolution
                        .symbol(*symbol)
                        .is_some_and(|symbol_data| symbol_data.kind() == SymbolKind::Struct)
                        || self.type_result.imported_struct_fields.contains_key(symbol);
                    if is_struct {
                        return Some(*symbol);
                    }
                    break;
                }
                Some(TypeKind::GenericInstance { base, .. }) => current = *base,
                _ => break,
            }
        }
        None
    }

    pub(super) fn find_struct_field_default_expr(
        &self,
        struct_symbol: SymbolId,
        field_name: &str,
    ) -> Option<NodeId> {
        let resolution = self.graph.resolution()?;
        let struct_symbol_data = resolution.symbol(struct_symbol)?;
        let root = self.graph.syntax().root().unwrap();
        let struct_item = self.find_struct_item_by_name(
            root,
            self.string_table
                .resolve(struct_symbol_data.name())
                .unwrap_or(""),
        )?;

        let field_node = self.find_struct_field_node_by_name(struct_item, field_name)?;
        let syntax = self.graph.syntax();
        let default_node =
            self.find_descendant_of_kind(field_node, SyntaxNodeKind::StructFieldDefault)?;
        syntax.child(default_node, 0)
    }

    pub(super) fn find_struct_field_node_by_name(
        &self,
        node: NodeId,
        field_name: &str,
    ) -> Option<NodeId> {
        let syntax = self.graph.syntax();
        let syntax_node = syntax.node(node)?;
        if matches!(
            syntax_node.kind(),
            SyntaxNodeKind::StructField | SyntaxNodeKind::WeakStructField
        ) {
            let matches_name = syntax
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .is_some_and(|identifier| self.node_text(identifier) == field_name);
            if matches_name {
                return Some(node);
            }
        }
        for &child in syntax_node.children() {
            if let Some(found) = self.find_struct_field_node_by_name(child, field_name) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn find_descendant_of_kind(
        &self,
        node: NodeId,
        kind: SyntaxNodeKind,
    ) -> Option<NodeId> {
        let syntax = self.graph.syntax();
        let syntax_node = syntax.node(node)?;
        for &child in syntax_node.children() {
            if let Some(child_node) = syntax.node(child) {
                if child_node.kind() == kind {
                    return Some(child);
                }
                if let Some(found) = self.find_descendant_of_kind(child, kind) {
                    return Some(found);
                }
            }
        }
        None
    }
}
