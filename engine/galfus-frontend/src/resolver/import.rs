use super::*;
use galfus_core::{ImportId, NodeId, SymbolId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKind {
    Namespace,
    Named,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportDetails {
    pub(crate) kind: ImportKind,
    pub(crate) source: String,
    pub(crate) source_node: NodeId,
    pub(crate) import_node: NodeId,
    pub(crate) declaration: NodeId,
    pub(crate) local_name: String,
    pub(crate) imported_name: Option<String>,
    pub(crate) local_symbol: SymbolId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    id: ImportId,
    kind: ImportKind,

    source: String,
    source_node: NodeId,

    import_node: NodeId,
    declaration: NodeId,

    local_name: String,
    imported_name: Option<String>,

    local_symbol: SymbolId,
}

impl ImportRecord {
    pub(crate) fn new(id: ImportId, details: ImportDetails) -> Self {
        Self {
            id,
            kind: details.kind,
            source: details.source,
            source_node: details.source_node,
            import_node: details.import_node,
            declaration: details.declaration,
            local_name: details.local_name,
            imported_name: details.imported_name,
            local_symbol: details.local_symbol,
        }
    }

    pub fn id(&self) -> ImportId {
        self.id
    }

    pub fn kind(&self) -> ImportKind {
        self.kind
    }

    pub fn source(&self) -> &str {
        self.source.as_str()
    }

    pub fn source_node(&self) -> NodeId {
        self.source_node
    }

    pub fn import_node(&self) -> NodeId {
        self.import_node
    }

    pub fn declaration(&self) -> NodeId {
        self.declaration
    }

    pub fn local_name(&self) -> &str {
        self.local_name.as_str()
    }

    pub fn imported_name(&self) -> Option<&str> {
        self.imported_name.as_deref()
    }

    pub fn local_symbol(&self) -> SymbolId {
        self.local_symbol
    }
}

impl<'a> Resolver<'a> {
    pub(super) fn declare_import_item(&mut self, item: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        if node.kind() != SyntaxNodeKind::ImportItem {
            return;
        }

        let Some(clause) = node.first_child() else {
            return;
        };

        let Some(source_node) = self.import_source_of(item) else {
            return;
        };

        let source = self.import_source_text(source_node);

        self.declare_import_clause(item, clause, source_node, source, scope);
    }

    pub(super) fn declare_import_clause(
        &mut self,
        import_item: NodeId,
        clause: NodeId,
        source_node: NodeId,
        source: String,
        scope: ScopeId,
    ) {
        let Some(node) = self.syntax.node(clause) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::NamespaceImport => {
                self.declare_namespace_import(import_item, clause, source_node, source, scope);
            }

            SyntaxNodeKind::NamedImportList => {
                for import in node.children() {
                    self.declare_named_import(
                        import_item,
                        *import,
                        source_node,
                        source.clone(),
                        scope,
                    );
                }
            }

            _ => {}
        }
    }

    pub(super) fn declare_namespace_import(
        &mut self,
        import_item: NodeId,
        namespace_import: NodeId,
        source_node: NodeId,
        source: String,
        scope: ScopeId,
    ) {
        let Some(name) = self
            .syntax
            .first_child_of_kind(namespace_import, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        let local_name = self.node_text(name);

        let Some(symbol) =
            self.declare_symbol(local_name.clone(), SymbolKind::ImportNamespace, name, scope)
        else {
            return;
        };

        self.resolution.add_import(ImportDetails {
            kind: ImportKind::Namespace,
            source,
            source_node,
            import_node: import_item,
            declaration: name,
            local_name,
            imported_name: None,
            local_symbol: symbol,
        });
    }

    pub(super) fn declare_named_import(
        &mut self,
        import_item: NodeId,
        named_import: NodeId,
        source_node: NodeId,
        source: String,
        scope: ScopeId,
    ) {
        let Some(node) = self.syntax.node(named_import) else {
            return;
        };

        if node.kind() != SyntaxNodeKind::NamedImport {
            return;
        }

        let Some(imported_name_node) = self
            .syntax
            .first_child_of_kind(named_import, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        let imported_name = self.node_text(imported_name_node);

        let declaration = if let Some(alias) = self
            .syntax
            .first_child_of_kind(named_import, SyntaxNodeKind::ImportAlias)
        {
            self.syntax
                .first_child_of_kind(alias, SyntaxNodeKind::Identifier)
        } else {
            Some(imported_name_node)
        };

        let Some(declaration) = declaration else {
            return;
        };

        let local_name = self.node_text(declaration);

        let Some(symbol) = self.declare_symbol(
            local_name.clone(),
            SymbolKind::ImportBinding,
            declaration,
            scope,
        ) else {
            return;
        };

        self.resolution.add_import(ImportDetails {
            kind: ImportKind::Named,
            source,
            source_node,
            import_node: import_item,
            declaration,
            local_name,
            imported_name: Some(imported_name),
            local_symbol: symbol,
        });
    }

    pub(super) fn import_source_of(&self, import_item: NodeId) -> Option<NodeId> {
        self.syntax
            .first_child_of_kind(import_item, SyntaxNodeKind::ImportSource)
    }

    pub(super) fn import_source_text(&self, source_node: NodeId) -> String {
        let raw = self.node_text(source_node);

        if let Some(stripped) = raw
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            stripped.to_string()
        } else if let Some(stripped) = raw
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
        {
            stripped.to_string()
        } else {
            raw
        }
    }
}
