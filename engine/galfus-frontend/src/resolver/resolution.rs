use super::*;
use galfus_core::{ExportId, ImportId, NodeId, ScopeId, SymbolId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathReferenceKind {
    LocalMember,
    EnumVariant,
    ChoiceVariant,
    ConstraintMember,
    AnchorFunction,
}

#[derive(Debug, Clone)]
pub struct ResolutionLayer {
    builtin_scope: Option<ScopeId>,
    module_scope: ScopeId,

    scopes: Vec<Scope>,
    symbols: Vec<Symbol>,
    imports: Vec<ImportRecord>,
    exports: Vec<ExportRecord>,

    declarations: HashMap<NodeId, SymbolId>,
    references: HashMap<NodeId, SymbolId>,
    path_references: HashMap<NodeId, SymbolId>,
    path_reference_kinds: HashMap<NodeId, PathReferenceKind>,
    type_references: HashMap<NodeId, SymbolId>,
    type_path_references: HashMap<NodeId, SymbolId>,
    node_scopes: HashMap<NodeId, ScopeId>,
    member_scopes: HashMap<SymbolId, ScopeId>,
    symbol_imports: HashMap<SymbolId, ImportId>,
    exports_by_name: HashMap<String, ExportId>,
    symbol_exports: HashMap<SymbolId, ExportId>,
}

impl ResolutionLayer {
    pub fn new() -> Self {
        let module_scope = ScopeId::new(0);

        Self {
            builtin_scope: None,
            module_scope,
            scopes: Vec::new(),
            symbols: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),

            declarations: HashMap::new(),
            references: HashMap::new(),
            path_references: HashMap::new(),
            path_reference_kinds: HashMap::new(),
            type_references: HashMap::new(),
            type_path_references: HashMap::new(),
            node_scopes: HashMap::new(),
            member_scopes: HashMap::new(),
            symbol_imports: HashMap::new(),
            exports_by_name: HashMap::new(),
            symbol_exports: HashMap::new(),
        }
    }

    pub fn builtin_scope(&self) -> Option<ScopeId> {
        self.builtin_scope
    }

    pub fn module_scope(&self) -> ScopeId {
        self.module_scope
    }

    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }

    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    pub fn scope(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.raw() as usize)
    }

    pub fn scope_mut(&mut self, id: ScopeId) -> Option<&mut Scope> {
        self.scopes.get_mut(id.raw() as usize)
    }

    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.raw() as usize)
    }

    pub fn declaration_symbol(&self, node: NodeId) -> Option<SymbolId> {
        self.declarations.get(&node).copied()
    }

    pub fn reference_symbol(&self, node: NodeId) -> Option<SymbolId> {
        self.references.get(&node).copied()
    }

    pub fn path_reference_symbol(&self, node: NodeId) -> Option<SymbolId> {
        self.path_references.get(&node).copied()
    }

    pub fn path_reference_kind(&self, node: NodeId) -> Option<PathReferenceKind> {
        self.path_reference_kinds.get(&node).copied()
    }

    pub fn declarations(&self) -> &HashMap<NodeId, SymbolId> {
        &self.declarations
    }

    pub fn references(&self) -> &HashMap<NodeId, SymbolId> {
        &self.references
    }

    pub fn node_scopes(&self) -> &HashMap<NodeId, ScopeId> {
        &self.node_scopes
    }

    pub fn node_scope(&self, node: NodeId) -> Option<ScopeId> {
        self.node_scopes.get(&node).copied()
    }

    pub fn imports(&self) -> &[ImportRecord] {
        self.imports.as_slice()
    }

    pub fn import(&self, id: ImportId) -> Option<&ImportRecord> {
        self.imports.get(id.raw() as usize)
    }

    pub fn import_for_symbol(&self, symbol: SymbolId) -> Option<ImportId> {
        self.symbol_imports.get(&symbol).copied()
    }

    pub fn exports(&self) -> &[ExportRecord] {
        self.exports.as_slice()
    }

    pub fn export_record(&self, id: ExportId) -> Option<&ExportRecord> {
        self.exports.get(id.raw() as usize)
    }

    pub fn export_by_name(&self, name: &str) -> Option<ExportId> {
        self.exports_by_name.get(name).copied()
    }

    pub fn export_for_symbol(&self, symbol: SymbolId) -> Option<ExportId> {
        self.symbol_exports.get(&symbol).copied()
    }

    pub fn lookup_symbol<N: AsNameId>(&self, scope: ScopeId, name: N) -> Option<SymbolId> {
        let mut current = Some(scope);
        let name_id = name.to_name_id();

        while let Some(scope_id) = current {
            let scope = self.scope(scope_id)?;

            if let Some(symbol) = scope.symbol(name_id) {
                return Some(symbol);
            }

            current = scope.parent();
        }

        None
    }

    pub fn type_reference_symbol(&self, node: NodeId) -> Option<SymbolId> {
        self.type_references.get(&node).copied()
    }

    pub fn type_path_reference_symbol(&self, node: NodeId) -> Option<SymbolId> {
        self.type_path_references.get(&node).copied()
    }

    pub fn member_scope(&self, symbol: SymbolId) -> Option<ScopeId> {
        self.member_scopes.get(&symbol).copied()
    }

    pub fn builtin_type_symbol<N: AsNameId>(&self, name: N) -> Option<SymbolId> {
        let builtin_scope = self.builtin_scope?;
        let scope = self.scope(builtin_scope)?;

        scope.symbol(name.to_name_id())
    }

    pub(crate) fn add_scope(
        &mut self,
        kind: ScopeKind,
        parent: Option<ScopeId>,
        owner: Option<NodeId>,
    ) -> ScopeId {
        let id = ScopeId::new(self.scopes.len() as u32);

        if kind == ScopeKind::Module {
            self.module_scope = id;
        }

        if kind == ScopeKind::Builtin {
            self.builtin_scope = Some(id);
        }

        self.scopes.push(Scope::new(id, kind, parent, owner));

        if let Some(owner) = owner {
            self.bind_scope(owner, id);
        }

        id
    }

    pub(crate) fn add_symbol<N: AsNameId>(
        &mut self,
        kind: SymbolKind,
        name: N,
        declaration: NodeId,
        scope: ScopeId,
    ) -> SymbolId {
        let id = SymbolId::new(self.symbols.len() as u32);
        let name_id = name.to_name_id();

        self.symbols
            .push(Symbol::new(id, kind, name_id, declaration, scope));

        self.declarations.insert(declaration, id);

        id
    }

    pub(crate) fn bind_reference(&mut self, node: NodeId, symbol: SymbolId) {
        self.references.insert(node, symbol);
    }

    pub(crate) fn bind_path_reference(&mut self, node: NodeId, symbol: SymbolId) {
        self.bind_path_reference_kind(node, symbol, PathReferenceKind::LocalMember);
    }

    pub(crate) fn bind_path_reference_kind(
        &mut self,
        node: NodeId,
        symbol: SymbolId,
        kind: PathReferenceKind,
    ) {
        self.path_references.insert(node, symbol);
        self.path_reference_kinds.insert(node, kind);
    }

    pub(crate) fn bind_scope(&mut self, node: NodeId, scope: ScopeId) {
        self.node_scopes.insert(node, scope);
    }

    pub(crate) fn bind_member_scope(&mut self, symbol: SymbolId, scope: ScopeId) {
        self.member_scopes.insert(symbol, scope);
    }

    pub(crate) fn add_import(&mut self, details: ImportDetails) -> ImportId {
        let id = ImportId::new(self.imports.len() as u32);

        self.symbol_imports.insert(details.local_symbol, id);
        self.imports.push(ImportRecord::new(id, details));

        id
    }

    pub(crate) fn add_export(
        &mut self,
        name: String,
        kind: SymbolKind,
        export_node: NodeId,
        item_node: NodeId,
        declaration: NodeId,
        symbol: SymbolId,
    ) -> ExportId {
        if let Some(existing) = self.symbol_exports.get(&symbol).copied() {
            return existing;
        }

        if let Some(existing) = self.exports_by_name.get(name.as_str()).copied() {
            return existing;
        }

        let id = ExportId::new(self.exports.len() as u32);

        self.exports.push(ExportRecord::new(
            id,
            name.clone(),
            kind,
            export_node,
            item_node,
            declaration,
            symbol,
        ));

        self.exports_by_name.insert(name, id);
        self.symbol_exports.insert(symbol, id);

        id
    }

    pub(crate) fn bind_type_reference(&mut self, node: NodeId, symbol: SymbolId) {
        self.type_references.insert(node, symbol);
    }

    pub(crate) fn bind_type_path_reference(&mut self, node: NodeId, symbol: SymbolId) {
        self.type_path_references.insert(node, symbol);
    }
}

impl Default for ResolutionLayer {
    fn default() -> Self {
        Self::new()
    }
}
