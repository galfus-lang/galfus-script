
use std::collections::{HashMap, HashSet};
use galfus_core::{NodeId, SymbolId, TypeId};
use crate::type_validation::TypeValidator;
use crate::{ImportedMemberKey, PrimitiveType, SymbolKind, TypeKind, SyntaxNodeKind, FunctionParameterType};

pub type GenericSubstitution = HashMap<SymbolId, TypeId>;

impl<'a> TypeValidator<'a> {
    pub(super) fn infer_expression_type(&self, node: NodeId) -> Option<TypeId> {
        self.layer.node_type(node)
    }

    pub(super) fn infer_expression_type_with_expected(
        &self,
        node: NodeId,
        _expected: Option<TypeId>,
    ) -> Option<TypeId> {
        self.layer.node_type(node)
    }

    pub(super) fn call_argument_nodes(&self, arguments: NodeId) -> Vec<NodeId> {
        let mut nodes = Vec::new();
        let Some(syntax_node) = self.graph.syntax().node(arguments) else { return nodes; };
        for child in syntax_node.children() { nodes.push(*child); }
        nodes
    }

    pub(super) fn member_type_for_target_type(
        &mut self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<TypeId> {
        let target_type = self.resolve_alias_type(target_type);
        match self.layer.table().kind(target_type) {
            Some(TypeKind::Named { symbol }) => {
                let key = ImportedMemberKey::new(*symbol, "", member_name);
                self.imported_member_types.get(&key).copied()
            }
            Some(TypeKind::Path { root, segments }) => {
                let owner = segments.join("::");
                let key = ImportedMemberKey::new(*root, owner, member_name);
                self.imported_member_types.get(&key).copied()
            }
            _ => None,
        }
    }

    pub(super) fn substitute_generic_expression_type(
        &mut self,
        ty: TypeId,
        substitution: &GenericSubstitution,
    ) -> TypeId {
        let ty = self.resolve_alias_type(ty);
        match self.layer.table().kind(ty).cloned() {
            Some(TypeKind::GenericParameter { symbol }) => {
                substitution.get(&symbol).copied().unwrap_or(ty)
            }
            _ => ty,
        }
    }

    pub(super) fn generic_expression_parameter_symbols(
        &self,
        _target: NodeId,
        target_type: TypeId,
    ) -> Vec<SymbolId> {
        self.generic_parameter_symbols_from_type(target_type)
    }

    pub(super) fn generic_parameter_symbols_from_type(&self, ty: TypeId) -> Vec<SymbolId> {
        let mut symbols = Vec::new();
        let ty = self.resolve_alias_type(ty);
        if let Some(TypeKind::GenericParameter { symbol }) = self.layer.table().kind(ty).cloned() {
            symbols.push(symbol);
        }
        symbols
    }

    pub(super) fn infer_substitutions_from_types(
        &mut self,
        parameters: &[SymbolId],
        expected: TypeId,
        actual: TypeId,
        substitutions: &mut HashMap<SymbolId, TypeId>,
    ) {
        let expected = self.resolve_alias_type(expected);
        let actual = self.resolve_alias_type(actual);
        if let Some(TypeKind::GenericParameter { symbol }) = self.layer.table().kind(expected) {
            if parameters.contains(symbol) {
                substitutions.insert(*symbol, actual);
            }
        }
    }

    pub(super) fn is_integer_type(&self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);
        if let Some(TypeKind::Primitive(p)) = self.layer.table().kind(ty) {
            return p.is_int();
        }
        false
    }

    pub(super) fn is_same_numeric_type(&self, a: TypeId, b: TypeId) -> bool {
        self.resolve_alias_type(a) == self.resolve_alias_type(b)
    }

    pub(super) fn is_same_integer_type(&self, a: TypeId, b: TypeId) -> bool {
        let a = self.resolve_alias_type(a);
        let b = self.resolve_alias_type(b);
        a == b && self.is_integer_type(a)
    }

    pub(super) fn rest_parameter_element_type(&self, ty: TypeId) -> Option<TypeId> {
        let ty = self.resolve_alias_type(ty);
        if let Some(TypeKind::Array { element }) = self.layer.table().kind(ty) {
            Some(*element)
        } else {
            None
        }
    }

    pub(super) fn non_null_member_target_types(&self, ty: TypeId) -> Vec<TypeId> {
        vec![ty]
    }

    pub(super) fn infer_member_expression_type(&mut self, node: NodeId, _null_safe: bool) -> Option<TypeId> {
        self.layer.node_type(node)
    }

    pub(super) fn infer_inferred_struct_literal_type(&mut self, node: NodeId, _expected: TypeId) -> Option<TypeId> {
        self.layer.node_type(node)
    }

    pub(super) fn member_symbol_for_target_type(
        &mut self,
        target_type: TypeId,
        member_name: &str,
    ) -> Option<SymbolId> {
        let target_type = self.resolve_alias_type(target_type);
        match self.layer.table().kind(target_type) {
            Some(TypeKind::Named { symbol }) => {
                let resolution = self.graph.resolution()?;
                let member_scope = resolution.member_scope(*symbol)?;
                let scope = resolution.scope(member_scope)?;
                scope.symbol(member_name)
            }
            _ => None,
        }
    }
}
