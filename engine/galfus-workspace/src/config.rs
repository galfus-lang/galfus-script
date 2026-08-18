#[cfg(test)]
mod tests;

use crate::diagnostic::WorkspaceDiagnosticCode;
use galfus_contract::{ExecutionTarget, LimitsMetadata};
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
    version: Option<String>,
    author: Option<String>,
    email: Option<String>,
    description: Option<String>,
    target: ModuleTarget,
    pub(super) entry: Option<ModulePath>,
    pub(super) run_entry: String,
    pub(super) run_args: Vec<String>,
    exports: Vec<WorkspaceExport>,
    compile_target: ExecutionTarget,
    compile_arch: String,
    compile_profile: String,
    limits: LimitsMetadata,
}

impl WorkspaceConfig {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
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

    pub fn compile_target(&self) -> &ExecutionTarget {
        &self.compile_target
    }

    pub fn compile_arch(&self) -> &str {
        self.compile_arch.as_str()
    }

    pub fn compile_profile(&self) -> &str {
        self.compile_profile.as_str()
    }

    pub fn limits(&self) -> &LimitsMetadata {
        &self.limits
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct WorkspaceManifest {
    pub module: Option<ModuleManifest>,
    pub entry: Option<EntryManifest>,
    pub compile: Option<CompileManifest>,
    pub limits: Option<LimitsManifest>,

    #[serde(default)]
    pub exports: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ModuleManifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub email: Option<String>,
    pub description: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct EntryManifest {
    pub path: Option<String>,
    pub function: Option<String>,
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CompileManifest {
    pub target: Option<String>,
    pub arch: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct LimitsManifest {
    pub max_heap_objects: Option<usize>,
    pub max_heap_bytes: Option<usize>,
    pub max_call_depth: Option<usize>,
    pub max_threads: Option<usize>,
    pub max_futures: Option<usize>,
    pub max_pending_requests: Option<usize>,
    pub max_mailbox_messages: Option<usize>,
    pub max_mailbox_bytes: Option<usize>,
    pub max_event_queue: Option<usize>,
    pub max_kernel_tasks: Option<usize>,
    pub max_runnable_threads: Option<usize>,
    pub max_external_handles: Option<usize>,
    pub max_timers: Option<usize>,
    pub max_pending_states: Option<usize>,
}

macro_rules! apply_limit {
    ($raw:ident, $limits:ident, $field:ident, $diagnostics:ident) => {
        if let Some(val) = $raw.$field {
            if val == 0 {
                $diagnostics.push(Diagnostic::error_with_message(
                    WorkspaceDiagnosticCode::InvalidLimit,
                    concat!("limit `", stringify!($field), "` cannot be 0"),
                    workspace_span(),
                ));
            } else {
                $limits.$field = val;
            }
        }
    };
}

pub(super) fn parse_workspace_config(
    raw: WorkspaceManifest,
    diagnostics: &mut DiagnosticBag,
) -> Option<WorkspaceConfig> {
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

    let entry = match raw.entry.as_ref().and_then(|e| e.path.as_deref()) {
        Some(entry_str) => match ModulePath::new(entry_str) {
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
        .entry
        .as_ref()
        .and_then(|run| run.function.clone())
        .unwrap_or_else(|| "main".to_string());

    let run_args = raw
        .entry
        .as_ref()
        .and_then(|run| run.args.clone())
        .unwrap_or_default();

    let compile_target_text = raw
        .compile
        .as_ref()
        .and_then(|c| c.target.as_deref())
        .unwrap_or("default");
    let Some(compile_target) = ExecutionTarget::new(compile_target_text) else {
        diagnostics.push(Diagnostic::error_with_message(
            WorkspaceDiagnosticCode::InvalidConfig,
            "compile target must not be empty",
            workspace_span(),
        ));
        return None;
    };

    let compile_arch = raw
        .compile
        .as_ref()
        .and_then(|c| c.arch.clone())
        .unwrap_or_else(|| "x64".to_string());

    let compile_profile = raw
        .compile
        .as_ref()
        .and_then(|c| c.profile.clone())
        .unwrap_or_else(|| "debug".to_string());

    let mut limits = LimitsMetadata::default();
    if let Some(raw_limits) = raw.limits {
        apply_limit!(raw_limits, limits, max_heap_objects, diagnostics);
        apply_limit!(raw_limits, limits, max_heap_bytes, diagnostics);
        apply_limit!(raw_limits, limits, max_call_depth, diagnostics);
        apply_limit!(raw_limits, limits, max_threads, diagnostics);
        apply_limit!(raw_limits, limits, max_futures, diagnostics);
        apply_limit!(raw_limits, limits, max_pending_requests, diagnostics);
        apply_limit!(raw_limits, limits, max_mailbox_messages, diagnostics);
        apply_limit!(raw_limits, limits, max_mailbox_bytes, diagnostics);
        apply_limit!(raw_limits, limits, max_event_queue, diagnostics);
        apply_limit!(raw_limits, limits, max_kernel_tasks, diagnostics);
        apply_limit!(raw_limits, limits, max_runnable_threads, diagnostics);
        apply_limit!(raw_limits, limits, max_external_handles, diagnostics);
        apply_limit!(raw_limits, limits, max_timers, diagnostics);
        apply_limit!(raw_limits, limits, max_pending_states, diagnostics);
    }

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
        version: module.version,
        author: module.author,
        email: module.email,
        description: module.description,
        target,
        entry,
        run_entry,
        run_args,
        exports,
        compile_target,
        compile_arch,
        compile_profile,
        limits,
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
