#[cfg(test)]
mod tests;

use std::str;

use crate::config::{WORKSPACE_SOURCE_ID, WorkspaceConfig, parse_workspace_config};
use crate::diagnostic::WorkspaceDiagnosticCode;
use crate::source_store::{LoadModuleError, ModuleOrigin};
use crate::state::{
    BytecodeState, CheckState, CompileBlocked, CompileState, RunBlocked, SemanticState,
    SourceState, WorkspaceError,
};
use galfus_bytecode::{BytecodeGraph, ImportEdge, PackageEntryPoint, PackageImage};
use galfus_compiler::{CompiledModule, gfp::parse_gfp_frontmatter};
use galfus_contract::{
    AdapterFunctionSignature, AdapterModuleDescriptor, AdapterModuleRequirement, BoundaryType,
    CURRENT_BOUNDARY_ABI_VERSION, ExecutionTarget, ProviderFunctionSignature,
    ProviderModuleRequirement, Providers,
};
use galfus_core::{Diagnostic, DiagnosticBag, ModulePath, OpaqueTypeId, SourceFile, Span, TypeId};
use galfus_frontend::modules::{
    FrontendModuleKind, FrontendRoots, FrontendSession, FrontendSnapshot, FrontendSource,
    FrontendUpdate, SemanticRoot, SemanticRootKind,
};
use galfus_frontend::{
    PrimitiveType, ResolutionLayer, StringTable, SymbolKind, TypeKind, TypeTable,
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
    frontend_snapshot: Option<FrontendSnapshot>,
    pub catalog: Arc<galfus_contract::CapabilityCatalog>,
    pub adapter_descriptors: HashMap<ModulePath, AdapterModuleDescriptor>,
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
    /// The immutable compiled package, ready to be delivered to a host.
    pub package: Arc<PackageImage>,
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
            frontend_snapshot: None,
            catalog: Arc::new(galfus_contract::CapabilityCatalog::default()),
            adapter_descriptors: HashMap::new(),
        }
    }

    pub fn set_catalog(&mut self, catalog: Arc<galfus_contract::CapabilityCatalog>) {
        if self.catalog.fingerprint() != catalog.fingerprint() {
            self.source_state.revision.next();
            let removed = self
                .source_state
                .store
                .remove_by_origin(ModuleOrigin::ProviderCatalog);
            for entry in removed {
                self.source_state.dirty_sources.remove(&entry.path);
                self.source_state.removed_modules.push(entry.module_id);
            }
            self.catalog = catalog;
            self.mark_dirty();
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
        if self.catalog.is_provider_module(
            module_path
                .as_str()
                .strip_suffix(".gfs")
                .unwrap_or(module_path.as_str()),
        ) {
            return Err(WorkspaceError::ReservedProviderModule(path.to_string()));
        }
        if !path.ends_with(".gfp")
            && self
                .source_state
                .store
                .get(&module_path)
                .is_some_and(|entry| entry.bytes.as_ref() == module_bytes)
        {
            return Ok(LoadResult::Success);
        }

        let (source_bytes, origin, descriptor) = if path.ends_with(".gfp") {
            let source = match str::from_utf8(module_bytes) {
                Ok(source) => source,
                Err(_) => return Ok(Self::invalid_adapter_proxy(".gfp source must be UTF-8")),
            };
            let (frontmatter, body) = match parse_gfp_frontmatter(source) {
                Ok(parsed) => parsed,
                Err(error) => return Ok(Self::invalid_adapter_proxy(error)),
            };
            (
                Arc::from(body.as_bytes()),
                ModuleOrigin::AdapterProxy,
                Some(AdapterModuleDescriptor {
                    adapter: frontmatter.adapter,
                    config: frontmatter.config,
                    targets: frontmatter.targets,
                    exports: Vec::new(),
                }),
            )
        } else {
            (Arc::from(module_bytes), ModuleOrigin::User, None)
        };

        self.source_state.revision.next();
        self.source_state
            .store
            .load_module(
                module_path.clone(),
                source_bytes,
                origin,
                self.source_state.revision,
            )
            .map_err(|err| match err {
                LoadModuleError::Collision {
                    attempted,
                    existing,
                    identity,
                    id,
                } => WorkspaceError::Collision {
                    attempted: attempted.as_str().to_string(),
                    existing: existing.as_str().to_string(),
                    identity,
                    id,
                },
            })?;
        if let Some(descriptor) = descriptor {
            self.adapter_descriptors
                .insert(module_path.clone(), descriptor);
        } else {
            self.adapter_descriptors.remove(&module_path);
        }
        self.source_state.dirty_sources.insert(module_path);
        self.mark_dirty();
        Ok(LoadResult::Success)
    }

    pub fn register_bridge_module(
        &mut self,
        bridge: galfus_contract::BridgeModule,
    ) -> Result<LoadResult, WorkspaceError> {
        let module_path = ModulePath::new(&bridge.name).ok_or(WorkspaceError::InvalidPath)?;
        self.source_state.revision.next();
        self.source_state
            .store
            .load_module(
                module_path.clone(),
                Arc::from(bridge.source.as_bytes()),
                ModuleOrigin::Builtin,
                self.source_state.revision,
            )
            .map_err(|err| match err {
                LoadModuleError::Collision {
                    attempted,
                    existing,
                    identity,
                    id,
                } => WorkspaceError::Collision {
                    attempted: attempted.as_str().to_string(),
                    existing: existing.as_str().to_string(),
                    identity,
                    id,
                },
            })?;
        self.source_state.dirty_sources.insert(module_path);
        self.mark_dirty();
        Ok(LoadResult::Success)
    }

    pub fn remove_module(&mut self, path: &str) -> Result<RemoveResult, WorkspaceError> {
        let module_path = ModulePath::new(path).ok_or(WorkspaceError::InvalidPath)?;

        if let Some(entry) = self.source_state.store.remove_module(&module_path) {
            self.adapter_descriptors.remove(&module_path);
            self.source_state.revision.next();
            self.source_state.dirty_sources.remove(&module_path);
            self.source_state.removed_modules.push(entry.module_id);
            self.mark_dirty();
            Ok(RemoveResult::Success)
        } else {
            Ok(RemoveResult::NotFound)
        }
    }

    fn invalid_adapter_proxy(message: impl Into<String>) -> LoadResult {
        let mut diagnostics = DiagnosticBag::new();
        diagnostics.push(Diagnostic::error_with_message(
            WorkspaceDiagnosticCode::InvalidAdapterProxy,
            message,
            Span::empty(WORKSPACE_SOURCE_ID, 0),
        ));
        LoadResult::Diagnostics(diagnostics)
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
        self.frontend_snapshot = None;

        // Mark compile stale when check is invalidated.
        if let CompileState::Ready {
            semantic_revision,
            package,
        } = &self.bytecode_state.compile_state
        {
            self.bytecode_state.compile_state = CompileState::Stale {
                semantic_revision: *semantic_revision,
                package: Arc::clone(package),
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
                self.frontend_snapshot = None;
            } else {
                let roots = self.frontend_roots();
                let mut report = loop {
                    let source_files = {
                        let mut sorted_dirty: Vec<_> =
                            self.source_state.dirty_sources.iter().collect();
                        sorted_dirty.sort();
                        sorted_dirty
                    }
                    .into_iter()
                    .filter_map(|path| self.source_state.store.get(path))
                    .map(|entry| {
                        (
                            entry.module_id,
                            entry.path.clone(),
                            match entry.origin {
                                ModuleOrigin::User => FrontendModuleKind::Standard,
                                ModuleOrigin::Builtin => FrontendModuleKind::Builtin,
                                ModuleOrigin::AdapterProxy => FrontendModuleKind::AdapterProxy,
                                ModuleOrigin::ProviderCatalog => FrontendModuleKind::Builtin,
                            },
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
                        .map(|(module_id, path, kind, source)| FrontendSource {
                            module_id: *module_id,
                            path: path.clone(),
                            source,
                            kind: *kind,
                        })
                        .collect::<Vec<_>>();
                    let update = FrontendUpdate {
                        source_revision: self.source_state.revision,
                        sources: &sources,
                        removed_modules: self.source_state.removed_modules.as_slice(),
                        roots: &roots,
                        catalog: Arc::clone(&self.catalog),
                    };
                    let mut report = self.frontend.check(update);
                    self.source_state.dirty_sources.clear();
                    self.source_state.removed_modules.clear();

                    match self.load_required_dependencies(&report.required_dependencies) {
                        Ok(false) => break report,
                        Ok(true) => continue,
                        Err(WorkspaceError::Collision {
                            attempted,
                            existing,
                            identity,
                            id,
                        }) => {
                            report.diagnostics.push(Diagnostic::error_with_message(
                                WorkspaceDiagnosticCode::ModuleCollision,
                                format!(
                                    "Cannot load builtin module '{}' because {} {} collides with existing module '{}'",
                                    attempted,
                                    identity.label(),
                                    id,
                                    existing
                                ),
                                Span::empty(galfus_core::SourceId::new(0), 0),
                            ));
                            break report;
                        }
                        Err(_) => break report,
                    }
                };

                self.refresh_adapter_proxy_descriptors(&mut report.diagnostics);
                self.validate_registered_adapter_schemas(&mut report.diagnostics);

                if report.diagnostics.has_errors() {
                    self.semantic_state.check_state = CheckState::Failed {
                        revision: report.source_revision,
                        diagnostics: report.diagnostics,
                    };
                    self.frontend_snapshot = None;
                } else {
                    self.frontend_snapshot = Some(self.frontend.snapshot(report.semantic_revision));
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

    fn load_required_dependencies(&mut self, paths: &[ModulePath]) -> Result<bool, WorkspaceError> {
        let mut loaded = false;
        for path in paths {
            if self.source_state.store.get(path).is_some() {
                continue;
            }
            let builtin_name = path.as_str().strip_suffix(".gfs").unwrap_or(path.as_str());
            let (source, origin) = if let Some((_, source)) = galfus_contract::BUILTIN_MODULES
                .iter()
                .find(|(name, _)| *name == builtin_name)
            {
                (*source, ModuleOrigin::Builtin)
            } else if let Some(source) = self.catalog.provider_source(builtin_name) {
                (source, ModuleOrigin::ProviderCatalog)
            } else {
                continue;
            };
            self.source_state.revision.next();
            self.source_state
                .store
                .load_module(
                    path.clone(),
                    Arc::from(source.as_bytes()),
                    origin,
                    self.source_state.revision,
                )
                .map_err(|err| match err {
                    LoadModuleError::Collision {
                        attempted,
                        existing,
                        identity,
                        id,
                    } => WorkspaceError::Collision {
                        attempted: attempted.as_str().to_string(),
                        existing: existing.as_str().to_string(),
                        identity,
                        id,
                    },
                })?;
            self.source_state.dirty_sources.insert(path.clone());
            loaded = true;
        }
        Ok(loaded)
    }

    fn validate_registered_adapter_schemas(&self, diagnostics: &mut DiagnosticBag) {
        for descriptor in self.adapter_descriptors.values() {
            let Some(adapter) = self.catalog.adapter_schema(&descriptor.adapter) else {
                diagnostics.push(Diagnostic::error_with_message(
                    WorkspaceDiagnosticCode::InvalidAdapterProxy,
                    format!("unresolved external adapter '{}'", descriptor.adapter),
                    Span::empty(WORKSPACE_SOURCE_ID, 0),
                ));
                continue;
            };
            if let Err(error) = adapter.validate_schema(descriptor) {
                diagnostics.push(Diagnostic::error_with_message(
                    WorkspaceDiagnosticCode::InvalidAdapterProxy,
                    error.to_string(),
                    Span::empty(WORKSPACE_SOURCE_ID, 0),
                ));
            }
        }
    }

    fn refresh_adapter_proxy_descriptors(&mut self, diagnostics: &mut DiagnosticBag) {
        for module in self.frontend.modules() {
            if module.kind() != FrontendModuleKind::AdapterProxy {
                continue;
            }
            let Some(descriptor) = self.adapter_descriptors.get_mut(module.path()) else {
                continue;
            };
            descriptor.exports.clear();
            let Some(type_result) = module.type_result() else {
                continue;
            };
            let Some(resolution) = module.graph().resolution() else {
                continue;
            };
            for export in resolution.exports() {
                if export.kind() != SymbolKind::Function {
                    continue;
                }
                let Some(function_type) = type_result
                    .layer()
                    .symbol_type(export.symbol())
                    .and_then(|ty| type_result.layer().table().kind(ty))
                    .and_then(|kind| match kind {
                        TypeKind::Function(function) => Some(function),
                        _ => None,
                    })
                else {
                    continue;
                };
                let proxy_name = module
                    .path()
                    .as_str()
                    .strip_suffix(".gfp")
                    .unwrap_or(module.path().as_str());
                let parameter_types = function_type
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        Self::boundary_type(
                            type_result.layer().table(),
                            resolution,
                            self.frontend.string_table(),
                            proxy_name,
                            parameter.ty(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>();
                let return_type = Self::boundary_type(
                    type_result.layer().table(),
                    resolution,
                    self.frontend.string_table(),
                    proxy_name,
                    function_type.return_type(),
                );
                match parameter_types
                    .and_then(|parameters| return_type.map(|return_type| (parameters, return_type)))
                {
                    Ok((parameter_types, return_type)) => {
                        descriptor.exports.push(AdapterFunctionSignature {
                            name: export.name().to_string(),
                            is_async: true,
                            parameter_types,
                            return_type,
                        })
                    }
                    Err(error) => diagnostics.push(Diagnostic::error_with_message(
                        WorkspaceDiagnosticCode::InvalidAdapterProxy,
                        format!(
                            "proxy export '{}' is not boundary-representable: {error}",
                            export.name()
                        ),
                        module.source().span(),
                    )),
                }
            }
        }
    }

    fn boundary_type(
        table: &TypeTable,
        resolution: &ResolutionLayer,
        string_table: &StringTable,
        proxy_name: &str,
        ty: TypeId,
    ) -> Result<BoundaryType, String> {
        match table.kind(ty) {
            Some(TypeKind::Primitive(primitive)) => match primitive {
                PrimitiveType::Null => Ok(BoundaryType::Null),
                PrimitiveType::Bool => Ok(BoundaryType::Bool),
                PrimitiveType::Int8 => Ok(BoundaryType::I8),
                PrimitiveType::Int16 => Ok(BoundaryType::I16),
                PrimitiveType::Int32 => Ok(BoundaryType::I32),
                PrimitiveType::Int64 => Ok(BoundaryType::I64),
                PrimitiveType::Uint8 => Ok(BoundaryType::U8),
                PrimitiveType::Uint16 => Ok(BoundaryType::U16),
                PrimitiveType::Uint32 => Ok(BoundaryType::U32),
                PrimitiveType::Uint64 => Ok(BoundaryType::U64),
                PrimitiveType::Float32 => Ok(BoundaryType::F32),
                PrimitiveType::Float64 => Ok(BoundaryType::F64),
                PrimitiveType::Float16 => {
                    Err("f16 is not supported by the boundary ABI".to_string())
                }
            },
            Some(TypeKind::Named { symbol }) => {
                if let Some(symbol_data) = resolution.symbol(*symbol) {
                    if symbol_data.kind() == SymbolKind::Struct {
                        let name = string_table.resolve(symbol_data.name()).ok_or_else(|| {
                            "struct name is missing from the string table".to_string()
                        })?;
                        return Ok(BoundaryType::Handle {
                            type_id: OpaqueTypeId::new(proxy_name, name)
                                .expect("adapter proxy types have a module path and name"),
                        });
                    }
                }
                Err("named type is not supported by the boundary ABI".to_string())
            }
            Some(TypeKind::Array { element }) => Ok(BoundaryType::Array(Box::new(
                Self::boundary_type(table, resolution, string_table, proxy_name, *element)?,
            ))),
            Some(TypeKind::Tuple { elements }) => elements
                .iter()
                .map(|element| {
                    Self::boundary_type(table, resolution, string_table, proxy_name, *element)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(BoundaryType::Tuple),
            Some(TypeKind::Function(_)) => Ok(BoundaryType::Function),
            Some(TypeKind::GenericInstance { arguments, .. }) if arguments.len() == 1 => {
                Self::boundary_type(table, resolution, string_table, proxy_name, arguments[0])
            }
            Some(TypeKind::Union { members }) => {
                let non_null = members
                    .iter()
                    .filter(|member| {
                        !matches!(
                            table.kind(**member),
                            Some(TypeKind::Primitive(PrimitiveType::Null))
                        )
                    })
                    .copied()
                    .collect::<Vec<_>>();
                if non_null.len() == 1 && non_null.len() + 1 == members.len() {
                    Ok(BoundaryType::Nullable(Box::new(Self::boundary_type(
                        table,
                        resolution,
                        string_table,
                        proxy_name,
                        non_null[0],
                    )?)))
                } else {
                    Err("only nullable unions are supported by the boundary ABI".to_string())
                }
            }
            _ => Err("type is not supported by the boundary ABI".to_string()),
        }
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
        let frontend_snapshot = self
            .frontend_snapshot
            .as_ref()
            .expect("passed frontend check has a snapshot");
        debug_assert_eq!(frontend_snapshot.semantic_revision(), semantic_revision);

        // Skip recompilation if already up-to-date.
        if let CompileState::Ready {
            semantic_revision: compiled_rev,
            package,
        } = &self.bytecode_state.compile_state
            && *compiled_rev == semantic_revision
        {
            return Ok(CompileReport {
                package: Arc::clone(package),
            });
        }

        let cached_graph = match &self.bytecode_state.compile_state {
            CompileState::Stale { package, .. } => Some(package.graph()),
            _ => None,
        };
        let empty_graph = BytecodeGraph::new();
        let base_graph = cached_graph.unwrap_or(&empty_graph);

        // The first compilation has no graph to upsert into, so it must emit
        // every semantic module even if the last frontend delta was narrower.
        let compilation_targets = if let Some(cached_graph) = cached_graph {
            frontend_snapshot
                .modules()
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
            frontend_snapshot
                .modules()
                .iter()
                .map(|module| module.id())
                .collect::<HashSet<_>>()
        };

        let semantic_graph = frontend_snapshot.semantic_graph();
        let mut reachable_modules = HashSet::new();

        // Build CompiledModule list from the frontend's semantic modules.
        let semantic_modules = frontend_snapshot.modules();

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
                    m.source().name().ends_with(".gfp"),
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
            frontend_snapshot.string_table(),
            base_graph.version(),
            semantic_revision,
            removed_modules,
            edges,
        )
        .map_err(|error| CompileBlocked::CompilerError(error.to_string()))?;

        let graph = base_graph
            .apply(transaction)
            .map_err(|error| CompileBlocked::CompilerError(error.to_string()))?;
        let adapter_requirements = self.adapter_requirements_for(&graph);
        let provider_requirements = self.provider_requirements_for(&graph);
        let entry_point = self.config.as_ref().and_then(|config| {
            config
                .entry
                .clone()
                .map(|entry| PackageEntryPoint::new(entry, config.run_entry.clone()))
        });
        let package = Arc::new(
            PackageImage::try_new(
                graph,
                self.config
                    .as_ref()
                    .map(WorkspaceConfig::execution_target)
                    .cloned()
                    .unwrap_or_else(|| {
                        ExecutionTarget::new("default").expect("default target is valid")
                    }),
                entry_point,
                adapter_requirements,
                provider_requirements,
            )
            .map_err(|error| CompileBlocked::CompilerError(error.to_string()))?,
        );
        self.bytecode_state.compile_state = CompileState::Ready {
            semantic_revision,
            package: Arc::clone(&package),
        };

        Ok(CompileReport { package })
    }

    fn adapter_requirements_for(&self, graph: &BytecodeGraph) -> Vec<AdapterModuleRequirement> {
        let mut requirements = graph
            .modules()
            .filter_map(|module| {
                self.adapter_descriptors
                    .get(module.path())
                    .cloned()
                    .map(|descriptor| AdapterModuleRequirement {
                        proxy_module: module.path().as_str().to_string(),
                        descriptor,
                        boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
                    })
            })
            .collect::<Vec<_>>();
        requirements.sort_by(|left, right| left.proxy_module.cmp(&right.proxy_module));
        requirements
    }

    fn provider_requirements_for(&self, graph: &BytecodeGraph) -> Vec<ProviderModuleRequirement> {
        graph
            .modules()
            .filter_map(|module| {
                self.catalog
                    .provider_schema_fingerprint(module.path().as_str())
                    .map(|schema_fingerprint| ProviderModuleRequirement {
                        module_path: module.path().as_str().to_string(),
                        schema_fingerprint,
                        boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
                        exports: self.provider_exports_for(module.path()),
                    })
            })
            .collect()
    }

    fn provider_exports_for(&self, path: &ModulePath) -> Vec<ProviderFunctionSignature> {
        let Some(module) = self
            .frontend
            .modules()
            .iter()
            .find(|module| module.path() == path)
        else {
            return Vec::new();
        };
        let Some(type_result) = module.type_result() else {
            return Vec::new();
        };
        let Some(resolution) = module.graph().resolution() else {
            return Vec::new();
        };
        let table = type_result.layer().table();
        let mut exports = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::Function)
            .filter_map(|symbol| {
                let name = self.frontend.string_table().resolve(symbol.name())?;
                let name = name.strip_prefix("__provider_")?;
                let TypeKind::Function(function) =
                    table.kind(type_result.layer().symbol_type(symbol.id())?)?
                else {
                    return None;
                };
                let parameter_types = function
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        Self::boundary_type(
                            table,
                            resolution,
                            self.frontend.string_table(),
                            path.as_str(),
                            parameter.ty(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                let return_type = Self::boundary_type(
                    table,
                    resolution,
                    self.frontend.string_table(),
                    path.as_str(),
                    function.return_type(),
                )
                .ok()?;
                Some(ProviderFunctionSignature {
                    name: name.to_string(),
                    parameter_types,
                    return_type,
                })
            })
            .collect::<Vec<_>>();
        exports.sort();
        exports
    }

    /// Starts the configured entry as a persistent execution.
    pub fn start_execution(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        driver: std::rc::Rc<dyn galfus_contract::KernelDriver>,
    ) -> Result<Execution, RunBlocked> {
        let package = match &self.bytecode_state.compile_state {
            CompileState::Ready { package, .. } => Arc::clone(package),
            _ => return Err(RunBlocked::CompileRequired),
        };
        Runtime::new(Arc::clone(&package), providers)
            .start(args, driver.clone())
            .map_err(|error| {
                if let RuntimeError::VmPanic(panic) = &error {
                    RunBlocked::RuntimeError(format_panic(package.graph(), panic))
                } else {
                    RunBlocked::RuntimeError(error.to_string())
                }
            })
    }

    pub fn start_execution_with_bindings(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        bindings: galfus_contract::AdapterBindings,
        driver: std::rc::Rc<dyn galfus_contract::KernelDriver>,
    ) -> Result<Execution, RunBlocked> {
        let package = match &self.bytecode_state.compile_state {
            CompileState::Ready { package, .. } => Arc::clone(package),
            _ => return Err(RunBlocked::CompileRequired),
        };
        Runtime::new(Arc::clone(&package), providers)
            .with_adapter_bindings(bindings)
            .start(args, driver.clone())
            .map_err(|error| {
                if let RuntimeError::VmPanic(panic) = &error {
                    RunBlocked::RuntimeError(format_panic(package.graph(), panic))
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
        let _result = execution.run_to_completion();
        Ok(())
    }

    pub fn run_with_bindings(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        bindings: galfus_contract::AdapterBindings,
        driver: std::rc::Rc<dyn galfus_contract::KernelDriver>,
    ) -> Result<(), RunBlocked> {
        let mut execution =
            self.start_execution_with_bindings(args, providers, bindings, driver)?;
        let _result = execution.run_to_completion();
        Ok(())
    }
}
