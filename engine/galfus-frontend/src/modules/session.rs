#[cfg(test)]
mod tests;

use crate::modules;

use crate::ImportKind;
use crate::diagnostics::CheckDiagnosticCode;
use crate::modules::collect_implicit_dependencies;
use crate::modules::graph::SemanticModuleGraph;
use crate::modules::module::{FrontendModuleKind, SemanticModule};
use crate::modules::resolution::{
    is_builtin_module, is_resolvable_import, resolve_relative_import,
};
use crate::modules::snapshot::FrontendSnapshot;
use crate::{
    ImportedSurfaceTypes, ModuleSurface, SyntaxNodeKind, build_module_surface,
    check_declaration_types, check_definition_types_with_surfaces,
    imported_surface_types_for_named_export, parse, resolve,
};
use galfus_core::{
    Diagnostic, DiagnosticBag, ModuleId, ModulePath, NodeId, Revision, SourceFile, SymbolId,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct ImportCheckRecord {
    pub(crate) kind: ImportKind,
    pub(crate) source: String,
    pub(crate) local_name: String,
    pub(crate) imported_name: Option<String>,
    pub(crate) declaration: NodeId,
    pub(crate) local_symbol: SymbolId,
}

#[derive(Debug, Clone)]
pub(crate) struct PathSegmentRecord {
    pub(crate) name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PathCheckRecord {
    pub(crate) node: NodeId,
    pub(crate) segments: Vec<PathSegmentRecord>,
}

#[derive(Default)]
pub struct FrontendRoots {
    roots: Vec<modules::graph::SemanticRoot>,
}

impl FrontendRoots {
    pub fn new(roots: Vec<modules::graph::SemanticRoot>) -> Self {
        Self { roots }
    }

    pub fn roots(&self) -> &[modules::graph::SemanticRoot] {
        self.roots.as_slice()
    }
}

pub struct FrontendSource<'a> {
    pub module_id: ModuleId,
    pub path: ModulePath,
    pub source: &'a SourceFile,
    pub kind: FrontendModuleKind,
}

pub struct FrontendUpdate<'a> {
    pub source_revision: Revision,
    /// Sources added or changed since the previous check.
    pub sources: &'a [FrontendSource<'a>],
    /// Stable IDs of sources removed since the previous check.
    pub removed_modules: &'a [ModuleId],
    pub roots: &'a FrontendRoots,
    pub catalog: Arc<galfus_contract::CapabilityCatalog>,
}

pub struct FrontendReport {
    pub source_revision: Revision,
    pub semantic_revision: galfus_core::SemanticRevision,
    /// Modules whose semantic result was recomputed in this check.
    pub changed_modules: HashSet<ModuleId>,
    /// Builtin modules required by explicit imports or compiler desugaring,
    /// ordered by their canonical module path.
    pub required_dependencies: Vec<ModulePath>,
    pub diagnostics: DiagnosticBag,
}

#[derive(Default)]
pub struct FrontendSession {
    pub(super) modules: Vec<Arc<SemanticModule>>,
    module_by_path: HashMap<ModulePath, usize>,
    reverse_imports: BTreeMap<ModuleId, Box<[ModuleId]>>,
    semantic_graph: SemanticModuleGraph,
    pub diagnostics: DiagnosticBag,
    string_table: crate::StringTable,
    /// Incremented each time a module's semantic result changes in this session.
    next_semantic_revision: u64,
}

impl FrontendSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&mut self, update: FrontendUpdate<'_>) -> FrontendReport {
        let mut changed_modules =
            self.transitive_dependents(update.removed_modules.iter().copied(), &update.catalog);
        for id in update.removed_modules {
            changed_modules.insert(*id);
        }
        self.modules
            .retain(|module| !update.removed_modules.contains(&module.id()));
        self.rebuild_module_index();

        for input in update.sources {
            let existing_index = self
                .modules
                .iter()
                .position(|module| module.id() == input.module_id)
                .or_else(|| self.module_by_path.get(&input.path).copied());
            let source_changed = existing_index.is_none_or(|index| {
                let module = &self.modules[index];
                module.path() != &input.path
                    || module.source() != input.source
                    || module.kind() != input.kind
            });
            if !source_changed {
                continue;
            }

            if let Some(index) = existing_index {
                changed_modules.insert(self.modules[index].id());
                self.modules[index] = self.parse_module(input, update.source_revision);
            } else {
                changed_modules.insert(input.module_id);
                let module = self.parse_module(input, update.source_revision);
                self.modules.push(module);
            }
            self.rebuild_module_index();
        }

        self.rebuild_reverse_import_index(&update.catalog);
        changed_modules
            .extend(self.transitive_dependents(changed_modules.iter().copied(), &update.catalog));

        self.type_check_modules(&changed_modules, &update.catalog);
        self.rebuild_diagnostics(&update.catalog);
        self.semantic_graph.apply_delta(
            update.roots.roots(),
            &self.modules,
            &changed_modules,
            update.removed_modules,
            &update.catalog,
        );

        // Report the highest semantic revision produced in this check cycle.
        let semantic_revision = self
            .modules
            .iter()
            .map(|m| m.semantic_revision)
            .max()
            .unwrap_or(galfus_core::SemanticRevision::new(
                self.next_semantic_revision,
            ));

        FrontendReport {
            source_revision: update.source_revision,
            semantic_revision,
            changed_modules,
            required_dependencies: self.required_dependencies(&update.catalog),
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub fn semantic_graph(&self) -> &SemanticModuleGraph {
        &self.semantic_graph
    }

    pub fn modules(&self) -> &[Arc<SemanticModule>] {
        &self.modules
    }

    pub fn string_table(&self) -> &crate::StringTable {
        &self.string_table
    }

    pub fn snapshot(&self, semantic_revision: galfus_core::SemanticRevision) -> FrontendSnapshot {
        FrontendSnapshot::new(
            semantic_revision,
            self.modules.clone(),
            self.semantic_graph.clone(),
            self.string_table.clone(),
        )
    }

    fn required_dependencies(
        &self,
        catalog: &galfus_contract::CapabilityCatalog,
    ) -> Vec<ModulePath> {
        let mut required = HashSet::new();
        for (module_index, module) in self.modules.iter().enumerate() {
            for import in self.module_imports(module_index) {
                if is_builtin_module(import.source.as_str())
                    || catalog.is_provider_module(import.source.as_str())
                {
                    required.insert(
                        ModulePath::new(format!("{}.gfs", import.source).as_str())
                            .expect("builtin module path is valid"),
                    );
                }
            }

            let Some(root) = module.graph().syntax().root() else {
                continue;
            };
            let implicit = collect_implicit_dependencies(module.graph().syntax(), root);
            if implicit.requires_iterable {
                required.insert(ModulePath::new("std/iterable.gfs").expect("valid builtin path"));
            }
            if implicit.has_match {
                required
                    .insert(ModulePath::new("std/constraints.gfs").expect("valid builtin path"));
            }
        }
        let mut required = required.into_iter().collect::<Vec<_>>();
        required.sort();
        required
    }

    fn parse_module(
        &mut self,
        input: &FrontendSource<'_>,
        source_revision: Revision,
    ) -> Arc<SemanticModule> {
        let parse_result = parse(input.source);
        let resolve_result = resolve(
            input.source,
            parse_result.into_graph(),
            &mut self.string_table,
        );
        let graph = resolve_result.into_graph();
        self.next_semantic_revision += 1;

        Arc::new(SemanticModule {
            id: input.module_id,
            source_id: input.source.id(),
            path: input.path.clone(),
            source_revision,
            kind: input.kind,
            semantic_revision: galfus_core::SemanticRevision::new(self.next_semantic_revision),
            source: input.source.clone(),
            graph,
            type_result: None,
        })
    }

    fn rebuild_module_index(&mut self) {
        self.module_by_path = self
            .modules
            .iter()
            .enumerate()
            .map(|(index, module)| (module.path().clone(), index))
            .collect();
    }

    fn transitive_dependents(
        &self,
        roots: impl IntoIterator<Item = ModuleId>,
        _catalog: &galfus_contract::CapabilityCatalog,
    ) -> HashSet<ModuleId> {
        let mut changed = roots.into_iter().collect::<HashSet<_>>();
        let mut pending = changed.iter().copied().collect::<Vec<_>>();
        while let Some(target) = pending.pop() {
            for dependent in self
                .reverse_imports
                .get(&target)
                .into_iter()
                .flat_map(|dependents| dependents.iter().copied())
            {
                if changed.insert(dependent) {
                    pending.push(dependent);
                }
            }
        }
        changed
    }

    fn rebuild_reverse_import_index(&mut self, catalog: &galfus_contract::CapabilityCatalog) {
        let mut reverse_imports = BTreeMap::<ModuleId, Vec<ModuleId>>::new();
        for module_index in 0..self.modules.len() {
            let module_id = self.modules[module_index].id();
            let mut dependencies = self
                .module_imports(module_index)
                .into_iter()
                .filter_map(|import| {
                    self.import_target_index(module_index, import.source.as_str(), catalog)
                })
                .collect::<Vec<_>>();

            if let Some(root) = self.modules[module_index].graph().syntax().root() {
                let implicit = collect_implicit_dependencies(
                    self.modules[module_index].graph().syntax(),
                    root,
                );
                if implicit.requires_iterable {
                    dependencies.extend(self.import_target_index(
                        module_index,
                        "std/iterable",
                        catalog,
                    ));
                }
                if implicit.has_match {
                    dependencies.extend(self.import_target_index(
                        module_index,
                        "std/constraints",
                        catalog,
                    ));
                }
            }

            for dependency in dependencies {
                reverse_imports
                    .entry(self.modules[dependency].id())
                    .or_default()
                    .push(module_id);
            }
        }
        self.reverse_imports = reverse_imports
            .into_iter()
            .map(|(id, mut dependents)| {
                dependents.sort_by_key(|dependent| dependent.raw());
                dependents.dedup();
                (id, dependents.into_boxed_slice())
            })
            .collect();
    }

    fn rebuild_diagnostics(&mut self, catalog: &galfus_contract::CapabilityCatalog) {
        self.diagnostics = DiagnosticBag::new();
        for module in &self.modules {
            self.diagnostics
                .extend(module.graph().diagnostics().iter().cloned());
        }
        self.validate_imports(catalog);
        self.validate_adapter_proxy_declarations();
        for module in &self.modules {
            if let Some(result) = module.type_result() {
                self.diagnostics
                    .extend(result.diagnostics().iter().cloned());
            }
        }
    }

    fn validate_imports(&mut self, catalog: &galfus_contract::CapabilityCatalog) {
        for module_index in 0..self.modules.len() {
            let imports = self.module_imports(module_index);

            for import in imports {
                if !is_resolvable_import(import.source.as_str(), Some(catalog)) {
                    let span = self.modules[module_index]
                        .graph()
                        .syntax()
                        .node(import.declaration)
                        .map(|node| node.span())
                        .unwrap_or_else(|| self.modules[module_index].source().span());

                    self.diagnostics.push(Diagnostic::error_with_message(
                        CheckDiagnosticCode::ImportModuleNotFound,
                        format!("import module `{}` not found", import.source),
                        span,
                    ));
                    continue;
                }

                if import.kind != ImportKind::Named {
                    continue;
                }

                let Some(target_path) = resolve_relative_import(
                    self.modules[module_index].path(),
                    import.source.as_str(),
                    Some(catalog),
                ) else {
                    continue;
                };

                let Some(target_index) = self.module_by_path.get(&target_path).copied() else {
                    let span = self.modules[module_index]
                        .graph()
                        .syntax()
                        .node(import.declaration)
                        .map(|node| node.span())
                        .unwrap_or_else(|| self.modules[module_index].source().span());

                    self.diagnostics.push(Diagnostic::error_with_message(
                        CheckDiagnosticCode::ImportModuleNotFound,
                        format!("import module `{}` not found", import.source),
                        span,
                    ));
                    continue;
                };

                let Some(target_resolution) = self.modules[target_index].graph().resolution()
                else {
                    continue;
                };

                let Some(imported_name) = import.imported_name else {
                    continue;
                };

                if target_resolution
                    .export_by_name(imported_name.as_str())
                    .is_some()
                {
                    continue;
                }

                let span = self.modules[module_index]
                    .graph()
                    .syntax()
                    .node(import.declaration)
                    .map(|node| node.span())
                    .unwrap_or_else(|| self.modules[module_index].source().span());

                self.diagnostics.push(Diagnostic::error_with_message(
                    CheckDiagnosticCode::MissingExport,
                    format!(
                        "module `{}` does not export `{}`",
                        import.source, imported_name
                    ),
                    span,
                ));
            }
        }
    }

    fn validate_adapter_proxy_declarations(&mut self) {
        for module in &self.modules {
            let Some(root) = module.graph().syntax().root() else {
                continue;
            };
            let syntax = module.graph().syntax();
            for item in syntax
                .node(root)
                .into_iter()
                .flat_map(|node| node.children())
            {
                let (function, exported) = match syntax.node(*item).map(|node| node.kind()) {
                    Some(SyntaxNodeKind::FunctionItem) => (Some(*item), false),
                    Some(SyntaxNodeKind::ExportItem) => (syntax.first_child(*item), true),
                    _ => (None, false),
                };
                let Some(function) = function else {
                    continue;
                };
                if !syntax
                    .node(function)
                    .is_some_and(|node| node.kind() == SyntaxNodeKind::FunctionItem)
                {
                    continue;
                }

                let has_body = syntax
                    .node(function)
                    .and_then(|node| node.last_child())
                    .is_some_and(|last| {
                        syntax.node(last).is_some_and(|node| {
                            node.kind() == SyntaxNodeKind::Block || node.kind().is_expression()
                        })
                    });
                if has_body {
                    if module.kind() == FrontendModuleKind::AdapterProxy {
                        self.diagnostics.push(Diagnostic::error_with_message(
                            CheckDiagnosticCode::InvalidAdapterProxyDeclaration,
                            "external proxy functions must not have a body",
                            syntax
                                .node(function)
                                .map(|node| node.span())
                                .unwrap_or_else(|| module.source().span()),
                        ));
                    }
                    continue;
                }

                match module.kind() {
                    FrontendModuleKind::Standard => self.diagnostics.push(Diagnostic::error(
                        CheckDiagnosticCode::BodylessFunction,
                        syntax
                            .node(function)
                            .map(|node| node.span())
                            .unwrap_or_else(|| module.source().span()),
                    )),
                    FrontendModuleKind::Builtin => {}
                    FrontendModuleKind::AdapterProxy => {
                        let is_async = syntax
                            .first_child_of_kind(function, SyntaxNodeKind::KeywordMetadataList)
                            .and_then(|metadata| {
                                syntax.first_child_of_kind(
                                    metadata,
                                    SyntaxNodeKind::KeywordMetadataFlag,
                                )
                            })
                            .and_then(|flag| {
                                syntax.first_child_of_kind(flag, SyntaxNodeKind::Identifier)
                            })
                            .is_some_and(|flag| {
                                module.source().slice(syntax.node(flag).unwrap().span())
                                    == Some("async")
                            });
                        if !exported || !is_async {
                            self.diagnostics.push(Diagnostic::error_with_message(
                                CheckDiagnosticCode::InvalidAdapterProxyDeclaration,
                                "external proxy functions must be declared as `export fn(async)`",
                                syntax
                                    .node(function)
                                    .map(|node| node.span())
                                    .unwrap_or_else(|| module.source().span()),
                            ));
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn module_imports(&self, module_index: usize) -> Vec<ImportCheckRecord> {
        let Some(resolution) = self.modules[module_index].graph().resolution() else {
            return Vec::new();
        };

        resolution
            .imports()
            .iter()
            .map(|import| ImportCheckRecord {
                kind: import.kind(),
                source: import.source().to_string(),
                local_name: import.local_name().to_string(),
                imported_name: import.imported_name().map(str::to_string),
                declaration: import.declaration(),
                local_symbol: import.local_symbol(),
            })
            .collect()
    }

    fn type_check_modules(
        &mut self,
        changed_modules: &HashSet<ModuleId>,
        catalog: &galfus_contract::CapabilityCatalog,
    ) {
        let baseline_results = self
            .modules
            .iter()
            .map(|module| {
                check_declaration_types(
                    module.source(),
                    module.graph(),
                    &self.string_table,
                    catalog.is_provider_module(
                        module
                            .path()
                            .as_str()
                            .strip_suffix(".gfs")
                            .unwrap_or(module.path().as_str()),
                    ),
                )
            })
            .collect::<Vec<_>>();

        let mut surfaces = self
            .modules
            .iter()
            .zip(baseline_results.iter())
            .map(|(module, result)| {
                build_module_surface(module.source(), module.graph(), result, &self.string_table)
            })
            .collect::<Vec<_>>();

        let mut results = baseline_results.clone();
        for _ in 0..self.modules.len().max(1) {
            let imported_types = (0..self.modules.len())
                .map(|module_index| {
                    self.imported_surface_types_for_module(module_index, &surfaces, catalog)
                })
                .collect::<Vec<_>>();

            results = imported_types
                .iter()
                .enumerate()
                .map(|(module_index, imported_type)| {
                    let module = &self.modules[module_index];
                    check_definition_types_with_surfaces(
                        module.source(),
                        module.graph(),
                        baseline_results[module_index].clone(),
                        imported_type,
                        &self.string_table,
                        catalog.is_provider_module(
                            module
                                .path()
                                .as_str()
                                .strip_suffix(".gfs")
                                .unwrap_or(module.path().as_str()),
                        ),
                    )
                })
                .collect();

            surfaces = self
                .modules
                .iter()
                .zip(results.iter())
                .map(|(module, result)| {
                    build_module_surface(
                        module.source(),
                        module.graph(),
                        result,
                        &self.string_table,
                    )
                })
                .collect();
        }

        for (module_index, result) in results.into_iter().enumerate() {
            if !changed_modules.contains(&self.modules[module_index].id()) {
                continue;
            }
            let module = Arc::make_mut(&mut self.modules[module_index]);
            if module.type_result.is_some() {
                self.next_semantic_revision += 1;
                module.semantic_revision =
                    galfus_core::SemanticRevision::new(self.next_semantic_revision);
            }
            module.type_result = Some(result);
        }
    }

    fn imported_surface_types_for_module(
        &self,
        module_index: usize,
        surfaces: &[ModuleSurface],
        catalog: &galfus_contract::CapabilityCatalog,
    ) -> ImportedSurfaceTypes {
        let mut imported_types = ImportedSurfaceTypes::new();

        for import in self.module_imports(module_index) {
            if import.kind != ImportKind::Named
                || !is_resolvable_import(import.source.as_str(), Some(catalog))
            {
                continue;
            }

            let Some(target_index) =
                self.import_target_index(module_index, import.source.as_str(), catalog)
            else {
                continue;
            };

            let Some(imported_name) = import.imported_name.as_deref() else {
                continue;
            };

            let Some(imported_type) =
                surfaces[target_index].imported_type_for_export(import.local_symbol, imported_name)
            else {
                if let Some(imported_constraint) = surfaces[target_index]
                    .imported_constraint_for_export(imported_name, Some(import.local_symbol))
                {
                    imported_types
                        .insert_symbol_constraint(import.local_symbol, imported_constraint);
                }
                continue;
            };

            imported_types.insert_symbol_type(import.local_symbol, imported_type);

            if let Some(imported_constraint) = surfaces[target_index]
                .imported_constraint_for_export(imported_name, Some(import.local_symbol))
            {
                imported_types.insert_symbol_constraint(import.local_symbol, imported_constraint);
            }

            if let Some(imported_choice) = surfaces[target_index]
                .imported_choice_for_export(imported_name, Some(import.local_symbol))
            {
                imported_types.insert_symbol_choice(import.local_symbol, imported_choice);
            }

            imported_types.extend(imported_surface_types_for_named_export(
                &surfaces[target_index],
                import.local_symbol,
                imported_name,
            ));
        }

        self.collect_named_imported_path_types(
            module_index,
            surfaces,
            &mut imported_types,
            catalog,
        );
        self.collect_namespace_imported_path_types(
            module_index,
            surfaces,
            &mut imported_types,
            catalog,
        );

        imported_types
    }

    pub(super) fn import_target_index(
        &self,
        module_index: usize,
        source: &str,
        catalog: &galfus_contract::CapabilityCatalog,
    ) -> Option<usize> {
        let module_path = self.modules[module_index].path();
        let target_path = resolve_relative_import(module_path, source, Some(catalog))?;
        self.module_by_path.get(&target_path).copied()
    }
}
