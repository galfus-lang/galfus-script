use super::{
    DeclarationTypeChecker, OwnershipCycleMetadata, ReleaseEligibilityKind,
    ReleaseEligibilityMetadata,
};
use crate::{PrimitiveType, SymbolKind, TypeDiagnosticCode, TypeKind};
use galfus_core::{Diagnostic, NodeId, SymbolId, TypeId};
use std::collections::{HashMap, HashSet};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn validate_ownership_cycles(&mut self) {
        let graph = self.strong_edge_graph();
        let mut seen_cycles = HashSet::new();

        for start in graph.keys().copied() {
            let mut path = Vec::new();
            let mut visiting = HashSet::new();

            self.collect_ownership_cycles_from(
                start,
                start,
                &graph,
                &mut path,
                &mut visiting,
                &mut seen_cycles,
            );
        }
    }

    fn strong_edge_graph(&self) -> HashMap<SymbolId, Vec<SymbolId>> {
        let mut graph = HashMap::new();

        for edge in &self.ownership_metadata.edges {
            let mut targets = Vec::new();
            self.collect_owned_struct_symbols(edge.field_type(), &mut targets);

            if targets.is_empty() {
                continue;
            }

            let entry = graph.entry(edge.owner_symbol()).or_insert_with(Vec::new);

            for target in targets {
                if !entry.contains(&target) {
                    entry.push(target);
                }
            }
        }

        graph
    }

    fn collect_ownership_cycles_from(
        &mut self,
        start: SymbolId,
        current: SymbolId,
        graph: &HashMap<SymbolId, Vec<SymbolId>>,
        path: &mut Vec<SymbolId>,
        visiting: &mut HashSet<SymbolId>,
        seen_cycles: &mut HashSet<Vec<u32>>,
    ) {
        if !visiting.insert(current) {
            return;
        }

        path.push(current);

        if let Some(targets) = graph.get(&current) {
            for target in targets {
                if *target == start {
                    let mut cycle = path.clone();
                    cycle.push(start);

                    if self.record_ownership_cycle(cycle, seen_cycles) {
                        continue;
                    }
                }

                if visiting.contains(target) {
                    continue;
                }

                self.collect_ownership_cycles_from(
                    start,
                    *target,
                    graph,
                    path,
                    visiting,
                    seen_cycles,
                );
            }
        }

        path.pop();
        visiting.remove(&current);
    }

    fn record_ownership_cycle(
        &mut self,
        cycle: Vec<SymbolId>,
        seen_cycles: &mut HashSet<Vec<u32>>,
    ) -> bool {
        let key = self.ownership_cycle_key(cycle.as_slice());

        if !seen_cycles.insert(key) {
            return false;
        }

        self.ownership_metadata
            .cycles
            .push(OwnershipCycleMetadata::new(cycle));

        true
    }

    fn ownership_cycle_key(&self, cycle: &[SymbolId]) -> Vec<u32> {
        let mut raw_cycle = cycle
            .iter()
            .take(cycle.len().saturating_sub(1))
            .map(|symbol| symbol.raw())
            .collect::<Vec<_>>();

        raw_cycle.sort_unstable();
        raw_cycle.dedup();
        raw_cycle
    }

    fn collect_owned_struct_symbols(&self, ty: TypeId, symbols: &mut Vec<SymbolId>) {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty) {
            Some(TypeKind::Named { symbol }) => {
                if self
                    .graph
                    .resolution()
                    .and_then(|resolution| resolution.symbol(*symbol))
                    .is_some_and(|symbol| symbol.kind() == SymbolKind::Struct)
                    && !symbols.contains(symbol)
                {
                    symbols.push(*symbol);
                }
            }

            Some(TypeKind::Array { element }) | Some(TypeKind::Range { element }) => {
                self.collect_owned_struct_symbols(*element, symbols);
            }

            Some(TypeKind::Tuple { elements }) | Some(TypeKind::Union { members: elements }) => {
                for element in elements {
                    self.collect_owned_struct_symbols(*element, symbols);
                }
            }

            Some(TypeKind::GenericInstance { base, arguments }) => {
                self.collect_owned_struct_symbols(*base, symbols);

                for argument in arguments {
                    self.collect_owned_struct_symbols(*argument, symbols);
                }
            }

            _ => {}
        }
    }

    pub(super) fn validate_release_eligibility(&mut self) {
        let mut seen = HashSet::new();

        for anchor in self.ownership_metadata.anchors.clone() {
            let Some(ty) = anchor.ty() else {
                continue;
            };

            self.push_release_eligibility(
                ReleaseEligibilityKind::Anchor,
                anchor.node(),
                anchor.symbol(),
                ty,
                &mut seen,
            );
        }

        for capture in self.ownership_metadata.captures.clone() {
            self.push_release_eligibility(
                ReleaseEligibilityKind::Capture,
                capture.reference(),
                Some(capture.symbol()),
                capture.ty(),
                &mut seen,
            );
        }

        for temporary in self.ownership_metadata.temporaries.clone() {
            self.push_release_eligibility(
                ReleaseEligibilityKind::Temporary,
                temporary.expression(),
                None,
                temporary.ty(),
                &mut seen,
            );
        }
    }

    pub(super) fn push_release_eligibility(
        &mut self,
        kind: ReleaseEligibilityKind,
        node: NodeId,
        symbol: Option<SymbolId>,
        ty: TypeId,
        seen: &mut HashSet<(ReleaseEligibilityKind, NodeId, Option<SymbolId>)>,
    ) {
        if !self.is_owned_edge_type(ty) {
            return;
        }

        if !seen.insert((kind, node, symbol)) {
            return;
        }

        self.ownership_metadata
            .release_eligibilities
            .push(ReleaseEligibilityMetadata::new(kind, node, symbol, ty));
    }

    pub(super) fn node_contains_child(&self, parent: NodeId, child: NodeId) -> bool {
        if parent == child {
            return true;
        }

        let Some(parent_node) = self.graph.syntax().node(parent) else {
            return false;
        };

        parent_node
            .children()
            .iter()
            .any(|nested| self.node_contains_child(*nested, child))
    }

    pub(super) fn is_weak_field_nullable_type(&self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);
        let null_type = self.layer.table().primitive(PrimitiveType::Null);

        match self.layer.table().kind(ty) {
            Some(TypeKind::Union { members }) => members.contains(&null_type),
            Some(TypeKind::Primitive(PrimitiveType::Null)) => true,
            _ => false,
        }
    }

    pub(super) fn report_invalid_weak_field_type(&mut self, field: NodeId, field_type: TypeId) {
        let span = self
            .graph
            .syntax()
            .node(field)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        let field_type = self.describe_type_for_diagnostic(field_type);

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidWeakFieldType,
            format!("weak field type must be nullable, got `{field_type}`"),
            span,
        ));
    }
}
