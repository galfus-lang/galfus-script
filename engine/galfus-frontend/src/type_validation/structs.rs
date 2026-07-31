use super::DeclarationTypeChecker;
use crate::{SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(super) struct StructFieldInfo {
    pub(super) name: String,
    pub(super) ty: TypeId,
    pub(super) has_default: bool,
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_struct_literal_type(&mut self, node: NodeId) -> Option<TypeId> {
        let syntax = self.graph.syntax();
        let target = syntax.child(node, 0)?;
        let fields = syntax
            .node(node)
            .and_then(|n| n.children().last().copied())?;
        let target_name = self.node_text(target);

        let Some((struct_symbol, target_type, struct_name)) = self.struct_literal_target(target)
        else {
            self.report_invalid_struct_literal_target(target, target_name.as_str());

            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        };

        let ty =
            self.check_struct_literal_fields(node, fields, struct_symbol, target_type, struct_name);

        self.layer.bind_node_type(node, ty);

        Some(ty)
    }

    pub(super) fn check_struct_literal_fields(
        &mut self,
        literal: NodeId,
        fields: NodeId,
        struct_symbol: SymbolId,
        target_type: TypeId,
        struct_name: String,
    ) -> TypeId {
        let expected_fields = self.struct_fields_for_target(struct_symbol, target_type);
        let mut provided = HashSet::new();
        let mut explicit = HashSet::new();
        let mut has_error = false;

        let field_nodes = self
            .graph
            .syntax()
            .node(fields)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        for field in field_nodes {
            if self
                .graph
                .syntax()
                .node(field)
                .is_some_and(|node| node.kind() == SyntaxNodeKind::SpreadStructLiteralField)
            {
                self.check_struct_literal_spread_field(
                    field,
                    expected_fields.as_slice(),
                    &mut provided,
                    &mut has_error,
                );
                continue;
            }

            let Some(field_name) = self.struct_literal_field_name(field) else {
                continue;
            };

            if !explicit.insert(field_name.clone()) {
                self.report_duplicate_struct_field(field, field_name.as_str());
                has_error = true;
                continue;
            }

            provided.insert(field_name.clone());

            let Some(expected_field) = expected_fields
                .iter()
                .find(|candidate| candidate.name == field_name)
            else {
                self.report_unknown_struct_field(field, field_name.as_str(), struct_name.as_str());
                has_error = true;
                continue;
            };

            let Some((value_node, actual)) =
                self.struct_literal_field_value_type(field, expected_field.ty)
            else {
                continue;
            };

            if self.is_assignable(expected_field.ty, actual) {
                continue;
            }

            self.report_type_mismatch(value_node, expected_field.ty, actual);
            has_error = true;
        }

        for expected_field in expected_fields.iter() {
            if provided.contains(expected_field.name.as_str()) {
                continue;
            }

            if expected_field.has_default {
                continue;
            }

            self.report_missing_struct_field(
                literal,
                expected_field.name.as_str(),
                struct_name.as_str(),
            );

            has_error = true;
        }

        if has_error {
            self.layer.table_mut().error()
        } else {
            target_type
        }
    }

    pub(super) fn struct_literal_target(
        &mut self,
        target: NodeId,
    ) -> Option<(SymbolId, TypeId, String)> {
        let target_name = self.node_text(target);

        let (symbol, struct_name) = {
            let resolution = self.graph.resolution()?;

            let symbol = resolution
                .type_reference_symbol(target)
                .or_else(|| resolution.reference_symbol(target))
                .or_else(|| {
                    resolution
                        .symbols()
                        .iter()
                        .find(|symbol| {
                            symbol.name() == target_name && symbol.kind() == SymbolKind::Struct
                        })
                        .map(|symbol| symbol.id())
                })?;

            let symbol_data = resolution.symbol(symbol)?;

            if symbol_data.kind() != SymbolKind::Struct {
                return None;
            }

            (symbol, symbol_data.name().to_string())
        };

        let ty = self
            .layer
            .symbol_type(symbol)
            .unwrap_or_else(|| self.layer.table_mut().intern_named(symbol));

        Some((symbol, ty, struct_name))
    }

    pub(super) fn struct_fields(&self, struct_symbol: SymbolId) -> Vec<StructFieldInfo> {
        let mut visited = HashSet::new();
        self.struct_fields_with_visited(struct_symbol, &mut visited)
    }

    pub(super) fn struct_fields_for_target(
        &mut self,
        struct_symbol: SymbolId,
        target_type: TypeId,
    ) -> Vec<StructFieldInfo> {
        let mut fields = self.struct_fields(struct_symbol);
        let substitution = self.struct_generic_substitution(struct_symbol, target_type);

        if substitution.is_empty() {
            return fields;
        }

        for field in fields.iter_mut() {
            field.ty = self.substitute_generic_expression_type(field.ty, &substitution);
        }

        fields
    }

    fn struct_generic_substitution(
        &self,
        struct_symbol: SymbolId,
        target_type: TypeId,
    ) -> HashMap<SymbolId, TypeId> {
        let target_type = self.resolve_alias_type(target_type);

        let Some(TypeKind::GenericInstance { base, arguments }) =
            self.layer.table().kind(target_type).cloned()
        else {
            return HashMap::new();
        };

        let Some(TypeKind::Named { symbol }) = self.layer.table().kind(base) else {
            return HashMap::new();
        };

        if *symbol != struct_symbol {
            return HashMap::new();
        }

        let Some(struct_item) = self.type_item_for_symbol(struct_symbol) else {
            return HashMap::new();
        };

        self.declaration_symbols_in_node(struct_item, &[SymbolKind::GenericParameter])
            .into_iter()
            .zip(arguments)
            .collect()
    }

    fn struct_fields_with_visited(
        &self,
        struct_symbol: SymbolId,
        visited: &mut HashSet<SymbolId>,
    ) -> Vec<StructFieldInfo> {
        if !visited.insert(struct_symbol) {
            return Vec::new();
        }

        let mut fields = self.expanded_struct_fields(struct_symbol, visited);
        let direct_fields = self.direct_struct_fields(struct_symbol);

        for field in direct_fields {
            if let Some(existing) = fields
                .iter_mut()
                .find(|candidate| candidate.name == field.name)
            {
                *existing = field;
                continue;
            }

            fields.push(field);
        }

        fields
    }

    fn direct_struct_fields(&self, struct_symbol: SymbolId) -> Vec<StructFieldInfo> {
        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let Some(struct_symbol_data) = resolution.symbol(struct_symbol) else {
            return Vec::new();
        };

        let struct_name = struct_symbol_data.name().to_string();

        let Some(member_scope) = resolution.member_scope(struct_symbol) else {
            return Vec::new();
        };

        let Some(scope) = resolution.scope(member_scope) else {
            return Vec::new();
        };

        scope
            .symbols()
            .iter()
            .filter_map(|(name, symbol)| {
                let symbol_data = resolution.symbol(*symbol)?;

                if symbol_data.kind() != SymbolKind::StructField {
                    return None;
                }

                let ty = self.layer.symbol_type(*symbol)?;

                Some(StructFieldInfo {
                    name: name.to_string(),
                    ty,
                    has_default: self.struct_field_has_default(
                        struct_name.as_str(),
                        name.as_str(),
                        symbol_data.declaration(),
                    ),
                })
            })
            .collect()
    }

    fn expanded_struct_fields(
        &self,
        struct_symbol: SymbolId,
        visited: &mut HashSet<SymbolId>,
    ) -> Vec<StructFieldInfo> {
        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let Some(struct_symbol_data) = resolution.symbol(struct_symbol) else {
            return Vec::new();
        };

        let Some(struct_item) = self.struct_item_node_by_name(
            self.graph
                .syntax()
                .root()
                .unwrap_or(struct_symbol_data.declaration()),
            struct_symbol_data.name(),
        ) else {
            return Vec::new();
        };

        let Some(field_list) = self
            .graph
            .syntax()
            .first_child_of_kind(struct_item, SyntaxNodeKind::StructFieldList)
        else {
            return Vec::new();
        };

        let Some(field_list_node) = self.graph.syntax().node(field_list) else {
            return Vec::new();
        };

        let mut fields = Vec::new();

        for field in field_list_node.children() {
            let Some(field_node) = self.graph.syntax().node(*field) else {
                continue;
            };

            if field_node.kind() != SyntaxNodeKind::StructExpansion {
                continue;
            }

            let Some(target) = self.graph.syntax().child(*field, 0) else {
                continue;
            };

            let Some(target_type) = self.layer.node_type(target) else {
                continue;
            };

            let Some(target_symbol) = self.struct_symbol_for_type(target_type) else {
                continue;
            };

            for expanded in self.struct_fields_with_visited(target_symbol, visited) {
                if fields
                    .iter()
                    .any(|candidate: &StructFieldInfo| candidate.name == expanded.name)
                {
                    continue;
                }

                fields.push(expanded);
            }
        }

        fields
    }

    fn struct_field_has_default(
        &self,
        struct_name: &str,
        field_name: &str,
        declaration: NodeId,
    ) -> bool {
        if self.node_contains_kind(declaration, SyntaxNodeKind::StructFieldDefault) {
            return true;
        }

        let Some(field) = self.struct_field_node_by_name(struct_name, field_name) else {
            return false;
        };

        self.node_contains_kind(field, SyntaxNodeKind::StructFieldDefault)
    }

    fn struct_field_node_by_name(&self, struct_name: &str, field_name: &str) -> Option<NodeId> {
        let root = self.graph.syntax().root()?;
        let struct_item = self.struct_item_node_by_name(root, struct_name)?;

        self.find_struct_field_node_by_name(struct_item, field_name)
    }

    pub(super) fn struct_item_node_by_name(
        &self,
        node: NodeId,
        struct_name: &str,
    ) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::StructItem {
            let identifier = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)?;

            if self.node_text(identifier) == struct_name {
                return Some(node);
            }
        }

        for child in syntax_node.children() {
            if let Some(found) = self.struct_item_node_by_name(*child, struct_name) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn find_struct_field_node_by_name(
        &self,
        node: NodeId,
        field_name: &str,
    ) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if matches!(
            syntax_node.kind(),
            SyntaxNodeKind::StructField | SyntaxNodeKind::WeakStructField
        ) {
            let identifier = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)?;

            if self.node_text(identifier) == field_name {
                return Some(node);
            }
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_struct_field_node_by_name(*child, field_name) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn node_contains_kind(&self, node: NodeId, kind: SyntaxNodeKind) -> bool {
        if self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.kind() == kind)
            .unwrap_or(false)
        {
            return true;
        }

        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return false;
        };

        for child in syntax_node.children() {
            if self.node_contains_kind(*child, kind) {
                return true;
            }
        }

        false
    }

    fn struct_literal_field_name(&self, field: NodeId) -> Option<String> {
        let field_node = self.graph.syntax().node(field)?;

        match field_node.kind() {
            SyntaxNodeKind::StructLiteralField | SyntaxNodeKind::StructLiteralFieldShorthand => {
                let identifier = self
                    .graph
                    .syntax()
                    .first_child_of_kind(field, SyntaxNodeKind::Identifier)?;

                Some(self.node_text(identifier))
            }

            _ => None,
        }
    }

    fn struct_literal_field_value_type(
        &mut self,
        field: NodeId,
        expected: TypeId,
    ) -> Option<(NodeId, TypeId)> {
        let field_node = self.graph.syntax().node(field)?;

        match field_node.kind() {
            SyntaxNodeKind::StructLiteralField => {
                let value = *field_node.children().get(1)?;
                let ty = self.infer_expression_type_with_expected(value, Some(expected))?;

                Some((value, ty))
            }

            SyntaxNodeKind::StructLiteralFieldShorthand => {
                let identifier = *field_node.children().first()?;
                let ty = self.infer_identifier_reference_type(identifier)?;

                self.layer.bind_node_type(field, ty);

                Some((identifier, ty))
            }

            _ => None,
        }
    }

    fn check_struct_literal_spread_field(
        &mut self,
        field: NodeId,
        expected_fields: &[StructFieldInfo],
        provided: &mut HashSet<String>,
        has_error: &mut bool,
    ) {
        let Some(expression) = self.graph.syntax().child(field, 0) else {
            return;
        };

        let Some(spread_type) = self.infer_expression_type(expression) else {
            return;
        };

        let Some(spread_symbol) = self.struct_symbol_for_type(spread_type) else {
            return;
        };

        for spread_field in self.struct_fields(spread_symbol) {
            let Some(expected_field) = expected_fields
                .iter()
                .find(|candidate| candidate.name == spread_field.name)
            else {
                continue;
            };

            provided.insert(spread_field.name.clone());

            if self.is_assignable(expected_field.ty, spread_field.ty) {
                continue;
            }

            self.report_type_mismatch(field, expected_field.ty, spread_field.ty);
            *has_error = true;
        }
    }

    pub(super) fn struct_field_type_for_symbol(
        &self,
        symbol: SymbolId,
        member_name: &str,
    ) -> Option<TypeId> {
        self.struct_fields(symbol)
            .into_iter()
            .find(|field| field.name == member_name)
            .map(|field| field.ty)
    }

    fn struct_symbol_for_type(&self, ty: TypeId) -> Option<SymbolId> {
        let resolved = self.resolve_alias_type(ty);
        let TypeKind::Named { symbol } = self.layer.table().kind(resolved)? else {
            return None;
        };

        let resolution = self.graph.resolution()?;

        if resolution.symbol(*symbol)?.kind() != SymbolKind::Struct {
            return None;
        }

        Some(*symbol)
    }

    fn infer_identifier_reference_type(&self, identifier: NodeId) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;
        let symbol = resolution.reference_symbol(identifier)?;

        self.layer.symbol_type(symbol)
    }
}
