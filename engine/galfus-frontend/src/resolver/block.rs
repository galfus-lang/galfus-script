use super::*;

impl<'a> Resolver<'a> {
    pub(super) fn resolve_block_scope_item(&mut self, item: NodeId, parent_scope: ScopeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::ExportItem => {
                if let Some(inner) = node.first_child() {
                    self.resolve_block_scope_item(inner, parent_scope);
                }
            }

            SyntaxNodeKind::FunctionItem => {
                self.resolve_function_body_block(item);
            }

            SyntaxNodeKind::VarItem | SyntaxNodeKind::ConstItem => {
                self.resolve_nested_blocks_in(item, parent_scope);
            }

            _ => {}
        }
    }

    fn resolve_function_body_block(&mut self, function: NodeId) {
        let Some(function_scope) = self.resolution.node_scope(function) else {
            return;
        };

        let Some(block) = self
            .syntax
            .first_child_of_kind(function, SyntaxNodeKind::Block)
        else {
            return;
        };

        self.resolve_block(block, function_scope);
    }

    pub(super) fn resolve_block(&mut self, block: NodeId, parent_scope: ScopeId) -> ScopeId {
        let block_scope =
            self.resolution
                .add_scope(ScopeKind::Block, Some(parent_scope), Some(block));

        self.declare_block_statements(block, block_scope);

        block_scope
    }

    fn declare_block_statements(&mut self, block: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(block) else {
            return;
        };

        for statement in node.children() {
            self.declare_block_statement(*statement, scope);
        }
    }

    fn declare_block_statement(&mut self, statement: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(statement) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::VarStatement => {
                self.declare_statement_binding(statement, SymbolKind::Var, scope);
                self.resolve_nested_blocks_in(statement, scope);
            }

            SyntaxNodeKind::ConstStatement => {
                self.declare_statement_binding(statement, SymbolKind::Const, scope);
                self.resolve_nested_blocks_in(statement, scope);
            }

            SyntaxNodeKind::Block => {
                self.resolve_block(statement, scope);
            }

            SyntaxNodeKind::ForStatement => {
                self.resolve_for_statement(statement, scope);
            }

            SyntaxNodeKind::IfStatement
            | SyntaxNodeKind::LoopStatement
            | SyntaxNodeKind::ReturnStatement
            | SyntaxNodeKind::ExpressionStatement
            | SyntaxNodeKind::AssignmentStatement => {
                self.resolve_nested_blocks_in(statement, scope);
            }

            _ => {}
        }
    }

    fn declare_statement_binding(&mut self, statement: NodeId, kind: SymbolKind, scope: ScopeId) {
        let Some(binding) = self
            .syntax
            .first_child_of_kind(statement, SyntaxNodeKind::BindingPattern)
        else {
            return;
        };

        self.declare_binding_pattern(binding, kind, scope);
    }

    fn resolve_nested_blocks_in(&mut self, node: NodeId, parent_scope: ScopeId) {
        let Some(syntax_node) = self.syntax.node(node) else {
            return;
        };

        for child in syntax_node.children() {
            let Some(child_node) = self.syntax.node(*child) else {
                continue;
            };

            match child_node.kind() {
                SyntaxNodeKind::ExpressionFunction | SyntaxNodeKind::BlockFunction => {
                    self.resolve_function_expression(*child, parent_scope);
                }

                SyntaxNodeKind::ForStatement => {
                    self.resolve_for_statement(*child, parent_scope);
                }

                SyntaxNodeKind::Block => {
                    self.resolve_block(*child, parent_scope);
                }

                SyntaxNodeKind::MatchArm => {
                    self.resolve_match_arm(*child, parent_scope);
                }

                SyntaxNodeKind::InstanceofArm => {
                    self.resolve_instanceof_arm(*child, parent_scope);
                }

                _ => {
                    self.resolve_nested_blocks_in(*child, parent_scope);
                }
            }
        }
    }

    fn resolve_function_expression(&mut self, expression: NodeId, parent_scope: ScopeId) {
        let function_scope = self.resolution.add_scope(
            ScopeKind::FunctionExpression,
            Some(parent_scope),
            Some(expression),
        );

        if let Some(parameters) = self
            .syntax
            .first_child_of_kind(expression, SyntaxNodeKind::ParameterList)
        {
            self.resolution.bind_scope(parameters, function_scope);
            self.declare_parameter_list(parameters, function_scope);
        }

        let Some(body) = self
            .syntax
            .node(expression)
            .and_then(|node| node.children().last())
        else {
            return;
        };

        let Some(body_node) = self.syntax.node(*body) else {
            return;
        };

        if body_node.kind() == SyntaxNodeKind::Block {
            self.resolve_block(*body, function_scope);
        } else {
            self.resolve_nested_blocks_in(*body, function_scope);
        }
    }

    fn resolve_for_statement(&mut self, statement: NodeId, parent_scope: ScopeId) {
        let for_scope =
            self.resolution
                .add_scope(ScopeKind::For, Some(parent_scope), Some(statement));

        if let Some(binding) = self
            .syntax
            .first_child_of_kind(statement, SyntaxNodeKind::ForBinding)
        {
            self.declare_for_binding(binding, for_scope);
        }

        if let Some(body) = self
            .syntax
            .first_child_of_kind(statement, SyntaxNodeKind::Block)
        {
            self.resolve_block(body, for_scope);
        }
    }

    fn declare_for_binding(&mut self, binding: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(binding) else {
            return;
        };

        for child in node.children() {
            let Some(child_node) = self.syntax.node(*child) else {
                continue;
            };

            if child_node.kind() == SyntaxNodeKind::BindingPattern {
                self.declare_binding_pattern(*child, SymbolKind::ForBinding, scope);
            } else if child_node.kind() == SyntaxNodeKind::Identifier {
                let symbol_name = self.node_text(*child);
                if symbol_name != "_" {
                    let name_id = self.string_table.intern(&symbol_name);
                    self.declare_symbol(name_id, SymbolKind::ForBinding, *child, scope);
                }
            }
        }
    }

    fn resolve_match_arm(&mut self, arm: NodeId, parent_scope: ScopeId) {
        let arm_scope =
            self.resolution
                .add_scope(ScopeKind::MatchArm, Some(parent_scope), Some(arm));

        if let Some(pattern) = self.syntax.first_child(arm) {
            self.declare_pattern_bindings(pattern, arm_scope);
        }

        let Some(body) = self
            .syntax
            .child(arm, 1)
            .and_then(|body| self.syntax.child(body, 0))
        else {
            return;
        };

        let Some(body_node) = self.syntax.node(body) else {
            return;
        };

        if body_node.kind() == SyntaxNodeKind::Block {
            self.resolve_block(body, arm_scope);
        } else {
            self.resolve_nested_blocks_in(body, arm_scope);
        }
    }

    fn resolve_instanceof_arm(&mut self, arm: NodeId, parent_scope: ScopeId) {
        let arm_scope =
            self.resolution
                .add_scope(ScopeKind::InstanceofArm, Some(parent_scope), Some(arm));

        if let Some(pattern) = self.syntax.first_child(arm) {
            self.declare_instanceof_pattern_bindings(pattern, arm_scope);
        }

        let Some(body) = self
            .syntax
            .child(arm, 1)
            .and_then(|body| self.syntax.child(body, 0))
        else {
            return;
        };

        let Some(body_node) = self.syntax.node(body) else {
            return;
        };

        if body_node.kind() == SyntaxNodeKind::Block {
            self.resolve_block(body, arm_scope);
        } else {
            self.resolve_nested_blocks_in(body, arm_scope);
        }
    }

    fn declare_pattern_bindings(&mut self, pattern: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(pattern) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::BindingPattern => {
                self.declare_match_binding_pattern(pattern, scope);
            }

            SyntaxNodeKind::VariantPatternPayload
            | SyntaxNodeKind::TupleBindingPattern
            | SyntaxNodeKind::ArrayBindingPattern => {
                for child in node.children() {
                    self.declare_pattern_bindings(*child, scope);
                }
            }

            SyntaxNodeKind::VariantPattern => {
                if let Some(payload) = self
                    .syntax
                    .first_child_of_kind(pattern, SyntaxNodeKind::VariantPatternPayload)
                {
                    self.declare_pattern_bindings(payload, scope);
                }
            }

            _ => {}
        }
    }

    fn declare_instanceof_pattern_bindings(&mut self, pattern: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(pattern) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::BindingPattern => {
                self.declare_match_binding_pattern(pattern, scope);
            }

            SyntaxNodeKind::TypePattern => {
                if let Some(binding) = self
                    .syntax
                    .first_child_of_kind(pattern, SyntaxNodeKind::TypePatternBinding)
                {
                    self.declare_type_pattern_binding(binding, scope);
                }
            }

            _ => {}
        }
    }

    fn declare_type_pattern_binding(&mut self, binding: NodeId, scope: ScopeId) {
        let Some(name) = self
            .syntax
            .first_child_of_kind(binding, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        let symbol_name = self.node_text(name);

        if symbol_name != "_" {
            let name_id = self.string_table.intern(&symbol_name);
            self.declare_symbol(name_id, SymbolKind::TypePatternBinding, name, scope);
        }
    }

    fn declare_match_binding_pattern(&mut self, pattern: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(pattern) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::BindingPattern => {
                if let Some(inner) = node.first_child() {
                    self.declare_match_binding_pattern(inner, scope);
                }
            }

            SyntaxNodeKind::Identifier => {
                let symbol_name = self.node_text(pattern);

                if symbol_name != "_" {
                    let name_id = self.string_table.intern(&symbol_name);
                    self.declare_symbol(name_id, SymbolKind::PatternBinding, pattern, scope);
                }
            }

            SyntaxNodeKind::StructBindingPattern
            | SyntaxNodeKind::TupleBindingPattern
            | SyntaxNodeKind::ArrayBindingPattern => {
                for child in node.children() {
                    self.declare_match_binding_pattern(*child, scope);
                }
            }

            SyntaxNodeKind::StructPattern => {
                if node.children().len() > 1 {
                    for field in &node.children()[1..] {
                        self.declare_match_binding_pattern(*field, scope);
                    }
                }
            }

            SyntaxNodeKind::StructBindingField | SyntaxNodeKind::StructPatternField => {
                match node.child_count() {
                    0 => {}

                    1 => {
                        if let Some(name) = node.first_child() {
                            let symbol_name = self.node_text(name);

                            if symbol_name != "_" {
                                let name_id = self.string_table.intern(&symbol_name);
                                self.declare_symbol(
                                    name_id,
                                    SymbolKind::PatternBinding,
                                    name,
                                    scope,
                                );
                            }
                        }
                    }

                    _ => {
                        if let Some(alias_pattern) = node.child(1) {
                            self.declare_match_binding_pattern(alias_pattern, scope);
                        }
                    }
                }
            }

            SyntaxNodeKind::RestBindingPattern => {
                if let Some(inner) = node.first_child() {
                    self.declare_match_binding_pattern(inner, scope);
                }
            }

            _ => {}
        }
    }
}
