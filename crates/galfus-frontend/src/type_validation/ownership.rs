use super::{
    AnchorKind, AnchorMetadata, CaptureMetadata, DeclarationTypeChecker, EdgeMetadata,
    TemporaryMetadata, WeakFieldMetadata, WeakObserverMetadata,
};
use crate::{SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::HashSet;

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_ownership_metadata(&mut self, root: NodeId) {
        let mut seen_anchors = HashSet::new();
        let mut seen_edges = HashSet::new();
        let mut seen_captures = HashSet::new();
        let mut seen_temporaries = HashSet::new();

        self.collect_ownership_metadata(
            root,
            None,
            &mut seen_anchors,
            &mut seen_edges,
            &mut seen_captures,
            &mut seen_temporaries,
        );
        self.validate_ownership_cycles();
        self.validate_release_eligibility();
    }

    fn collect_ownership_metadata(
        &mut self,
        node: NodeId,
        owner_struct: Option<SymbolId>,
        seen_anchors: &mut HashSet<(AnchorKind, NodeId, Option<SymbolId>)>,
        seen_edges: &mut HashSet<(SymbolId, SymbolId)>,
        seen_captures: &mut HashSet<(NodeId, SymbolId)>,
        seen_temporaries: &mut HashSet<NodeId>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        let owner_struct = if syntax_node.kind() == SyntaxNodeKind::StructItem {
            self.direct_identifier_symbol(node, SymbolKind::Struct)
                .or(owner_struct)
        } else {
            owner_struct
        };

        self.collect_anchor_metadata(node, seen_anchors);
        self.collect_temporary_metadata(node, seen_anchors, seen_temporaries);

        match syntax_node.kind() {
            SyntaxNodeKind::StructField => {
                self.collect_struct_edge_metadata(node, owner_struct, seen_edges);
            }

            SyntaxNodeKind::WeakStructField => {
                self.check_weak_struct_field(node, owner_struct, seen_edges);
            }

            SyntaxNodeKind::ExpressionFunction | SyntaxNodeKind::BlockFunction => {
                self.collect_closure_capture_metadata(node, seen_captures);
            }

            SyntaxNodeKind::FunctionItem => {
                self.collect_function_anchor_metadata(node, seen_anchors);
            }

            _ => {}
        }

        let children = syntax_node.children().to_vec();

        for child in children {
            self.collect_ownership_metadata(
                child,
                owner_struct,
                seen_anchors,
                seen_edges,
                seen_captures,
                seen_temporaries,
            );
        }
    }

    fn collect_anchor_metadata(
        &mut self,
        node: NodeId,
        seen_anchors: &mut HashSet<(AnchorKind, NodeId, Option<SymbolId>)>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        let (kind, symbol_kinds) = match syntax_node.kind() {
            SyntaxNodeKind::VarItem | SyntaxNodeKind::ConstItem => (
                AnchorKind::ModuleState,
                &[SymbolKind::Var, SymbolKind::Const][..],
            ),

            SyntaxNodeKind::VarStatement | SyntaxNodeKind::ConstStatement => (
                AnchorKind::BlockLocal,
                &[SymbolKind::Var, SymbolKind::Const][..],
            ),

            SyntaxNodeKind::Parameter | SyntaxNodeKind::RestParameter => (
                AnchorKind::FunctionParameter,
                &[SymbolKind::Parameter, SymbolKind::RestParameter][..],
            ),

            SyntaxNodeKind::ForBinding => (AnchorKind::BlockLocal, &[SymbolKind::ForBinding][..]),

            SyntaxNodeKind::ExpressionFunction | SyntaxNodeKind::BlockFunction => {
                (AnchorKind::Closure, &[][..])
            }

            _ => return,
        };

        if symbol_kinds.is_empty() {
            self.push_anchor_metadata(kind, node, None, self.layer.node_type(node), seen_anchors);
            return;
        }

        for symbol in self.declaration_symbols_in_node(node, symbol_kinds) {
            let ty = self.layer.symbol_type(symbol);
            self.push_anchor_metadata(kind, node, Some(symbol), ty, seen_anchors);
        }
    }

    fn collect_function_anchor_metadata(
        &mut self,
        function: NodeId,
        seen_anchors: &mut HashSet<(AnchorKind, NodeId, Option<SymbolId>)>,
    ) {
        let Some(anchor) = self
            .graph
            .syntax()
            .first_child_of_kind(function, SyntaxNodeKind::FunctionAnchor)
        else {
            return;
        };

        let function_symbol = self.direct_identifier_symbol(function, SymbolKind::Function);
        let anchor_type = self
            .graph
            .syntax()
            .first_child(anchor)
            .and_then(|ty| self.layer.node_type(ty));

        self.push_anchor_metadata(
            AnchorKind::FunctionAnchor,
            anchor,
            function_symbol,
            anchor_type,
            seen_anchors,
        );
    }

    fn push_anchor_metadata(
        &mut self,
        kind: AnchorKind,
        node: NodeId,
        symbol: Option<SymbolId>,
        ty: Option<TypeId>,
        seen_anchors: &mut HashSet<(AnchorKind, NodeId, Option<SymbolId>)>,
    ) {
        if !seen_anchors.insert((kind, node, symbol)) {
            return;
        }

        self.ownership_metadata
            .anchors
            .push(AnchorMetadata::new(kind, node, symbol, ty));
    }

    fn collect_struct_edge_metadata(
        &mut self,
        field: NodeId,
        owner_struct: Option<SymbolId>,
        seen_edges: &mut HashSet<(SymbolId, SymbolId)>,
    ) {
        let Some(owner_struct) = owner_struct else {
            return;
        };

        let Some(field_symbol) = self.direct_identifier_symbol(field, SymbolKind::StructField)
        else {
            return;
        };

        let Some(field_type) = self.layer.symbol_type(field_symbol) else {
            return;
        };

        if !self.is_owned_edge_type(field_type) {
            return;
        }

        if !seen_edges.insert((owner_struct, field_symbol)) {
            return;
        }

        self.ownership_metadata.edges.push(EdgeMetadata::new(
            owner_struct,
            field_symbol,
            field,
            field_type,
        ));
    }

    fn check_weak_struct_field(
        &mut self,
        field: NodeId,
        owner_struct: Option<SymbolId>,
        seen_edges: &mut HashSet<(SymbolId, SymbolId)>,
    ) {
        let Some(owner_struct) = owner_struct else {
            return;
        };

        let Some(field_symbol) = self.direct_identifier_symbol(field, SymbolKind::StructField)
        else {
            return;
        };

        let Some(field_type) = self.layer.symbol_type(field_symbol) else {
            return;
        };

        if !self.is_weak_field_nullable_type(field_type) {
            self.report_invalid_weak_field_type(field, field_type);
        }

        self.ownership_metadata
            .weak_fields
            .push(WeakFieldMetadata::new(
                owner_struct,
                field_symbol,
                field,
                field_type,
            ));

        if !seen_edges.insert((owner_struct, field_symbol)) {
            return;
        }

        self.ownership_metadata
            .weak_observers
            .push(WeakObserverMetadata::new(
                owner_struct,
                field_symbol,
                field,
                field_type,
            ));
    }

    fn collect_closure_capture_metadata(
        &mut self,
        closure: NodeId,
        seen_captures: &mut HashSet<(NodeId, SymbolId)>,
    ) {
        let local_symbols = self.local_symbols_in_closure(closure);
        let mut references = Vec::new();
        self.collect_closure_references(closure, true, &mut references);

        for (reference, symbol) in references {
            if local_symbols.contains(&symbol) {
                continue;
            }

            let Some(symbol_data) = self.graph.resolution().and_then(|resolution| {
                resolution
                    .symbol(symbol)
                    .map(|symbol_data| (symbol_data.kind(), symbol_data.declaration()))
            }) else {
                continue;
            };

            if !matches!(
                symbol_data.0,
                SymbolKind::Var
                    | SymbolKind::Const
                    | SymbolKind::Parameter
                    | SymbolKind::RestParameter
                    | SymbolKind::ForBinding
                    | SymbolKind::PatternBinding
                    | SymbolKind::TypePatternBinding
            ) {
                continue;
            }

            if self.node_contains_child(closure, symbol_data.1) {
                continue;
            }

            let Some(ty) = self.layer.symbol_type(symbol) else {
                continue;
            };

            if !seen_captures.insert((closure, symbol)) {
                continue;
            }

            self.ownership_metadata
                .captures
                .push(CaptureMetadata::new(closure, reference, symbol, ty));
        }
    }

    fn local_symbols_in_closure(&self, closure: NodeId) -> HashSet<SymbolId> {
        let mut symbols = HashSet::new();

        self.collect_local_symbols_in_closure(closure, true, &mut symbols);

        symbols
    }

    fn collect_local_symbols_in_closure(
        &self,
        node: NodeId,
        is_root: bool,
        symbols: &mut HashSet<SymbolId>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if !is_root && syntax_node.kind().is_function_expression() {
            return;
        }

        for symbol in self.declaration_symbols_in_node(
            node,
            &[
                SymbolKind::Var,
                SymbolKind::Const,
                SymbolKind::Parameter,
                SymbolKind::RestParameter,
                SymbolKind::ForBinding,
                SymbolKind::PatternBinding,
                SymbolKind::TypePatternBinding,
            ],
        ) {
            symbols.insert(symbol);
        }

        for child in syntax_node.children() {
            self.collect_local_symbols_in_closure(*child, false, symbols);
        }
    }

    fn collect_closure_references(
        &self,
        node: NodeId,
        is_root: bool,
        references: &mut Vec<(NodeId, SymbolId)>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if !is_root && syntax_node.kind().is_function_expression() {
            return;
        }

        if syntax_node.kind() == SyntaxNodeKind::NameExpression
            && let Some(symbol) = self
                .graph
                .resolution()
                .and_then(|resolution| resolution.reference_symbol(node))
        {
            references.push((node, symbol));
        }

        for child in syntax_node.children() {
            self.collect_closure_references(*child, false, references);
        }
    }

    fn collect_temporary_metadata(
        &mut self,
        node: NodeId,
        seen_anchors: &mut HashSet<(AnchorKind, NodeId, Option<SymbolId>)>,
        seen_temporaries: &mut HashSet<NodeId>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if !matches!(
            syntax_node.kind(),
            SyntaxNodeKind::ArrayLiteral
                | SyntaxNodeKind::StructLiteral
                | SyntaxNodeKind::InferredStructLiteral
                | SyntaxNodeKind::TupleExpression
                | SyntaxNodeKind::CopyExpression
                | SyntaxNodeKind::CallExpression
        ) {
            return;
        }

        let Some(ty) = self.layer.node_type(node) else {
            return;
        };

        if !self.is_owned_edge_type(ty) {
            return;
        }

        if !seen_temporaries.insert(node) {
            return;
        }

        self.ownership_metadata
            .temporaries
            .push(TemporaryMetadata::new(node, ty));
        self.push_anchor_metadata(AnchorKind::Temporary, node, None, Some(ty), seen_anchors);
    }

    pub(super) fn is_owned_edge_type(&self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty) {
            Some(TypeKind::Named { symbol }) => self
                .graph
                .resolution()
                .and_then(|resolution| resolution.symbol(*symbol))
                .is_some_and(|symbol| {
                    matches!(symbol.kind(), SymbolKind::Struct | SymbolKind::Choice)
                }),

            Some(TypeKind::Array { .. }) | Some(TypeKind::Tuple { .. }) => true,

            Some(TypeKind::Union { members }) => members
                .iter()
                .copied()
                .any(|member| self.is_owned_edge_type(member)),

            _ => false,
        }
    }

    pub(super) fn is_fieldless_struct_type(&self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);

        let Some(TypeKind::Named { symbol }) = self.layer.table().kind(ty) else {
            return false;
        };

        let Some(symbol_data) = self
            .graph
            .resolution()
            .and_then(|resolution| resolution.symbol(*symbol))
        else {
            return false;
        };

        if symbol_data.kind() != SymbolKind::Struct {
            return false;
        }

        let Some(resolution) = self.graph.resolution() else {
            return false;
        };

        let Some(member_scope) = resolution.member_scope(*symbol) else {
            return true;
        };

        let Some(scope) = resolution.scope(member_scope) else {
            return true;
        };

        !scope.symbols().values().any(|field_symbol| {
            resolution
                .symbol(*field_symbol)
                .is_some_and(|field| field.kind() == SymbolKind::StructField)
        })
    }
}
