use super::*;
use galfus_core::{NodeId, ScopeId};

impl<'a> Resolver<'a> {
    pub(super) fn resolve_reference_item(&mut self, item: NodeId, parent_scope: ScopeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::ExportItem => {
                if let Some(inner) = node.first_child() {
                    self.resolve_reference_item(inner, parent_scope);
                }
            }

            SyntaxNodeKind::FunctionItem => {
                self.resolve_direct_decorator_list(item, parent_scope);
                self.resolve_function_references(item);
            }

            SyntaxNodeKind::StructItem | SyntaxNodeKind::ChoiceItem => {
                let scope = self.resolution.node_scope(item).unwrap_or(parent_scope);
                self.resolve_decorator_references_in_node(item, scope);
            }

            SyntaxNodeKind::VarItem | SyntaxNodeKind::ConstItem => {
                self.resolve_node_references(item, parent_scope);
            }

            _ => {}
        }
    }

    fn resolve_function_references(&mut self, function: NodeId) {
        let Some(function_scope) = self.resolution.node_scope(function) else {
            return;
        };

        self.resolve_function_parameter_defaults(function, function_scope);

        let Some(body) = self
            .syntax
            .node(function)
            .and_then(|node| node.last_child())
        else {
            return;
        };

        let body_scope = self.resolution.node_scope(body).unwrap_or(function_scope);

        self.resolve_node_references(body, body_scope);
    }

    fn resolve_function_parameter_defaults(&mut self, function: NodeId, function_scope: ScopeId) {
        let Some(parameters) = self
            .syntax
            .first_child_of_kind(function, SyntaxNodeKind::ParameterList)
        else {
            return;
        };

        self.resolve_node_references(parameters, function_scope);
    }

    fn resolve_node_references(&mut self, node: NodeId, current_scope: ScopeId) {
        self.resolve_node_references_with_parent(node, current_scope, None);
    }

    fn resolve_node_references_with_parent(
        &mut self,
        node: NodeId,
        current_scope: ScopeId,
        parent_kind: Option<SyntaxNodeKind>,
    ) {
        let Some(syntax_node) = self.syntax.node(node) else {
            return;
        };

        let scope = self.resolution.node_scope(node).unwrap_or(current_scope);

        match syntax_node.kind() {
            SyntaxNodeKind::NameExpression => {
                self.resolve_name_expression(node, scope);
                return;
            }

            SyntaxNodeKind::StructLiteralFieldShorthand => {
                if let Some(ident) = self.syntax.first_child(node) {
                    let symbol_name = self.node_text(ident);

                    if let Some(symbol) = self.lookup_symbol_by_name(scope, &symbol_name) {
                        self.resolution.bind_reference(ident, symbol);
                    } else {
                        self.report_unresolved_name(ident, symbol_name);
                    }
                }
                return;
            }

            SyntaxNodeKind::PathExpression => {
                self.resolve_path_expression(node, scope);
                return;
            }

            SyntaxNodeKind::VariantPattern => {
                self.resolve_variant_pattern(node, scope);
            }

            SyntaxNodeKind::ForStatement => {
                self.resolve_for_statement_references(node, current_scope, scope);
                return;
            }

            SyntaxNodeKind::ExpressionFunction | SyntaxNodeKind::BlockFunction => {
                self.resolve_function_expression_references(node, scope);
                return;
            }

            // Nested functions, if allowed later, should own their own pass.
            SyntaxNodeKind::FunctionItem => {
                return;
            }

            SyntaxNodeKind::WildcardExpression => {
                if parent_kind != Some(SyntaxNodeKind::Argument) {
                    self.diagnostics.push(Diagnostic::error_with_message(
                        ResolverDiagnosticCode::UnresolvedName,
                        "unresolved name `_`".to_string(),
                        syntax_node.span(),
                    ));
                }
                return;
            }

            _ => {}
        }

        for child in syntax_node.children() {
            self.resolve_node_references_with_parent(*child, scope, Some(syntax_node.kind()));
        }
    }

    fn resolve_name_expression(&mut self, expression: NodeId, scope: ScopeId) {
        let Some(name) = self
            .syntax
            .first_child_of_kind(expression, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        let symbol_name = self.node_text(name);

        if let Some(symbol) = self.lookup_symbol_by_name(scope, &symbol_name) {
            self.resolution.bind_reference(expression, symbol);
            return;
        }

        let Some(name_node) = self.syntax.node(name) else {
            return;
        };

        self.diagnostics.push(Diagnostic::error_with_message(
            ResolverDiagnosticCode::UnresolvedName,
            format!("unresolved name `{symbol_name}`"),
            name_node.span(),
        ));
    }

    fn resolve_path_expression(&mut self, expression: NodeId, scope: ScopeId) {
        let Some(target) = self.syntax.first_child(expression) else {
            return;
        };

        self.resolve_node_references(target, scope);

        let mut reference_target = target;
        while self.syntax.node(reference_target).map(|n| n.kind())
            == Some(SyntaxNodeKind::GenericExpression)
        {
            if let Some(first_child) = self.syntax.first_child(reference_target) {
                reference_target = first_child;
            } else {
                break;
            }
        }

        if let Some(symbol) = self.resolution.reference_symbol(reference_target) {
            self.resolution.bind_reference(expression, symbol);
            self.resolve_path_expression_member(expression, symbol);
        }
    }

    fn resolve_path_expression_member(&mut self, expression: NodeId, root_symbol: SymbolId) {
        let Some(member) = self.syntax.child(expression, 1) else {
            return;
        };

        let member_name = self.node_text(member);

        if let Some(symbol) = self.resolve_local_path_member(root_symbol, member_name.as_str()) {
            let kind = self.path_reference_kind_for_symbol(symbol);

            self.resolution
                .bind_path_reference_kind(expression, symbol, kind);

            return;
        }

        if let Some(symbol) = self.resolve_anchor_function_member(root_symbol, member_name.as_str())
        {
            self.resolution.bind_path_reference_kind(
                expression,
                symbol,
                PathReferenceKind::AnchorFunction,
            );

            return;
        }

        if !self.should_report_unresolved_path_member(root_symbol) {
            return;
        }

        self.report_unresolved_path_member(member, member_name);
    }

    fn should_report_unresolved_path_member(&self, root_symbol: SymbolId) -> bool {
        let Some(symbol_data) = self.resolution.symbol(root_symbol) else {
            return false;
        };

        match symbol_data.kind() {
            SymbolKind::Enum | SymbolKind::Choice | SymbolKind::Constraint | SymbolKind::Struct => {
                true
            }

            SymbolKind::ImportNamespace => false,

            _ => false,
        }
    }

    fn resolve_for_statement_references(
        &mut self,
        statement: NodeId,
        parent_scope: ScopeId,
        for_scope: ScopeId,
    ) {
        if let Some(iterable) = self.syntax.child(statement, 1) {
            self.resolve_node_references(iterable, parent_scope);
        }

        if let Some(body) = self.syntax.child(statement, 2) {
            self.resolve_node_references(body, for_scope);
        }
    }

    fn resolve_function_expression_references(
        &mut self,
        expression: NodeId,
        function_scope: ScopeId,
    ) {
        if let Some(parameters) = self
            .syntax
            .first_child_of_kind(expression, SyntaxNodeKind::ParameterList)
        {
            self.resolve_node_references(parameters, function_scope);
        }

        let Some(body) = self
            .syntax
            .node(expression)
            .and_then(|node| node.children().last())
        else {
            return;
        };

        self.resolve_node_references(*body, function_scope);
    }

    fn resolve_direct_decorator_list(&mut self, node: NodeId, scope: ScopeId) {
        let Some(decorators) = self
            .syntax
            .first_child_of_kind(node, SyntaxNodeKind::DecoratorList)
        else {
            return;
        };

        self.resolve_decorator_references_in_node(decorators, scope);
    }

    fn resolve_decorator_references_in_node(&mut self, node: NodeId, current_scope: ScopeId) {
        let Some(syntax_node) = self.syntax.node(node) else {
            return;
        };

        let scope = self.resolution.node_scope(node).unwrap_or(current_scope);

        if syntax_node.kind() == SyntaxNodeKind::Decorator {
            self.resolve_decorator_reference(node, scope);
            return;
        }

        for child in syntax_node.children() {
            self.resolve_decorator_references_in_node(*child, scope);
        }
    }

    fn resolve_decorator_reference(&mut self, decorator: NodeId, scope: ScopeId) {
        let Some(target) = self.syntax.child(decorator, 0) else {
            return;
        };

        self.resolve_node_references(target, scope);
    }

    fn resolve_variant_pattern(&mut self, pattern: NodeId, scope: ScopeId) {
        let Some(root) = self.syntax.child(pattern, 0) else {
            return;
        };

        self.resolve_node_references(root, scope);

        let mut reference_root = root;
        while self.syntax.node(reference_root).map(|n| n.kind())
            == Some(SyntaxNodeKind::GenericExpression)
        {
            if let Some(first_child) = self.syntax.first_child(reference_root) {
                reference_root = first_child;
            } else {
                break;
            }
        }

        let root_symbol = self
            .resolution
            .reference_symbol(reference_root)
            .or_else(|| self.resolution.path_reference_symbol(reference_root))
            .or_else(|| {
                let root_name = self.node_text(reference_root);
                self.lookup_symbol_by_name(scope, &root_name)
            });

        let Some(root_symbol) = root_symbol else {
            let root_name = self.node_text(reference_root);
            self.report_unresolved_name(reference_root, root_name);
            return;
        };

        self.resolution.bind_reference(pattern, root_symbol);

        let Some(variant) = self.syntax.child(pattern, 1) else {
            return;
        };

        self.resolve_variant_pattern_member(pattern, root_symbol, variant);
    }

    fn resolve_variant_pattern_member(
        &mut self,
        pattern: NodeId,
        root_symbol: SymbolId,
        variant: NodeId,
    ) {
        let Some(member_scope) = self.resolution.member_scope(root_symbol) else {
            return;
        };

        let variant_name = self.node_text(variant);

        let Some(symbol) = self.lookup_direct_symbol_by_name(member_scope, &variant_name) else {
            self.report_unresolved_path_member(variant, variant_name);
            return;
        };

        let Some(symbol_data) = self.resolution.symbol(symbol) else {
            return;
        };

        if matches!(
            symbol_data.kind(),
            SymbolKind::EnumVariant | SymbolKind::ChoiceVariant
        ) {
            self.resolution.bind_path_reference(pattern, symbol);
            return;
        }

        self.report_unresolved_path_member(variant, variant_name);
    }

    fn report_unresolved_name(&mut self, name: NodeId, symbol_name: String) {
        let Some(name_node) = self.syntax.node(name) else {
            return;
        };

        self.diagnostics.push(Diagnostic::error_with_message(
            ResolverDiagnosticCode::UnresolvedName,
            format!("unresolved name `{symbol_name}`"),
            name_node.span(),
        ));
    }

    fn report_unresolved_path_member(&mut self, name: NodeId, member_name: String) {
        let Some(name_node) = self.syntax.node(name) else {
            return;
        };

        self.diagnostics.push(Diagnostic::error_with_message(
            ResolverDiagnosticCode::UnresolvedName,
            format!("unresolved path member `{member_name}`"),
            name_node.span(),
        ));
    }

    fn resolve_local_path_member(
        &mut self,
        root_symbol: SymbolId,
        member_name: &str,
    ) -> Option<SymbolId> {
        let member_scope = self.resolution.member_scope(root_symbol)?;

        self.lookup_direct_symbol_by_name(member_scope, member_name)
    }

    fn resolve_anchor_function_member(
        &mut self,
        root_symbol: SymbolId,
        member_name: &str,
    ) -> Option<SymbolId> {
        let root_symbol_data = self.resolution.symbol(root_symbol)?;

        if root_symbol_data.kind() != SymbolKind::Struct {
            return None;
        }

        let anchored_name = format!(
            "{}::{member_name}",
            self.string_table
                .resolve(root_symbol_data.name())
                .unwrap_or("")
        );
        let symbol = self.lookup_symbol_by_name(self.resolution.module_scope(), &anchored_name)?;

        let symbol_data = self.resolution.symbol(symbol)?;

        if symbol_data.kind() != SymbolKind::Function {
            return None;
        }

        Some(symbol)
    }

    fn path_reference_kind_for_symbol(&self, symbol: SymbolId) -> PathReferenceKind {
        let Some(symbol_data) = self.resolution.symbol(symbol) else {
            return PathReferenceKind::LocalMember;
        };

        match symbol_data.kind() {
            SymbolKind::EnumVariant => PathReferenceKind::EnumVariant,
            SymbolKind::ChoiceVariant => PathReferenceKind::ChoiceVariant,
            SymbolKind::ConstraintField | SymbolKind::ConstraintFunction => {
                PathReferenceKind::ConstraintMember
            }
            _ => PathReferenceKind::LocalMember,
        }
    }
}
