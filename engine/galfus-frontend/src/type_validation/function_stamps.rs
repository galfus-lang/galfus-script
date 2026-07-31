use super::DeclarationTypeChecker;
use crate::{SymbolKind, SyntaxNodeKind};
use galfus_core::{NodeId, SymbolId};
use std::collections::{HashMap, HashSet};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_function_stamps(&mut self, root: NodeId) {
        let stamp_functions = self.collect_stamp_functions(root);

        if stamp_functions.is_empty() {
            return;
        }

        let stamp_symbols = stamp_functions.keys().copied().collect::<HashSet<_>>();
        let mut dependencies = HashMap::new();

        for (&symbol, &function) in stamp_functions.iter() {
            let mut calls = HashSet::new();
            self.collect_called_stamp_symbols(function, &stamp_symbols, &mut calls);
            dependencies.insert(symbol, calls);
        }

        for symbol in stamp_functions.keys().copied() {
            let mut path = Vec::new();
            let mut visited = HashSet::new();
            self.check_stamp_cycles_from(
                symbol,
                symbol,
                &dependencies,
                &stamp_functions,
                &mut path,
                &mut visited,
            );
        }
    }

    fn collect_stamp_functions(&self, root: NodeId) -> HashMap<SymbolId, NodeId> {
        let mut stamps = HashMap::new();
        self.collect_stamp_functions_in_node(root, &mut stamps);
        stamps
    }

    fn collect_stamp_functions_in_node(
        &self,
        node: NodeId,
        stamps: &mut HashMap<SymbolId, NodeId>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem
            && self.is_function_stamped(node)
            && let Some(symbol) = self.direct_identifier_symbol(node, SymbolKind::Function)
        {
            stamps.insert(symbol, node);
        }

        for child in syntax_node.children() {
            self.collect_stamp_functions_in_node(*child, stamps);
        }
    }

    pub(super) fn is_function_stamped(&self, function: NodeId) -> bool {
        let Some(metadata_list_node) = self
            .graph
            .syntax()
            .first_child_of_kind(function, SyntaxNodeKind::KeywordMetadataList)
        else {
            return false;
        };

        let Some(metadata_list) = self.graph.syntax().node(metadata_list_node) else {
            return false;
        };

        for child in metadata_list.children() {
            if let Some(child_node) = self.graph.syntax().node(*child)
                && child_node.kind() == SyntaxNodeKind::KeywordMetadataFlag
                && let Some(flag_ident) = self.graph.syntax().child(*child, 0)
                && self.node_text(flag_ident) == "stamp"
            {
                return true;
            }
        }
        false
    }

    fn collect_called_stamp_symbols(
        &self,
        node: NodeId,
        stamp_symbols: &HashSet<SymbolId>,
        calls: &mut HashSet<SymbolId>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::CallExpression
            && let Some(symbol) = self.call_target_symbol(node)
            && stamp_symbols.contains(&symbol)
        {
            calls.insert(symbol);
        }

        for child in syntax_node.children() {
            self.collect_called_stamp_symbols(*child, stamp_symbols, calls);
        }
    }

    fn call_target_symbol(&self, call: NodeId) -> Option<SymbolId> {
        let target = self.graph.syntax().child(call, 0)?;
        self.stamp_expression_reference_symbol(target)
    }

    fn stamp_expression_reference_symbol(&self, expression: NodeId) -> Option<SymbolId> {
        let syntax = self.graph.syntax();
        let resolution = self.graph.resolution()?;
        let syntax_node = syntax.node(expression)?;

        match syntax_node.kind() {
            SyntaxNodeKind::NameExpression => resolution.reference_symbol(expression),
            SyntaxNodeKind::PathExpression => resolution.path_reference_symbol(expression),
            SyntaxNodeKind::GenericExpression => syntax
                .child(expression, 0)
                .and_then(|target| self.stamp_expression_reference_symbol(target)),
            _ => None,
        }
    }

    fn check_stamp_cycles_from(
        &mut self,
        start: SymbolId,
        current: SymbolId,
        dependencies: &HashMap<SymbolId, HashSet<SymbolId>>,
        stamp_functions: &HashMap<SymbolId, NodeId>,
        path: &mut Vec<SymbolId>,
        visited: &mut HashSet<SymbolId>,
    ) {
        if !visited.insert(current) {
            return;
        }

        path.push(current);

        if let Some(calls) = dependencies.get(&current) {
            for called in calls {
                if *called == start {
                    self.report_recursive_function_stamp(start, path.as_slice(), stamp_functions);
                    continue;
                }

                self.check_stamp_cycles_from(
                    start,
                    *called,
                    dependencies,
                    stamp_functions,
                    path,
                    visited,
                );
            }
        }

        path.pop();
        visited.remove(&current);
    }
}
