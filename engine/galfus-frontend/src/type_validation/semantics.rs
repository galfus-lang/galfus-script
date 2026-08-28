use super::DeclarationTypeChecker;
use crate::ScopeKind;
use crate::builtin_constraints::is_builtin_constraint;
use crate::{PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::{HashMap, HashSet};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_semantic_rules(&mut self, root: NodeId) {
        self.check_return_context(root, false);
        self.check_function_return_paths(root);
        self.check_unreachable_statements(root);
        self.check_binding_initialization_cycles(root);
        self.check_struct_expansion_targets(root);
        self.check_struct_literal_spread_targets(root);
        self.check_builtin_constraint_imports(root);
        self.check_builtin_symbol_visibility(root);
        self.check_bridge_and_internal_symbols(root);
        self.check_proxy_module_items(root);
    }

    fn check_return_context(&mut self, node: NodeId, inside_function: bool) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::FunctionItem
            | SyntaxNodeKind::ExpressionFunction
            | SyntaxNodeKind::BlockFunction => {
                for child in syntax_node.children() {
                    self.check_return_context(*child, true);
                }

                return;
            }

            SyntaxNodeKind::ReturnStatement if !inside_function => {
                self.report_return_outside_function(node);
            }

            _ => {}
        }

        for child in syntax_node.children() {
            self.check_return_context(*child, inside_function);
        }
    }

    fn check_function_return_paths(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if matches!(
            syntax_node.kind(),
            SyntaxNodeKind::FunctionItem | SyntaxNodeKind::BlockFunction
        ) {
            self.check_single_function_return_path(node);
        }

        for child in syntax_node.children() {
            self.check_function_return_paths(*child);
        }
    }

    fn check_single_function_return_path(&mut self, function: NodeId) {
        let Some(return_type) = self
            .last_direct_type_child(function)
            .and_then(|node| self.layer.node_type(node))
        else {
            return;
        };

        if self.is_null_type(return_type) || self.is_semantic_error_type(return_type) {
            return;
        }

        if !self
            .generic_parameter_symbols_from_type(return_type)
            .is_empty()
        {
            return;
        }

        let Some(body) = self
            .graph
            .syntax()
            .first_child_of_kind(function, SyntaxNodeKind::Block)
        else {
            return;
        };

        if self.statement_list_guarantees_return(body) {
            return;
        }

        self.report_missing_return(function, return_type);
    }

    fn statement_list_guarantees_return(&self, block: NodeId) -> bool {
        let Some(block_node) = self.graph.syntax().node(block) else {
            return false;
        };

        block_node
            .children()
            .iter()
            .any(|statement| self.statement_guarantees_return(*statement))
    }

    fn statement_guarantees_return(&self, statement: NodeId) -> bool {
        let Some(statement_node) = self.graph.syntax().node(statement) else {
            return false;
        };

        match statement_node.kind() {
            SyntaxNodeKind::ReturnStatement => true,

            SyntaxNodeKind::Block => self.statement_list_guarantees_return(statement),

            SyntaxNodeKind::IfStatement => self.if_statement_guarantees_return(statement),

            SyntaxNodeKind::LoopStatement => self
                .graph
                .syntax()
                .child(statement, 0)
                .is_some_and(|body| self.statement_list_guarantees_return(body)),

            _ => false,
        }
    }

    fn if_statement_guarantees_return(&self, statement: NodeId) -> bool {
        let Some(then_block) = self.graph.syntax().child(statement, 1) else {
            return false;
        };

        if !self.statement_list_guarantees_return(then_block) {
            return false;
        }

        let Some(else_clause) = self
            .graph
            .syntax()
            .first_child_of_kind(statement, SyntaxNodeKind::ElseClause)
        else {
            return false;
        };

        let Some(else_body) = self.graph.syntax().child(else_clause, 0) else {
            return false;
        };

        self.statement_guarantees_return(else_body)
    }

    fn check_unreachable_statements(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::Block {
            self.check_unreachable_block_statements(node);
        }

        for child in syntax_node.children() {
            self.check_unreachable_statements(*child);
        }
    }

    fn check_unreachable_block_statements(&mut self, block: NodeId) {
        let Some(block_node) = self.graph.syntax().node(block) else {
            return;
        };

        let mut unreachable = false;

        for statement in block_node.children().iter().copied() {
            if unreachable {
                self.report_unreachable_code(statement);
                continue;
            }

            if self.statement_guarantees_return(statement) {
                unreachable = true;
            }
        }
    }

    fn check_binding_initialization_cycles(&mut self, root: NodeId) {
        let bindings = self.binding_initialization_graph(root);
        let mut reported = HashSet::new();

        for symbol in bindings.keys().copied().collect::<Vec<_>>() {
            let mut visiting = Vec::new();
            self.check_binding_cycle_from(symbol, &bindings, &mut visiting, &mut reported);
        }
    }

    fn check_binding_cycle_from(
        &mut self,
        symbol: SymbolId,
        bindings: &HashMap<SymbolId, BindingInitializer>,
        visiting: &mut Vec<SymbolId>,
        reported: &mut HashSet<SymbolId>,
    ) {
        if reported.contains(&symbol) {
            return;
        }

        if let Some(cycle_start) = visiting.iter().position(|candidate| *candidate == symbol) {
            for cycle_symbol in visiting[cycle_start..].iter().copied() {
                self.report_binding_initialization_cycle(cycle_symbol, bindings, reported);
            }

            return;
        }

        let Some(binding) = bindings.get(&symbol) else {
            return;
        };

        visiting.push(symbol);

        for dependency in binding.dependencies.iter().copied() {
            if bindings.contains_key(&dependency) {
                self.check_binding_cycle_from(dependency, bindings, visiting, reported);
            }
        }

        visiting.pop();
    }

    fn report_binding_initialization_cycle(
        &mut self,
        symbol: SymbolId,
        bindings: &HashMap<SymbolId, BindingInitializer>,
        reported: &mut HashSet<SymbolId>,
    ) {
        if !reported.insert(symbol) {
            return;
        }

        let Some(binding) = bindings.get(&symbol) else {
            return;
        };

        let name = self
            .graph
            .resolution()
            .and_then(|resolution| resolution.symbol(symbol))
            .map(|symbol| {
                self.string_table
                    .resolve(symbol.name())
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_else(|| "<unknown>".to_string());

        self.report_initialization_cycle(binding.node, name.as_str());
    }

    fn binding_initialization_graph(&self, root: NodeId) -> HashMap<SymbolId, BindingInitializer> {
        let mut bindings = HashMap::new();
        self.collect_binding_initializers(root, &mut bindings);
        bindings
    }

    fn collect_binding_initializers(
        &self,
        node: NodeId,
        bindings: &mut HashMap<SymbolId, BindingInitializer>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if matches!(
            syntax_node.kind(),
            SyntaxNodeKind::VarItem
                | SyntaxNodeKind::ConstItem
                | SyntaxNodeKind::VarStatement
                | SyntaxNodeKind::ConstStatement
        ) && let Some(binding) = self.binding_initializer(node)
        {
            for symbol in binding.symbols.iter().copied() {
                bindings.insert(
                    symbol,
                    BindingInitializer {
                        node,
                        dependencies: binding.dependencies.clone(),
                    },
                );
            }
        }

        for child in syntax_node.children() {
            self.collect_binding_initializers(*child, bindings);
        }
    }

    fn binding_initializer(&self, node: NodeId) -> Option<CollectedBindingInitializer> {
        let initializer = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::Initializer)?;
        let expression = self.graph.syntax().child(initializer, 0)?;

        let mut symbols = Vec::new();
        let target_kinds = &[
            SymbolKind::Var,
            SymbolKind::Const,
            SymbolKind::PatternBinding,
            SymbolKind::TypePatternBinding,
        ];

        if let Some(pattern) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::BindingPattern)
        {
            symbols = self.declaration_symbols_in_node(pattern, target_kinds);
        } else if let Some(identifier) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::Identifier)
        {
            symbols = self.declaration_symbols_in_node(identifier, target_kinds);
        }

        if symbols.is_empty() {
            return None;
        }

        let mut dependencies = HashSet::new();
        self.collect_reference_dependencies(expression, &mut dependencies);

        Some(CollectedBindingInitializer {
            symbols,
            dependencies,
        })
    }

    fn collect_reference_dependencies(&self, node: NodeId, dependencies: &mut HashSet<SymbolId>) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if matches!(
            syntax_node.kind(),
            SyntaxNodeKind::NameExpression | SyntaxNodeKind::Identifier
        ) && let Some(symbol) = self
            .graph
            .resolution()
            .and_then(|resolution| resolution.reference_symbol(node))
        {
            dependencies.insert(symbol);
        }

        for child in syntax_node.children() {
            self.collect_reference_dependencies(*child, dependencies);
        }
    }

    fn check_struct_expansion_targets(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::StructExpansion {
            self.check_struct_expansion_target(node);
        }

        for child in syntax_node.children() {
            self.check_struct_expansion_targets(*child);
        }
    }

    fn check_struct_expansion_target(&mut self, expansion: NodeId) {
        let Some(target) = self.graph.syntax().child(expansion, 0) else {
            return;
        };

        let Some(ty) = self.layer.node_type(target) else {
            return;
        };

        if self.is_struct_type(ty) || self.is_semantic_error_type(ty) {
            return;
        }

        self.report_invalid_struct_expansion_target(expansion, ty);
    }

    fn check_struct_literal_spread_targets(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::SpreadStructLiteralField {
            self.check_struct_literal_spread_target(node);
        }

        for child in syntax_node.children() {
            self.check_struct_literal_spread_targets(*child);
        }
    }

    fn check_struct_literal_spread_target(&mut self, spread: NodeId) {
        let Some(expression) = self.graph.syntax().child(spread, 0) else {
            return;
        };

        let Some(ty) = self.infer_expression_type(expression) else {
            return;
        };

        if self.is_struct_type(ty) || self.is_semantic_error_type(ty) {
            return;
        }

        self.report_invalid_struct_spread_target(spread, ty);
    }

    fn is_struct_type(&self, ty: TypeId) -> bool {
        let resolved = self.resolve_alias_type(ty);

        let Some(TypeKind::Named { symbol }) = self.layer.table().kind(resolved) else {
            return false;
        };

        self.graph
            .resolution()
            .and_then(|resolution| resolution.symbol(*symbol))
            .is_some_and(|symbol| symbol.kind() == SymbolKind::Struct)
    }

    pub(super) fn is_null_type(&self, ty: TypeId) -> bool {
        matches!(
            self.layer.table().kind(self.resolve_alias_type(ty)),
            Some(TypeKind::Primitive(PrimitiveType::Null)) | Some(TypeKind::Error)
        )
    }

    fn is_semantic_error_type(&self, ty: TypeId) -> bool {
        matches!(self.layer.table().kind(ty), Some(TypeKind::Error))
    }

    fn check_builtin_symbol_visibility(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        let is_builtin_module = self.source.name() == "std/io"
            || self.source.name().starts_with("std/")
            || self.source.name() == "std/fs"; // Add common namespaces if needed

        if !is_builtin_module && syntax_node.kind() == SyntaxNodeKind::Identifier {
            let name = self.node_text(node);
            if name.starts_with("__builtin_")
                || name.starts_with("__provider_")
                || name.starts_with("__internal_")
            {
                self.report_restricted_builtin_symbol(node, &name);
            }
        }

        for child in syntax_node.children() {
            self.check_builtin_symbol_visibility(*child);
        }
    }

    fn check_builtin_constraint_imports(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if self.is_type_node_kind(syntax_node.kind())
            && let Some(symbol) = self
                .graph
                .resolution()
                .and_then(|resolution| resolution.type_reference_symbol(node))
            && self.is_direct_builtin_constraint_symbol(symbol)
        {
            let diagnostic_node = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .unwrap_or(node);
            let name = self.node_text(diagnostic_node);
            self.report_restricted_builtin_symbol(diagnostic_node, name.as_str());
        }

        for child in syntax_node.children() {
            self.check_builtin_constraint_imports(*child);
        }
    }

    fn is_direct_builtin_constraint_symbol(&self, symbol: SymbolId) -> bool {
        let Some(resolution) = self.graph.resolution() else {
            return false;
        };

        let Some(symbol_data) = resolution.symbol(symbol) else {
            return false;
        };

        if symbol_data.kind() != SymbolKind::Constraint {
            return false;
        }

        if !is_builtin_constraint(self.string_table.resolve(symbol_data.name()).unwrap_or("")) {
            return false;
        }

        resolution
            .scope(symbol_data.scope())
            .is_some_and(|scope| scope.kind() == ScopeKind::Builtin)
    }
}

#[derive(Debug, Clone)]
struct BindingInitializer {
    node: NodeId,
    dependencies: HashSet<SymbolId>,
}

#[derive(Debug, Clone)]
struct CollectedBindingInitializer {
    symbols: Vec<SymbolId>,
    dependencies: HashSet<SymbolId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataOwner {
    Function,
    Enum,
    Loop,
    For,

    Other,
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_keyword_metadata(&mut self, root: NodeId) {
        self.check_keyword_metadata_in_node(root);
    }

    fn check_keyword_metadata_in_node(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        let kind = syntax_node.kind();
        if let Some(metadata_list_node) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::KeywordMetadataList)
        {
            let owner = match kind {
                SyntaxNodeKind::FunctionItem
                | SyntaxNodeKind::ExpressionFunction
                | SyntaxNodeKind::BlockFunction => MetadataOwner::Function,
                SyntaxNodeKind::EnumItem => MetadataOwner::Enum,
                SyntaxNodeKind::LoopStatement => MetadataOwner::Loop,
                SyntaxNodeKind::ForStatement => MetadataOwner::For,

                _ => MetadataOwner::Other,
            };

            self.validate_keyword_metadata_list(metadata_list_node, owner);
        }

        let children = syntax_node.children().to_vec();
        for child in children {
            self.check_keyword_metadata_in_node(child);
        }
    }

    fn validate_keyword_metadata_list(&mut self, list_node: NodeId, owner: MetadataOwner) {
        let Some(list) = self.graph.syntax().node(list_node) else {
            return;
        };

        for child in list.children() {
            let Some(child_node) = self.graph.syntax().node(*child) else {
                continue;
            };

            match child_node.kind() {
                SyntaxNodeKind::KeywordMetadataFlag => {
                    let flag_ident = self.graph.syntax().child(*child, 0);
                    let flag_text = flag_ident.map(|id| self.node_text(id)).unwrap_or_default();

                    let is_valid = match owner {
                        MetadataOwner::Function => flag_text == "stamp" || flag_text == "async",

                        _ => false,
                    };

                    if !is_valid {
                        let owner_name = match owner {
                            MetadataOwner::Function => "function",
                            MetadataOwner::Enum => "enum",
                            MetadataOwner::Loop => "loop",
                            MetadataOwner::For => "for",

                            MetadataOwner::Other => "construct",
                        };
                        self.report_invalid_keyword_metadata(
                            *child,
                            format!("invalid metadata {} for {}", flag_text, owner_name),
                        );
                    }
                }

                SyntaxNodeKind::KeywordMetadataPair => {
                    let key_ident = child_node.first_child();
                    let key_text = key_ident.map(|id| self.node_text(id)).unwrap_or_default();

                    let is_valid = match owner {
                        MetadataOwner::Loop | MetadataOwner::For => key_text == "name",

                        _ => false,
                    };

                    if !is_valid {
                        let owner_name = match owner {
                            MetadataOwner::Function => "function",
                            MetadataOwner::Enum => "enum",
                            MetadataOwner::Loop => "loop",
                            MetadataOwner::For => "for",

                            MetadataOwner::Other => "construct",
                        };
                        self.report_invalid_keyword_metadata(
                            *child,
                            format!("invalid metadata {} for {}", key_text, owner_name),
                        );
                    }
                }

                SyntaxNodeKind::KeywordMetadataType => {
                    let is_valid = match owner {
                        MetadataOwner::Enum => {
                            if let Some(type_node) = child_node.first_child() {
                                if let Some(ty) = self.layer.node_type(type_node) {
                                    self.is_integer_type(ty)
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };

                    if !is_valid {
                        let owner_name = match owner {
                            MetadataOwner::Function => "function",
                            MetadataOwner::Enum => "enum",
                            MetadataOwner::Loop => "loop",
                            MetadataOwner::For => "for",

                            MetadataOwner::Other => "construct",
                        };
                        self.report_invalid_keyword_metadata(
                            *child,
                            format!("invalid metadata type for {}", owner_name),
                        );
                    }
                }

                _ => {}
            }
        }
    }

    fn check_bridge_and_internal_symbols(&mut self, root: NodeId) {
        let path = self.source.name();
        let name = path.strip_suffix(".gfs").unwrap_or(path);
        let is_internal_module = galfus_contract::is_internal_module(name);
        let is_bridge_module = is_internal_module || self.is_provider_module;

        self.check_node_bridge_and_internal_rules(root, is_internal_module, is_bridge_module);
    }

    fn check_node_bridge_and_internal_rules(
        &mut self,
        node: NodeId,
        is_internal_module: bool,
        is_bridge_module: bool,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem
            && let Some(ident) = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
        {
            let name = self.node_text(ident);
            let is_async = self
                .graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::KeywordMetadataList)
                .and_then(|meta| {
                    self.graph
                        .syntax()
                        .first_child_of_kind(meta, SyntaxNodeKind::KeywordMetadataFlag)
                })
                .and_then(|flag| {
                    self.graph
                        .syntax()
                        .first_child_of_kind(flag, SyntaxNodeKind::Identifier)
                })
                .map(|flag_ident| self.node_text(flag_ident))
                .map(|text| text == "async")
                .unwrap_or(false);

            if name.starts_with("__internal_") {
                if !is_internal_module {
                    self.report_cannot_infer_type(
                        node,
                        "__internal_* functions are reserved for internal core modules".to_string(),
                    );
                } else if !is_async
                    && !name.starts_with("__internal_math_")
                    && !matches!(
                        name.as_str(),
                        "__internal_thread_get"
                            | "__internal_thread_is_running"
                            | "__internal_thread_is_exited"
                            | "__internal_thread_exit_reason"
                            | "__internal_thread_send"
                            | "__internal_thread_has_messages"
                            | "__internal_thread_get_message"
                            | "__internal_thread_try_receive"
                    )
                {
                    self.report_cannot_infer_type(
                        node,
                        format!(
                            "internal function '{name}' must be explicitly marked with 'fn(async)'"
                        ),
                    );
                }
            } else if name.starts_with("__provider_") {
                if !is_bridge_module {
                    self.report_cannot_infer_type(
                        node,
                        "__provider_* functions are reserved for bridge modules".to_string(),
                    );
                }

                if !is_async {
                    self.report_cannot_infer_type(
                        node,
                        format!(
                            "provider function '{name}' must be explicitly marked with 'fn(async)'"
                        ),
                    );
                }
            }
        }

        let children = syntax_node.children().to_vec();
        for child in children {
            self.check_node_bridge_and_internal_rules(child, is_internal_module, is_bridge_module);
        }
    }

    fn check_proxy_module_items(&mut self, root: NodeId) {
        if !self.source.name().ends_with(".gfp") {
            return;
        }

        let Some(syntax_node) = self.graph.syntax().node(root) else {
            return;
        };

        for child in syntax_node.children() {
            let mut current_item = *child;

            if let Some(node) = self.graph.syntax().node(current_item)
                && node.kind() == SyntaxNodeKind::ExportItem
                && let Some(inner) = node.first_child()
            {
                current_item = inner;
            }

            let Some(item_node) = self.graph.syntax().node(current_item) else {
                continue;
            };

            match item_node.kind() {
                SyntaxNodeKind::StructItem
                | SyntaxNodeKind::TypeAliasItem
                | SyntaxNodeKind::ImportItem => {
                    // Allowed
                }
                SyntaxNodeKind::FunctionItem => {
                    // Allowed only if it does not have a Block body
                    if self
                        .graph
                        .syntax()
                        .first_child_of_kind(current_item, SyntaxNodeKind::Block)
                        .is_some()
                    {
                        self.report_proxy_module_invalid_item(current_item);
                    }
                }
                SyntaxNodeKind::ExportItem => {
                    // In case of empty export or double export
                }
                _ => {
                    self.report_proxy_module_invalid_item(current_item);
                }
            }
        }
    }
}
