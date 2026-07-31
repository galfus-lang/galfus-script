#[cfg(test)]
mod tests;

use crate::modules;

use crate::ImportKind;
use crate::diagnostics::CheckDiagnosticCode;
use crate::modules::collect_implicit_dependencies;
use crate::modules::graph::SemanticModuleGraph;
use crate::modules::module::SemanticModule;
use crate::modules::resolution::{
    is_builtin_module, is_resolvable_import, resolve_relative_import,
};
use crate::{
    ImportedSurfaceTypes, ModuleSurface, build_module_surface, check_declaration_types,
    check_definition_types_with_surfaces, imported_surface_types_for_named_export, parse, resolve,
};
use galfus_core::{
    Diagnostic, DiagnosticBag, ModuleId, ModulePath, NodeId, Revision, SourceFile, SymbolId,
};
use std::collections::{HashMap, HashSet};

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
}

pub struct FrontendUpdate<'a> {
    pub source_revision: Revision,
    /// Sources added or changed since the previous check.
    pub sources: &'a [FrontendSource<'a>],
    /// Stable IDs of sources removed since the previous check.
    pub removed_modules: &'a [ModuleId],
    pub roots: &'a FrontendRoots,
}

pub struct FrontendReport {
    pub source_revision: Revision,
    pub semantic_revision: galfus_core::SemanticRevision,
    /// Modules whose semantic result was recomputed in this check.
    pub changed_modules: HashSet<ModuleId>,
    /// Builtin modules required by explicit imports or compiler desugaring.
    pub required_builtin_modules: HashSet<ModulePath>,
    pub diagnostics: DiagnosticBag,
}

#[derive(Default)]
pub struct FrontendSession {
    pub modules: Vec<SemanticModule>,
    module_by_path: HashMap<ModulePath, usize>,
    semantic_graph: SemanticModuleGraph,
    pub diagnostics: DiagnosticBag,
    /// Global counter. Incremented each time any module's semantic result changes.
    next_semantic_revision: u64,
}

impl FrontendSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&mut self, update: FrontendUpdate<'_>) -> FrontendReport {
        let mut changed_modules =
            self.transitive_dependents(update.removed_modules.iter().copied());
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
                module.path() != &input.path || module.source() != input.source
            });
            if !source_changed {
                continue;
            }

            if let Some(index) = existing_index {
                changed_modules.insert(self.modules[index].id());
                changed_modules.extend(self.transitive_dependents([self.modules[index].id()]));
                self.modules[index] = self.parse_module(input, update.source_revision);
            } else {
                changed_modules.insert(input.module_id);
                let module = self.parse_module(input, update.source_revision);
                self.modules.push(module);
            }
            self.rebuild_module_index();
            changed_modules.extend(self.transitive_dependents([input.module_id]));
        }

        self.type_check_modules(&changed_modules);
        self.rebuild_diagnostics();
        self.semantic_graph.apply_delta(
            update.roots.roots(),
            &self.modules,
            &changed_modules,
            update.removed_modules,
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
            required_builtin_modules: self.required_builtin_modules(),
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub fn semantic_graph(&self) -> &SemanticModuleGraph {
        &self.semantic_graph
    }

    fn required_builtin_modules(&self) -> HashSet<ModulePath> {
        let mut required = HashSet::new();
        for (module_index, module) in self.modules.iter().enumerate() {
            for import in self.module_imports(module_index) {
                if is_builtin_module(import.source.as_str()) {
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
        required
    }

    fn parse_module(
        &mut self,
        input: &FrontendSource<'_>,
        source_revision: Revision,
    ) -> SemanticModule {
        let parse_result = parse(input.source);
        let resolve_result = resolve(input.source, parse_result.into_graph());
        let graph = resolve_result.into_graph();
        self.next_semantic_revision += 1;

        SemanticModule {
            id: input.module_id,
            source_id: input.source.id(),
            path: input.path.clone(),
            source_revision,
            semantic_revision: galfus_core::SemanticRevision::new(self.next_semantic_revision),
            source: input.source.clone(),
            graph,
            type_result: None,
        }
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
    ) -> HashSet<ModuleId> {
        let mut changed = roots.into_iter().collect::<HashSet<_>>();
        let mut pending = changed.iter().copied().collect::<Vec<_>>();
        while let Some(target) = pending.pop() {
            for (index, module) in self.modules.iter().enumerate() {
                if changed.contains(&module.id()) {
                    continue;
                }
                if self.module_imports(index).iter().any(|import| {
                    self.import_target_index(index, import.source.as_str())
                        .is_some_and(|target_index| self.modules[target_index].id() == target)
                }) {
                    changed.insert(module.id());
                    pending.push(module.id());
                }
            }
        }
        changed
    }

    fn rebuild_diagnostics(&mut self) {
        self.diagnostics = DiagnosticBag::new();
        for module in &self.modules {
            self.diagnostics
                .extend(module.graph().diagnostics().iter().cloned());
        }
        self.validate_imports();
        for module in &self.modules {
            if let Some(result) = module.type_result() {
                self.diagnostics
                    .extend(result.diagnostics().iter().cloned());
            }
        }
    }

    fn validate_imports(&mut self) {
        for module_index in 0..self.modules.len() {
            let imports = self.module_imports(module_index);

            for import in imports {
                if !is_resolvable_import(import.source.as_str()) {
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

    fn type_check_modules(&mut self, changed_modules: &HashSet<ModuleId>) {
        let baseline_results = self
            .modules
            .iter()
            .map(|module| check_declaration_types(module.source(), module.graph()))
            .collect::<Vec<_>>();

        let surfaces = self
            .modules
            .iter()
            .zip(baseline_results.iter())
            .map(|(module, result)| build_module_surface(module.graph(), result))
            .collect::<Vec<_>>();

        let imported_types = (0..self.modules.len())
            .map(|module_index| self.imported_surface_types_for_module(module_index, &surfaces))
            .collect::<Vec<_>>();

        for ((module_index, imported_type), previous_result) in imported_types
            .iter()
            .enumerate()
            .zip(baseline_results.into_iter())
        {
            if !changed_modules.contains(&self.modules[module_index].id()) {
                continue;
            }
            if self.modules[module_index].type_result.is_some() {
                self.next_semantic_revision += 1;
                self.modules[module_index].semantic_revision =
                    galfus_core::SemanticRevision::new(self.next_semantic_revision);
            }
            let result = check_definition_types_with_surfaces(
                self.modules[module_index].source(),
                self.modules[module_index].graph(),
                previous_result,
                imported_type,
            );

            self.modules[module_index].type_result = Some(result);
        }
    }

    fn imported_surface_types_for_module(
        &self,
        module_index: usize,
        surfaces: &[ModuleSurface],
    ) -> ImportedSurfaceTypes {
        let mut imported_types = ImportedSurfaceTypes::new();

        for import in self.module_imports(module_index) {
            if import.kind != ImportKind::Named || !is_resolvable_import(import.source.as_str()) {
                continue;
            }

            let Some(target_index) = self.import_target_index(module_index, import.source.as_str())
            else {
                continue;
            };

            let Some(imported_name) = import.imported_name.as_deref() else {
                continue;
            };

            let Some(imported_type) =
                surfaces[target_index].imported_type_for_export(import.local_symbol, imported_name)
            else {
                if let Some(imported_constraint) =
                    surfaces[target_index].imported_constraint_for_export(imported_name)
                {
                    imported_types
                        .insert_symbol_constraint(import.local_symbol, imported_constraint);
                }
                continue;
            };

            imported_types.insert_symbol_type(import.local_symbol, imported_type);

            if let Some(imported_constraint) =
                surfaces[target_index].imported_constraint_for_export(imported_name)
            {
                imported_types.insert_symbol_constraint(import.local_symbol, imported_constraint);
            }

            if let Some(imported_choice) =
                surfaces[target_index].imported_choice_for_export(imported_name)
            {
                imported_types.insert_symbol_choice(import.local_symbol, imported_choice);
            }

            imported_types.extend(imported_surface_types_for_named_export(
                &surfaces[target_index],
                import.local_symbol,
                imported_name,
            ));
        }

        self.collect_named_imported_path_types(module_index, surfaces, &mut imported_types);
        self.collect_namespace_imported_path_types(module_index, surfaces, &mut imported_types);

        imported_types
    }

    pub(super) fn import_target_index(&self, module_index: usize, source: &str) -> Option<usize> {
        let module_path = self.modules[module_index].path();
        let target_path = resolve_relative_import(module_path, source)?;
        self.module_by_path.get(&target_path).copied()
    }
}
