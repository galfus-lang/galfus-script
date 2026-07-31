use super::*;
use galfus_core::{NodeId, ScopeId};

impl<'a> Resolver<'a> {
    pub(super) fn resolve_type_member_scope_item(&mut self, item: NodeId, module_scope: ScopeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::ExportItem => {
                if let Some(inner) = node.first_child() {
                    self.resolve_type_member_scope_item(inner, module_scope);
                }
            }

            SyntaxNodeKind::StructItem => {
                let scope = self.resolution.node_scope(item).unwrap_or_else(|| {
                    self.add_type_item_scope(item, ScopeKind::Struct, module_scope)
                });

                self.bind_type_member_scope(item, scope);
                self.declare_struct_members(item, scope);
            }

            SyntaxNodeKind::EnumItem => {
                let scope = self.resolution.node_scope(item).unwrap_or_else(|| {
                    self.add_type_item_scope(item, ScopeKind::Enum, module_scope)
                });

                self.bind_type_member_scope(item, scope);
                self.declare_enum_members(item, scope);
            }

            SyntaxNodeKind::ChoiceItem => {
                let scope = self.resolution.node_scope(item).unwrap_or_else(|| {
                    self.add_type_item_scope(item, ScopeKind::Choice, module_scope)
                });

                self.bind_type_member_scope(item, scope);
                self.declare_choice_members(item, scope);
            }

            SyntaxNodeKind::ConstraintItem => {
                let scope = self.resolution.node_scope(item).unwrap_or_else(|| {
                    self.add_type_item_scope(item, ScopeKind::Constraint, module_scope)
                });

                self.bind_type_member_scope(item, scope);
                self.declare_constraint_members(item, scope);
            }

            _ => {}
        }
    }

    fn add_type_item_scope(
        &mut self,
        item: NodeId,
        kind: ScopeKind,
        parent_scope: ScopeId,
    ) -> ScopeId {
        self.resolution
            .add_scope(kind, Some(parent_scope), Some(item))
    }

    fn bind_type_member_scope(&mut self, item: NodeId, scope: ScopeId) {
        let Some(name) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        if let Some(symbol) = self.resolution.declaration_symbol(name) {
            self.resolution.bind_member_scope(symbol, scope);
        }
    }

    fn declare_struct_members(&mut self, item: NodeId, scope: ScopeId) {
        let Some(fields) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::StructFieldList)
        else {
            return;
        };

        let Some(fields_node) = self.syntax.node(fields) else {
            return;
        };

        for field in fields_node.children() {
            let Some(field_node) = self.syntax.node(*field) else {
                continue;
            };

            match field_node.kind() {
                SyntaxNodeKind::StructField | SyntaxNodeKind::WeakStructField => {
                    self.declare_type_member(*field, SymbolKind::StructField, scope);
                }

                _ => {}
            }
        }
    }

    fn declare_enum_members(&mut self, item: NodeId, scope: ScopeId) {
        let Some(variants) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::EnumVariantList)
        else {
            return;
        };

        let Some(variants_node) = self.syntax.node(variants) else {
            return;
        };

        for variant in variants_node.children() {
            self.declare_type_member(*variant, SymbolKind::EnumVariant, scope);
        }
    }

    fn declare_choice_members(&mut self, item: NodeId, scope: ScopeId) {
        let Some(variants) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::ChoiceVariantList)
        else {
            return;
        };

        let Some(variants_node) = self.syntax.node(variants) else {
            return;
        };

        for variant in variants_node.children() {
            self.declare_type_member(*variant, SymbolKind::ChoiceVariant, scope);
        }
    }

    fn declare_constraint_members(&mut self, item: NodeId, scope: ScopeId) {
        let Some(members) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::ConstraintMemberList)
        else {
            return;
        };

        let Some(members_node) = self.syntax.node(members) else {
            return;
        };

        for member in members_node.children() {
            let Some(member_node) = self.syntax.node(*member) else {
                continue;
            };

            match member_node.kind() {
                SyntaxNodeKind::ConstraintField => {
                    self.declare_type_member(*member, SymbolKind::ConstraintField, scope);
                }

                SyntaxNodeKind::ConstraintFunctionSignature => {
                    self.declare_type_member(*member, SymbolKind::ConstraintFunction, scope);
                }

                _ => {}
            }
        }
    }

    fn declare_type_member(&mut self, member: NodeId, kind: SymbolKind, scope: ScopeId) {
        let Some(name) = self
            .syntax
            .first_child_of_kind(member, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        let symbol_name = self.node_text(name);

        self.declare_symbol(symbol_name, kind, name, scope);
    }
}
