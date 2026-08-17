use super::DeclarationTypeChecker;
use crate::{ImportedMemberKey, PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::HashSet;

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_member_expression_type(
        &mut self,
        node: NodeId,
        null_safe: bool,
    ) -> Option<TypeId> {
        let target = self.graph.syntax().child(node, 0)?;
        let member = self.graph.syntax().child(node, 1)?;

        let target_type = self.infer_expression_type(target)?;
        let member_name = self.node_text(member);

        let target_contains_null = self.type_contains_null(target_type);

        if target_contains_null && !null_safe {
            self.report_unknown_member(member, member_name.as_str(), target_type);
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);
            return Some(error);
        }

        let target_types = self.non_null_member_target_types(target_type);

        if target_types.is_empty() {
            self.report_unknown_member(member, member_name.as_str(), target_type);
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);
            return Some(error);
        }

        let mut member_types = Vec::new();

        for target_type in target_types {
            let Some(member_type) =
                self.member_type_for_target_type(target_type, member_name.as_str())
            else {
                self.report_unknown_member(member, member_name.as_str(), target_type);
                let error = self.layer.table_mut().error();
                self.layer.bind_node_type(node, error);
                return Some(error);
            };

            member_types.push(member_type);
        }

        if null_safe && target_contains_null {
            member_types.push(self.layer.table().primitive(PrimitiveType::Null));
        }

        let ty = self.layer.table_mut().intern_union(member_types);

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    pub(super) fn infer_index_expression_type(&mut self, node: NodeId) -> Option<TypeId> {
        let target = self.graph.syntax().child(node, 0)?;
        let index = self.graph.syntax().child(node, 1)?;

        let target_type = self.infer_expression_type(target)?;
        let index_type = self.infer_expression_type(index)?;

        if !self.is_integer_type(index_type) {
            self.report_invalid_index_type(index, index_type);
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);
            return Some(error);
        }

        let Some(element_type) = self.index_element_type(target_type) else {
            self.report_invalid_index_target(target, target_type);
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);
            return Some(error);
        };

        let ty = if self.is_statically_in_bounds_array_index(target, index) {
            element_type
        } else {
            let null_type = self.layer.table().primitive(PrimitiveType::Null);
            self.layer
                .table_mut()
                .intern_union([element_type, null_type])
        };

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn is_statically_in_bounds_array_index(&self, target: NodeId, index: NodeId) -> bool {
        let Some(index) = self.static_integer_value(index) else {
            return false;
        };
        let Some(length) = self.static_array_length(target, &mut HashSet::new()) else {
            return false;
        };

        let index = if index < 0 {
            length as i64 + index
        } else {
            index
        };

        index >= 0 && (index as usize) < length
    }

    fn static_integer_value(&self, node: NodeId) -> Option<i64> {
        self.node_text(node).replace('_', "").parse().ok()
    }

    fn static_array_length(&self, node: NodeId, visited: &mut HashSet<SymbolId>) -> Option<usize> {
        let syntax_node = self.graph.syntax().node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::ArrayLiteral => {
                syntax_node
                    .children()
                    .iter()
                    .try_fold(0usize, |len, child| {
                        let child_node = self.graph.syntax().node(*child)?;
                        let child_len = match child_node.kind() {
                            SyntaxNodeKind::ArrayElement => 1,
                            SyntaxNodeKind::SpreadArrayElement => self.static_array_length(
                                self.graph.syntax().child(*child, 0)?,
                                visited,
                            )?,
                            _ => return None,
                        };
                        len.checked_add(child_len)
                    })
            }
            SyntaxNodeKind::NameExpression => {
                let resolution = self.graph.resolution()?;
                let symbol = resolution.reference_symbol(node)?;
                if !visited.insert(symbol) {
                    return None;
                }
                let initializer =
                    self.find_initializer_for_symbol(self.graph.syntax().root()?, symbol)?;
                self.static_array_length(initializer.0, visited)
            }
            _ => None,
        }
    }

    pub(super) fn member_type_for_target_type(
        &mut self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<TypeId> {
        let target_type = self.resolve_alias_type(target_type);

        match self.layer.table().kind(target_type).cloned() {
            Some(TypeKind::Array { .. }) if member_name == "length" => {
                Some(self.layer.table().primitive(PrimitiveType::Int32))
            }

            Some(TypeKind::Named { symbol }) => self
                .member_type_for_symbol(symbol, member_name)
                .or_else(|| {
                    let key = ImportedMemberKey::new(symbol, "", member_name);

                    self.imported_member_types.get(&key).copied()
                }),

            Some(TypeKind::GenericInstance { base, .. }) => {
                let TypeKind::Named { symbol } = self.layer.table().kind(base)? else {
                    return None;
                };

                self.struct_fields_for_target(*symbol, target_type)
                    .into_iter()
                    .find(|field| field.name == member_name)
                    .map(|field| field.ty)
            }

            Some(TypeKind::Path { root, segments }) => {
                let owner = segments.join("::");
                let key = ImportedMemberKey::new(root, owner, member_name);

                self.imported_member_types.get(&key).copied()
            }

            Some(TypeKind::Error) => Some(target_type),

            _ => None,
        }
    }

    fn member_type_for_symbol(&self, symbol: SymbolId, member_name: &str) -> Option<TypeId> {
        let resolution = self.graph.resolution()?;

        let symbol_data = resolution.symbol(symbol)?;

        if !matches!(
            symbol_data.kind(),
            SymbolKind::Struct | SymbolKind::Choice | SymbolKind::Enum
        ) {
            return None;
        }

        if let Some(member_scope) = resolution.member_scope(symbol)
            && let Some(member_symbol) = resolution.scope(member_scope).and_then(|scope| {
                self.string_table
                    .get(member_name)
                    .and_then(|id| scope.symbol(id))
            })
        {
            let member_symbol_data = resolution.symbol(member_symbol)?;

            if member_symbol_data.kind() != SymbolKind::StructField {
                return None;
            }

            return self.layer.symbol_type(member_symbol);
        }

        if symbol_data.kind() == SymbolKind::Struct {
            return self.struct_field_type_for_symbol(symbol, member_name);
        }

        None
    }

    pub(super) fn member_symbol_for_target_type(
        &self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<SymbolId> {
        let target_type = self.resolve_alias_type(target_type);

        match self.layer.table().kind(target_type) {
            Some(TypeKind::Named { symbol }) => self.member_symbol_for_symbol(*symbol, member_name),
            Some(TypeKind::GenericInstance { base, .. }) => {
                let TypeKind::Named { symbol } = self.layer.table().kind(*base)? else {
                    return None;
                };

                self.member_symbol_for_symbol(*symbol, member_name)
            }

            _ => None,
        }
    }

    fn member_symbol_for_symbol(&self, symbol: SymbolId, member_name: &str) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;

        let symbol_data = resolution.symbol(symbol)?;

        if !matches!(
            symbol_data.kind(),
            SymbolKind::Struct | SymbolKind::Choice | SymbolKind::Enum
        ) {
            return None;
        }

        let member_scope = resolution.member_scope(symbol)?;

        let member_symbol = resolution.scope(member_scope).and_then(|scope| {
            self.string_table
                .get(member_name)
                .and_then(|id| scope.symbol(id))
        })?;

        let member_symbol_data = resolution.symbol(member_symbol)?;

        if member_symbol_data.kind() != SymbolKind::StructField {
            return None;
        }

        Some(member_symbol)
    }

    pub(super) fn non_null_member_target_types(&self, ty: TypeId) -> Vec<TypeId> {
        let ty = self.resolve_alias_type(ty);
        let null_type = self.layer.table().primitive(PrimitiveType::Null);

        match self.layer.table().kind(ty) {
            Some(TypeKind::Union { members }) => members
                .iter()
                .copied()
                .filter(|member| *member != null_type)
                .collect(),

            Some(TypeKind::Primitive(PrimitiveType::Null)) => Vec::new(),

            Some(TypeKind::Error) => vec![ty],

            _ => vec![ty],
        }
    }

    fn type_contains_null(&self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);
        let null_type = self.layer.table().primitive(PrimitiveType::Null);

        match self.layer.table().kind(ty) {
            Some(TypeKind::Union { members }) => members.contains(&null_type),
            Some(TypeKind::Primitive(PrimitiveType::Null)) => true,
            _ => false,
        }
    }

    fn index_element_type(&self, target_type: TypeId) -> Option<TypeId> {
        let target_type = self.resolve_alias_type(target_type);

        match self.layer.table().kind(target_type) {
            Some(TypeKind::Array { element }) => Some(*element),

            Some(TypeKind::Error) => Some(target_type),

            _ => None,
        }
    }
}
