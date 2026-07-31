use super::*;
use galfus_core::{NodeId, ScopeId};

impl<'a> Resolver<'a> {
    pub(super) fn resolve_generic_parameter_scope_item(
        &mut self,
        item: NodeId,
        module_scope: ScopeId,
    ) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::ExportItem => {
                if let Some(inner) = node.first_child() {
                    self.resolve_generic_parameter_scope_item(inner, module_scope);
                }
            }

            SyntaxNodeKind::FunctionItem => {
                let scope = self.resolution.node_scope(item).unwrap_or(module_scope);

                self.declare_generic_parameters_in(item, scope);
            }

            SyntaxNodeKind::TypeAliasItem => {
                let scope = self.ensure_item_scope(item, ScopeKind::TypeAlias, module_scope);

                self.declare_generic_parameters_in(item, scope);
            }

            SyntaxNodeKind::StructItem => {
                let scope = self.ensure_item_scope(item, ScopeKind::Struct, module_scope);

                self.declare_generic_parameters_in(item, scope);
            }

            SyntaxNodeKind::EnumItem => {
                let scope = self.ensure_item_scope(item, ScopeKind::Enum, module_scope);

                self.declare_generic_parameters_in(item, scope);
            }

            SyntaxNodeKind::ChoiceItem => {
                let scope = self.ensure_item_scope(item, ScopeKind::Choice, module_scope);

                self.declare_generic_parameters_in(item, scope);
            }

            SyntaxNodeKind::ConstraintItem => {
                let scope = self.ensure_item_scope(item, ScopeKind::Constraint, module_scope);

                self.declare_generic_parameters_in(item, scope);
            }

            _ => {}
        }
    }

    fn ensure_item_scope(
        &mut self,
        item: NodeId,
        kind: ScopeKind,
        parent_scope: ScopeId,
    ) -> ScopeId {
        if let Some(scope) = self.resolution.node_scope(item) {
            return scope;
        }

        self.resolution
            .add_scope(kind, Some(parent_scope), Some(item))
    }

    fn declare_generic_parameters_in(&mut self, item: NodeId, scope: ScopeId) {
        let Some(generic_parameters) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::GenericParameterList)
        else {
            return;
        };

        let Some(node) = self.syntax.node(generic_parameters) else {
            return;
        };

        self.resolution.bind_scope(generic_parameters, scope);

        for parameter in node.children() {
            self.declare_generic_parameter(*parameter, scope);
        }
    }

    fn declare_generic_parameter(&mut self, parameter: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(parameter) else {
            return;
        };

        if node.kind() != SyntaxNodeKind::GenericParameter {
            return;
        }

        let Some(name) = self
            .syntax
            .first_child_of_kind(parameter, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        let symbol_name = self.node_text(name);

        self.declare_symbol(symbol_name, SymbolKind::GenericParameter, name, scope);
    }
}
