#[cfg(test)]
mod tests;

use std::str;

use crate::config::{WorkspaceConfig, parse_workspace_config};
use crate::source_store::ModuleOrigin;
use crate::state::{
    BytecodeState, CheckState, CompileBlocked, CompileState, RunBlocked, SemanticState,
    SourceState, WorkspaceError,
};
use galfus_bytecode::{BytecodeGraph, ImportEdge};
use galfus_compiler::CompiledModule;
use galfus_contract::Providers;
use galfus_core::{DiagnosticBag, ModulePath, SourceFile};
use galfus_frontend::modules::{
    FrontendRoots, FrontendSession, FrontendSource, FrontendUpdate, SemanticRoot, SemanticRootKind,
};
use galfus_runtime::{Execution, Runtime, RuntimeError, format_panic};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct Workspace {
    pub config: Option<WorkspaceConfig>,
    pub source_state: SourceState,
    pub semantic_state: SemanticState,
    pub bytecode_state: BytecodeState,
    pub frontend: FrontendSession,
}

pub enum LoadResult {
    Success,
    Diagnostics(DiagnosticBag),
}

pub enum RemoveResult {
    Success,
    NotFound,
}

pub struct CheckReport<'a> {
    pub is_valid: bool,
    pub diagnostics: &'a DiagnosticBag,
}

/// Result of a successful `compile()` call.
pub struct CompileReport {
    /// The compiled module graph, ready to be passed to the runtime.
    pub graph: Arc<BytecodeGraph>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            config: None,
            source_state: SourceState::new(),
            semantic_state: SemanticState::new(),
            bytecode_state: BytecodeState::new(),
            frontend: FrontendSession::new(),
        }
    }

    pub fn load_config(&mut self, config_toml: &[u8]) -> Result<LoadResult, WorkspaceError> {
        let text = match str::from_utf8(config_toml) {
            Ok(t) => t,
            Err(_) => return Err(WorkspaceError::MissingConfiguration),
        };

        let mut diagnostics = DiagnosticBag::new();
        if let Some(config) = parse_workspace_config(text, &mut diagnostics) {
            self.config = Some(config);
            self.mark_dirty();
            Ok(LoadResult::Success)
        } else {
            Ok(LoadResult::Diagnostics(diagnostics))
        }
    }

    pub fn load_module(
        &mut self,
        path: &str,
        module_bytes: &[u8],
    ) -> Result<LoadResult, WorkspaceError> {
        let module_path = ModulePath::new(path).ok_or(WorkspaceError::InvalidPath)?;
        if self
            .source_state
            .store
            .get(&module_path)
            .is_some_and(|entry| entry.bytes.as_ref() == module_bytes)
        {
            return Ok(LoadResult::Success);
        }

        self.source_state.revision.next();
        self.source_state.store.load_module(
            module_path.clone(),
            Arc::from(module_bytes),
            ModuleOrigin::User,
            self.source_state.revision,
        );
        self.source_state.dirty_sources.insert(module_path);
        self.mark_dirty();
        Ok(LoadResult::Success)
    }

    pub fn remove_module(&mut self, path: &str) -> Result<RemoveResult, WorkspaceError> {
        let module_path = ModulePath::new(path).ok_or(WorkspaceError::InvalidPath)?;

        if let Some(entry) = self.source_state.store.remove_module(&module_path) {
            self.source_state.revision.next();
            self.source_state.dirty_sources.remove(&module_path);
            self.source_state.removed_modules.push(entry.module_id);
            self.mark_dirty();
            Ok(RemoveResult::Success)
        } else {
            Ok(RemoveResult::NotFound)
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.semantic_state.check_state.is_dirty()
    }

    fn mark_dirty(&mut self) {
        let previous = match &self.semantic_state.check_state {
            CheckState::Passed { revision, .. } | CheckState::Failed { revision, .. } => {
                Some(*revision)
            }
            CheckState::Dirty {
                previous_checked_revision,
                ..
            } => *previous_checked_revision,
        };

        self.semantic_state.check_state = CheckState::Dirty {
            current_revision: self.source_state.revision,
            previous_checked_revision: previous,
        };

        // Mark compile stale when check is invalidated.
        if let CompileState::Ready {
            semantic_revision,
            graph,
        } = &self.bytecode_state.compile_state
        {
            self.bytecode_state.compile_state = CompileState::Stale {
                semantic_revision: *semantic_revision,
                graph: Arc::clone(graph),
            };
        }
    }

    pub fn check(&mut self) -> CheckReport<'_> {
        let is_dirty = matches!(self.semantic_state.check_state, CheckState::Dirty { .. });

        if is_dirty {
            if self.config.is_none() {
                self.semantic_state.check_state = CheckState::Failed {
                    revision: self.source_state.revision,
                    diagnostics: DiagnosticBag::new(),
                };
            } else {
                let roots = self.frontend_roots();
                let report = loop {
                    let source_files = self
                        .source_state
                        .dirty_sources
                        .iter()
                        .filter_map(|path| self.source_state.store.get(path))
                        .map(|entry| {
                            (
                                entry.module_id,
                                entry.path.clone(),
                                SourceFile::new(
                                    entry.source_id,
                                    entry.path.to_string(),
                                    str::from_utf8(&entry.bytes).unwrap_or("").to_string(),
                                ),
                            )
                        })
                        .collect::<Vec<_>>();
                    let sources = source_files
                        .iter()
                        .map(|(module_id, path, source)| FrontendSource {
                            module_id: *module_id,
                            path: path.clone(),
                            source,
                        })
                        .collect::<Vec<_>>();
                    let update = FrontendUpdate {
                        source_revision: self.source_state.revision,
                        sources: &sources,
                        removed_modules: self.source_state.removed_modules.as_slice(),
                        roots: &roots,
                    };
                    let report = self.frontend.check(update);
                    self.source_state.dirty_sources.clear();
                    self.source_state.removed_modules.clear();

                    if !self.load_required_builtins(&report.required_builtin_modules) {
                        break report;
                    }
                };

                if report.diagnostics.has_errors() {
                    self.semantic_state.check_state = CheckState::Failed {
                        revision: report.source_revision,
                        diagnostics: report.diagnostics,
                    };
                } else {
                    self.semantic_state.check_state = CheckState::Passed {
                        revision: report.source_revision,
                        semantic_revision: report.semantic_revision,
                        changed_modules: report.changed_modules,
                        diagnostics: report.diagnostics,
                    };
                }
            }
        }

        match &self.semantic_state.check_state {
            CheckState::Passed { diagnostics, .. } => CheckReport {
                is_valid: true,
                diagnostics,
            },
            CheckState::Failed { diagnostics, .. } => CheckReport {
                is_valid: false,
                diagnostics,
            },
            CheckState::Dirty { .. } => unreachable!(),
        }
    }

    fn load_required_builtins(&mut self, paths: &HashSet<ModulePath>) -> bool {
        let mut loaded = false;
        for path in paths {
            if self.source_state.store.get(path).is_some() {
                continue;
            }
            let builtin_name = path.as_str().strip_suffix(".gfs").unwrap_or(path.as_str());
            let Some((_, source)) = galfus_builtins::BUILTIN_MODULES
                .iter()
                .find(|(name, _)| *name == builtin_name)
            else {
                continue;
            };
            self.source_state.revision.next();
            self.source_state.store.load_module(
                path.clone(),
                Arc::from(source.as_bytes()),
                ModuleOrigin::Builtin,
                self.source_state.revision,
            );
            self.source_state.dirty_sources.insert(path.clone());
            loaded = true;
        }
        loaded
    }

    fn frontend_roots(&self) -> FrontendRoots {
        let Some(config) = &self.config else {
            return FrontendRoots::default();
        };

        let mut roots = Vec::new();
        if let Some(entry) = config.entry()
            && let Some(source) = self.source_state.store.get(entry)
        {
            roots.push(SemanticRoot::new(
                SemanticRootKind::Entry,
                source.module_id,
                entry.clone(),
            ));
        }
        for export in config.exports() {
            if let Some(source) = self.source_state.store.get(export.path()) {
                roots.push(SemanticRoot::new(
                    SemanticRootKind::Export {
                        address: export.address().to_string(),
                    },
                    source.module_id,
                    export.path().clone(),
                ));
            }
        }

        FrontendRoots::new(roots)
    }

    /// Compile the workspace into a [`BytecodeGraph`].
    ///
    /// Gate rules:
    /// - Returns `Err(CompileBlocked::Dirty)` if `check()` has not been called
    ///   since the last source change.
    /// - Returns `Err(CompileBlocked::CheckFailed)` if the last `check()` had errors.
    /// - Returns `Err(CompileBlocked::MissingConfiguration)` if no config was loaded.
    /// - Returns `Ok(CompileReport)` with the compiled graph on success.
    pub fn compile(&mut self) -> Result<CompileReport, CompileBlocked> {
        // Gate: check must have passed.
        let (semantic_revision, changed_modules) = match &self.semantic_state.check_state {
            CheckState::Dirty {
                current_revision,
                previous_checked_revision,
            } => {
                return Err(CompileBlocked::Dirty {
                    current_revision: *current_revision,
                    checked_revision: *previous_checked_revision,
                });
            }
            CheckState::Failed {
                revision,
                diagnostics,
            } => {
                return Err(CompileBlocked::CheckFailed {
                    revision: *revision,
                    error_count: diagnostics.iter().filter(|d| d.is_error()).count(),
                });
            }
            CheckState::Passed {
                semantic_revision,
                changed_modules,
                ..
            } => (*semantic_revision, changed_modules.clone()),
        };

        // Skip recompilation if already up-to-date.
        if let CompileState::Ready {
            semantic_revision: compiled_rev,
            graph,
        } = &self.bytecode_state.compile_state
            && *compiled_rev == semantic_revision
        {
            return Ok(CompileReport {
                graph: Arc::clone(graph),
            });
        }

        let cached_graph = match &self.bytecode_state.compile_state {
            CompileState::Stale { graph, .. } => Some(graph),
            _ => None,
        };
        let empty_graph = BytecodeGraph::new();
        let base_graph = cached_graph
            .map(|graph| graph.as_ref())
            .unwrap_or(&empty_graph);

        // The first compilation has no graph to upsert into, so it must emit
        // every semantic module even if the last frontend delta was narrower.
        let compilation_targets = if let Some(cached_graph) = cached_graph {
            self.frontend
                .modules
                .iter()
                .filter(|module| changed_modules.contains(&module.id()))
                .filter(|module| {
                    cached_graph
                        .get(module.id())
                        .is_none_or(|image| image.semantic_revision() != module.semantic_revision())
                })
                .map(|module| module.id())
                .collect::<HashSet<_>>()
        } else {
            self.frontend
                .modules
                .iter()
                .map(|module| module.id())
                .collect::<HashSet<_>>()
        };

        let semantic_graph = self.frontend.semantic_graph();
        let mut reachable_modules = HashSet::new();

        // Build CompiledModule list from the frontend's semantic modules.
        let semantic_modules = &self.frontend.modules;

        let mut path_to_id = HashMap::new();
        for module in semantic_modules {
            path_to_id.insert(module.path().clone(), module.id());
        }

        let mut queue: Vec<galfus_core::ModuleId> = semantic_graph
            .roots()
            .iter()
            .map(|r| r.module_id())
            .collect();
        while let Some(id) = queue.pop() {
            if reachable_modules.insert(id) {
                for edge in semantic_graph.import_edges() {
                    if edge.from() == id {
                        let to = edge
                            .to()
                            .or_else(|| path_to_id.get(edge.target_path()).copied());
                        if let Some(to) = to {
                            queue.push(to);
                        }
                    }
                }
            }
        }

        let mut compilation_targets: HashSet<_> = compilation_targets
            .into_iter()
            .filter(|id| reachable_modules.contains(id))
            .collect();

        // If a module is reachable but NOT in the base_graph, it MUST be compiled!
        // This happens if a module became unreachable (and was removed from bytecode graph)
        // but then became reachable again without changing source code.
        for id in &reachable_modules {
            if base_graph.get(*id).is_none() {
                compilation_targets.insert(*id);
            }
        }

        let mut compiled_modules: Vec<CompiledModule> = semantic_modules
            .iter()
            .filter(|m| reachable_modules.contains(&m.id()))
            .map(|m| {
                CompiledModule::new(
                    m.id(),
                    m.path().clone(),
                    m.semantic_revision(),
                    m.source().clone(),
                    m.graph().clone(),
                    m.type_result().cloned(),
                )
            })
            .collect();

        // Build import edges from the SemanticModuleGraph.
        let edges: Vec<ImportEdge> = semantic_graph
            .import_edges()
            .iter()
            .filter(|edge| reachable_modules.contains(&edge.from()))
            .filter_map(|edge| {
                let to = edge.to()?;
                Some(ImportEdge {
                    from: edge.from(),
                    to,
                })
            })
            .collect();

        // Build the transaction.
        let removed_modules: Vec<_> = base_graph
            .modules()
            .filter(|m| !reachable_modules.contains(&m.id()))
            .map(|m| m.id())
            .collect();

        let transaction = galfus_compiler::compile_transaction(
            &mut compiled_modules,
            &mut self.bytecode_state.compiler_state,
            &compilation_targets,
            base_graph.version(),
            semantic_revision,
            removed_modules,
            edges,
        )
        .map_err(|error| CompileBlocked::CompilerError(error.to_string()))?;

        let graph = Arc::new(
            base_graph
                .apply(transaction)
                .map_err(|error| CompileBlocked::CompilerError(error.to_string()))?,
        );
        self.bytecode_state.compile_state = CompileState::Ready {
            semantic_revision,
            graph: Arc::clone(&graph),
        };

        Ok(CompileReport { graph })
    }

    /// Starts the configured entry as a persistent execution.
    pub fn start_execution(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        driver: std::rc::Rc<dyn galfus_contract::KernelDriver>,
    ) -> Result<Execution, RunBlocked> {
        let graph = match &self.bytecode_state.compile_state {
            CompileState::Ready { graph, .. } => Arc::clone(graph),
            _ => return Err(RunBlocked::CompileRequired),
        };
        let entry_path = self
            .config
            .as_ref()
            .and_then(|config| config.entry.as_ref())
            .ok_or(RunBlocked::EntryModuleMissing)?;
        let entry_id = graph
            .modules()
            .find(|image| image.path() == entry_path)
            .map(|image| image.id())
            .ok_or(RunBlocked::EntryModuleMissing)?;
        let entry_name = self
            .config
            .as_ref()
            .expect("a successful compile requires configuration")
            .run_entry
            .clone();
        Runtime::new(graph.clone(), providers)
            .start(entry_id, entry_name.as_str(), args, driver.clone())
            .map_err(|error| {
                if let RuntimeError::VmPanic(panic) = &error {
                    RunBlocked::RuntimeError(format_panic(&graph, panic))
                } else {
                    RunBlocked::RuntimeError(error.to_string())
                }
            })
    }

    /// Compatibility helper that drives the returned execution through the supplied driver.
    pub fn run(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        driver: std::rc::Rc<dyn galfus_contract::KernelDriver>,
    ) -> Result<(), RunBlocked> {
        let mut execution = self.start_execution(args, providers, driver)?;
        let _ = execution.run_to_completion();

        Ok(())
    }
}
