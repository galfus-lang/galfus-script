use crate::modules;

use crate::ImportKind;
use crate::modules::module::SemanticModule;
use crate::modules::resolution::resolve_relative_import;
use crate::{ImportRecord, SyntaxNodeKind};
use galfus_core::{ModuleId, ModulePath, NodeId, SemanticRevision};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticRootKind {
    Entry,
    Export { address: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticImportKind {
    Explicit(ImportKind),
    Implicit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRoot {
    kind: SemanticRootKind,
    module_id: ModuleId,
    path: ModulePath,
}

impl SemanticRoot {
    pub fn new(kind: SemanticRootKind, module_id: ModuleId, path: ModulePath) -> Self {
        Self {
            kind,
            module_id,
            path,
        }
    }

    pub fn kind(&self) -> &SemanticRootKind {
        &self.kind
    }

    pub fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub fn path(&self) -> &ModulePath {
        &self.path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticImportEdge {
    from: ModuleId,
    kind: SemanticImportKind,
    source: String,
    source_node: Option<NodeId>,
    local_name: String,
    imported_name: Option<String>,
    target_path: ModulePath,
    to: Option<ModuleId>,
    export_name: Option<String>,
    referenced_exports: Vec<String>,
}

impl SemanticImportEdge {
    pub fn from(&self) -> ModuleId {
        self.from
    }

    pub fn kind(&self) -> SemanticImportKind {
        self.kind
    }

    pub fn source(&self) -> &str {
        self.source.as_str()
    }

    pub fn source_node(&self) -> Option<NodeId> {
        self.source_node
    }

    pub fn local_name(&self) -> &str {
        self.local_name.as_str()
    }

    pub fn imported_name(&self) -> Option<&str> {
        self.imported_name.as_deref()
    }

    pub fn target_path(&self) -> &ModulePath {
        &self.target_path
    }

    pub fn to(&self) -> Option<ModuleId> {
        self.to
    }

    pub fn is_resolved(&self) -> bool {
        self.to.is_some()
    }

    pub fn export_name(&self) -> Option<&str> {
        self.export_name.as_deref()
    }

    pub fn is_export_resolved(&self) -> bool {
        self.export_name.is_some()
    }

    pub fn referenced_exports(&self) -> &[String] {
        self.referenced_exports.as_slice()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticModuleGraph {
    roots: Vec<SemanticRoot>,
    modules: HashMap<ModuleId, SemanticModule>,
    module_by_path: HashMap<ModulePath, ModuleId>,
    import_edges: Vec<SemanticImportEdge>,
}

impl SemanticModuleGraph {
    pub fn build(roots: &[SemanticRoot], modules: &[SemanticModule]) -> Self {
        let mut graph = Self::default();
        let changed_modules = modules.iter().map(SemanticModule::id).collect();
        graph.apply_delta(roots, modules, &changed_modules, &[]);

        graph
    }

    pub fn apply_delta(
        &mut self,
        roots: &[SemanticRoot],
        modules: &[SemanticModule],
        changed_modules: &HashSet<ModuleId>,
        removed_modules: &[ModuleId],
    ) {
        self.roots = roots.to_vec();

        for id in removed_modules {
            if let Some(previous) = self.modules.remove(id) {
                self.module_by_path.remove(previous.path());
            }
        }

        for module in modules {
            if !changed_modules.contains(&module.id()) {
                continue;
            }
            if let Some(previous) = self.modules.insert(module.id(), module.clone())
                && previous.path() != module.path()
            {
                self.module_by_path.remove(previous.path());
            }
            self.module_by_path
                .insert(module.path().clone(), module.id());
        }

        self.import_edges.retain(|edge| {
            !changed_modules.contains(&edge.from) && !removed_modules.contains(&edge.from)
        });
        for edge in &mut self.import_edges {
            if edge.to.is_some_and(|id| removed_modules.contains(&id)) {
                edge.to = None;
                edge.export_name = None;
                edge.referenced_exports.clear();
            }
        }

        for module in modules {
            if !changed_modules.contains(&module.id()) {
                continue;
            }
            self.add_import_edges_for(module, modules);
            self.add_implicit_import_edges_for(module);
        }
    }

    pub fn roots(&self) -> &[SemanticRoot] {
        self.roots.as_slice()
    }

    pub fn import_edges(&self) -> &[SemanticImportEdge] {
        self.import_edges.as_slice()
    }

    pub fn module_by_path(&self, path: &ModulePath) -> Option<ModuleId> {
        self.module_by_path.get(path).copied()
    }

    pub fn get(&self, id: ModuleId) -> Option<&SemanticModule> {
        self.modules.get(&id)
    }

    pub fn modules(&self) -> impl Iterator<Item = &SemanticModule> {
        self.modules.values()
    }

    pub fn semantic_revision(&self, id: ModuleId) -> Option<SemanticRevision> {
        self.get(id).map(SemanticModule::semantic_revision)
    }

    fn add_import_edges_for(&mut self, module: &SemanticModule, modules: &[SemanticModule]) {
        let from = module.id();

        let Some(resolution) = module.graph().resolution() else {
            return;
        };

        for import in resolution.imports() {
            let source = import.source();

            let Some(target_path) = resolve_relative_import(module.path(), source) else {
                continue;
            };

            let to = self.module_by_path(&target_path);

            let export_name = self.resolve_import_export(import, to, modules);
            let referenced_exports =
                self.resolve_namespace_referenced_exports(module, import, to, modules);

            self.import_edges.push(SemanticImportEdge {
                from,
                kind: SemanticImportKind::Explicit(import.kind()),
                source: source.to_string(),
                source_node: Some(import.source_node()),
                local_name: import.local_name().to_string(),
                imported_name: import.imported_name().map(str::to_string),
                target_path,
                to,
                export_name,
                referenced_exports,
            });
        }
    }

    fn add_implicit_import_edges_for(&mut self, module: &SemanticModule) {
        let Some(root) = module.graph().syntax().root() else {
            return;
        };
        let dependencies = modules::collect_implicit_dependencies(module.graph().syntax(), root);

        if dependencies.requires_iterable {
            self.add_implicit_import_edge(module.id(), "std/iterable.gfs");
        }
        if dependencies.has_match {
            self.add_implicit_import_edge(module.id(), "std/constraints.gfs");
        }
    }

    fn add_implicit_import_edge(&mut self, from: ModuleId, target_path: &str) {
        let Some(target_path) = ModulePath::new(target_path) else {
            return;
        };
        if self
            .import_edges
            .iter()
            .any(|edge| edge.from == from && edge.target_path == target_path)
        {
            return;
        }

        let source = target_path
            .as_str()
            .strip_suffix(".gfs")
            .unwrap_or(target_path.as_str())
            .to_string();
        let to = self.module_by_path(&target_path);
        self.import_edges.push(SemanticImportEdge {
            from,
            kind: SemanticImportKind::Implicit,
            source,
            source_node: None,
            local_name: String::new(),
            imported_name: None,
            target_path,
            to,
            export_name: None,
            referenced_exports: Vec::new(),
        });
    }

    fn resolve_import_export(
        &self,
        import: &ImportRecord,
        to: Option<ModuleId>,
        modules: &[SemanticModule],
    ) -> Option<String> {
        if import.kind() != ImportKind::Named {
            return None;
        }

        let target_module = modules.iter().find(|m| Some(m.id()) == to)?;
        let target_resolution = target_module.graph().resolution()?;
        let imported_name = import.imported_name()?;

        target_resolution
            .export_by_name(imported_name)
            .map(|_| imported_name.to_string())
    }

    fn resolve_namespace_referenced_exports(
        &self,
        module: &SemanticModule,
        import: &ImportRecord,
        to: Option<ModuleId>,
        modules: &[SemanticModule],
    ) -> Vec<String> {
        if import.kind() != ImportKind::Namespace {
            return Vec::new();
        }

        let Some(target_module) = modules.iter().find(|m| Some(m.id()) == to) else {
            return Vec::new();
        };
        let Some(target_resolution) = target_module.graph().resolution() else {
            return Vec::new();
        };

        let mut references = self.namespace_reference_names(module, import.local_name());

        references.retain(|name| target_resolution.export_by_name(name.as_str()).is_some());
        references.sort();
        references.dedup();

        references
    }

    fn namespace_reference_names(&self, module: &SemanticModule, namespace: &str) -> Vec<String> {
        let Some(root) = module.graph().syntax().root() else {
            return Vec::new();
        };

        let mut references = Vec::new();
        self.collect_namespace_reference_names(module, root, namespace, &mut references);
        references
    }

    fn collect_namespace_reference_names(
        &self,
        module: &SemanticModule,
        node: NodeId,
        namespace: &str,
        references: &mut Vec<String>,
    ) {
        let syntax = module.graph().syntax();

        let Some(syntax_node) = syntax.node(node) else {
            return;
        };

        if matches!(
            syntax_node.kind(),
            SyntaxNodeKind::PathExpression | SyntaxNodeKind::Path
        ) {
            if let Some(reference) = self.namespace_reference_name(module, node, namespace) {
                references.push(reference);
            }

            return;
        }

        for child in syntax_node.children() {
            self.collect_namespace_reference_names(module, *child, namespace, references);
        }
    }

    fn namespace_reference_name(
        &self,
        module: &SemanticModule,
        node: NodeId,
        namespace: &str,
    ) -> Option<String> {
        let segments = self.path_segments(module, node);
        let root = segments.first()?;

        if root != namespace || segments.len() < 2 {
            return None;
        }

        Some(segments[1..].join("::"))
    }

    fn path_segments(&self, module: &SemanticModule, node: NodeId) -> Vec<String> {
        let syntax = module.graph().syntax();
        let Some(syntax_node) = syntax.node(node) else {
            return Vec::new();
        };

        match syntax_node.kind() {
            SyntaxNodeKind::NameExpression => syntax
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .map(|identifier| self.node_text(module, identifier))
                .into_iter()
                .collect(),

            SyntaxNodeKind::PathExpression => {
                let Some(target) = syntax.child(node, 0) else {
                    return Vec::new();
                };

                let Some(member) = syntax.child(node, 1) else {
                    return Vec::new();
                };

                let mut segments = self.path_segments(module, target);
                segments.push(self.node_text(module, member));
                segments
            }

            SyntaxNodeKind::Path => syntax_node
                .children()
                .iter()
                .filter_map(|child| {
                    let child_node = syntax.node(*child)?;

                    if child_node.kind() != SyntaxNodeKind::Identifier {
                        return None;
                    }

                    Some(self.node_text(module, *child))
                })
                .collect(),

            _ => Vec::new(),
        }
    }

    fn node_text(&self, module: &SemanticModule, node: NodeId) -> String {
        let Some(syntax_node) = module.graph().syntax().node(node) else {
            return String::new();
        };

        module
            .source()
            .slice(syntax_node.span())
            .unwrap_or("")
            .to_string()
    }
}
