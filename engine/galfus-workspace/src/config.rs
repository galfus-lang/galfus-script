use crate::diagnostic::WorkspaceDiagnosticCode;
use galfus_core::{Diagnostic, DiagnosticBag, ModulePath, SourceId, Span};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const WORKSPACE_SOURCE_ID: SourceId = SourceId::new(u32::MAX);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleTarget {
    App,
    Lib,
}

impl ModuleTarget {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "app" => Some(Self::App),
            "lib" => Some(Self::Lib),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceExport {
    address: String,
    path: ModulePath,
}

impl WorkspaceExport {
    pub fn address(&self) -> &str {
        self.address.as_str()
    }

    pub fn path(&self) -> &ModulePath {
        &self.path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceConfig {
    name: String,
    target: ModuleTarget,
    pub(super) entry: Option<ModulePath>,
    pub(super) run_entry: String,
    pub(super) run_args: Vec<String>,
    exports: Vec<WorkspaceExport>,
}

impl WorkspaceConfig {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn target(&self) -> ModuleTarget {
        self.target
    }

    pub fn entry(&self) -> Option<&ModulePath> {
        self.entry.as_ref()
    }

    pub fn run_entry(&self) -> &str {
        self.run_entry.as_str()
    }

    pub fn run_args(&self) -> &[String] {
        self.run_args.as_slice()
    }

    pub fn exports(&self) -> &[WorkspaceExport] {
        self.exports.as_slice()
    }
}

#[derive(Debug, Deserialize)]
struct RawWorkspaceConfig {
    module: Option<RawModuleConfig>,
    run: Option<RawRunConfig>,

    #[serde(default)]
    exports: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct RawModuleConfig {
    name: Option<String>,
    target: Option<String>,
    entry: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawRunConfig {
    entry: Option<String>,
    args: Option<Vec<String>>,
}

pub(super) fn parse_workspace_config(
    text: &str,
    diagnostics: &mut DiagnosticBag,
) -> Option<WorkspaceConfig> {
    let raw = match toml::from_str::<RawWorkspaceConfig>(text) {
        Ok(raw) => raw,
        Err(error) => {
            diagnostics.push(Diagnostic::error_with_message(
                WorkspaceDiagnosticCode::InvalidConfig,
                format!("invalid galfus.toml: {error}"),
                workspace_span(),
            ));
            return None;
        }
    };

    let Some(module) = raw.module else {
        diagnostics.push(Diagnostic::error(
            WorkspaceDiagnosticCode::MissingModuleTable,
            workspace_span(),
        ));
        return None;
    };

    let Some(name) = module.name else {
        diagnostics.push(Diagnostic::error(
            WorkspaceDiagnosticCode::MissingModuleName,
            workspace_span(),
        ));
        return None;
    };

    let Some(target_text) = module.target else {
        diagnostics.push(Diagnostic::error(
            WorkspaceDiagnosticCode::MissingModuleTarget,
            workspace_span(),
        ));
        return None;
    };

    let Some(target) = ModuleTarget::parse(target_text.as_str()) else {
        diagnostics.push(Diagnostic::error_with_message(
            WorkspaceDiagnosticCode::InvalidModuleTarget,
            format!("invalid module target `{target_text}`"),
            workspace_span(),
        ));
        return None;
    };

    let entry = match module.entry {
        Some(entry_str) => match ModulePath::new(&entry_str) {
            Some(path) => Some(path),
            None => {
                diagnostics.push(Diagnostic::error_with_message(
                    WorkspaceDiagnosticCode::UnsupportedWorkspaceTarget,
                    format!("entry must point to a .gfs source file: `{entry_str}`"),
                    workspace_span(),
                ));
                None
            }
        },
        None => None,
    };

    let run_entry = raw
        .run
        .as_ref()
        .and_then(|run| run.entry.clone())
        .unwrap_or_else(|| "main".to_string());

    let run_args = raw
        .run
        .as_ref()
        .and_then(|run| run.args.clone())
        .unwrap_or_default();

    let mut exports = Vec::new();
    for (address, path_str) in raw.exports {
        match ModulePath::new(&path_str) {
            Some(path) => {
                exports.push(WorkspaceExport { address, path });
            }
            None => {
                diagnostics.push(Diagnostic::error_with_message(
                    WorkspaceDiagnosticCode::UnsupportedWorkspaceTarget,
                    format!("export `{address}` must point to a .gfs source file: `{path_str}`"),
                    workspace_span(),
                ));
            }
        }
    }

    validate_workspace_surface(target, entry.as_ref(), exports.as_slice(), diagnostics);

    if diagnostics.has_errors() {
        return None;
    }

    Some(WorkspaceConfig {
        name,
        target,
        entry,
        run_entry,
        run_args,
        exports,
    })
}

fn validate_workspace_surface(
    target: ModuleTarget,
    entry: Option<&ModulePath>,
    exports: &[WorkspaceExport],
    diagnostics: &mut DiagnosticBag,
) {
    match target {
        ModuleTarget::App => {
            if entry.is_none() {
                diagnostics.push(Diagnostic::error(
                    WorkspaceDiagnosticCode::MissingAppEntry,
                    workspace_span(),
                ));
            }
        }
        ModuleTarget::Lib => {
            if entry.is_none() && exports.is_empty() {
                diagnostics.push(Diagnostic::error(
                    WorkspaceDiagnosticCode::MissingLibrarySurface,
                    workspace_span(),
                ));
            }
        }
    }
}

pub(super) fn workspace_span() -> Span {
    Span::empty(WORKSPACE_SOURCE_ID, 0)
}
