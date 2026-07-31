use crate::source_store;

use galfus_bytecode::BytecodeGraph;
use galfus_compiler::CompilerState;
use galfus_core::{DiagnosticBag, ModuleId, Revision, SemanticRevision};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug)]
pub enum CheckState {
    Dirty {
        current_revision: Revision,
        previous_checked_revision: Option<Revision>,
    },
    Passed {
        revision: Revision,
        semantic_revision: SemanticRevision,
        changed_modules: HashSet<ModuleId>,
        diagnostics: DiagnosticBag,
    },
    Failed {
        revision: Revision,
        diagnostics: DiagnosticBag,
    },
}

impl CheckState {
    pub fn is_dirty(&self) -> bool {
        matches!(self, Self::Dirty { .. })
    }

    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Passed { .. })
    }
}

/// Reason why `Workspace::compile()` cannot proceed.
#[derive(Debug)]
pub enum CompileBlocked {
    /// Sources changed but `check()` has not been called yet.
    Dirty {
        current_revision: Revision,
        checked_revision: Option<Revision>,
    },
    /// The last `check()` produced errors — compilation is gated behind a clean check.
    CheckFailed {
        revision: Revision,
        error_count: usize,
    },
    /// No workspace configuration has been loaded.
    MissingConfiguration,
    /// The compiler itself returned an error.
    CompilerError(String),
}

/// Reason why `Workspace::run()` cannot proceed.
#[derive(Debug)]
pub enum RunBlocked {
    /// `compile()` has not produced an up-to-date compiled graph.
    CompileRequired,
    /// The configured entry module is not in the compiled graph.
    EntryModuleMissing,
    /// The runtime rejected loading, linking, or executing the compiled graph.
    RuntimeError(String),
}

pub struct SourceState {
    pub store: source_store::SourceStore,
    pub revision: Revision,
    pub dirty_sources: HashSet<galfus_core::ModulePath>,
    pub removed_modules: Vec<ModuleId>,
}

impl Default for SourceState {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceState {
    pub fn new() -> Self {
        Self {
            store: source_store::SourceStore::new(),
            revision: Revision::new(1),
            dirty_sources: HashSet::new(),
            removed_modules: Vec::new(),
        }
    }
}

pub struct SemanticState {
    pub check_state: CheckState,
}

impl Default for SemanticState {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticState {
    pub fn new() -> Self {
        Self {
            check_state: CheckState::Dirty {
                current_revision: Revision::new(1),
                previous_checked_revision: None,
            },
        }
    }
}

pub struct BytecodeState {
    pub compile_state: CompileState,
    pub compiler_state: CompilerState,
}

impl Default for BytecodeState {
    fn default() -> Self {
        Self::new()
    }
}

impl BytecodeState {
    pub fn new() -> Self {
        Self {
            compile_state: CompileState::Missing,
            compiler_state: CompilerState::default(),
        }
    }
}

#[derive(Debug)]
pub enum CompileState {
    /// No compilation has ever been attempted.
    Missing,
    /// A previous compiled graph exists but is stale (check result changed).
    Stale {
        semantic_revision: SemanticRevision,
        graph: Arc<BytecodeGraph>,
    },
    /// A compiled graph is available and up-to-date with the last check.
    Ready {
        semantic_revision: SemanticRevision,
        /// The compiled module graph produced by the last successful compile.
        graph: Arc<BytecodeGraph>,
    },
    /// The last compilation attempt failed.
    Failed {
        semantic_revision: SemanticRevision,
        error: String,
    },
}

impl CompileState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn graph(&self) -> Option<&Arc<BytecodeGraph>> {
        match self {
            Self::Ready { graph, .. } | Self::Stale { graph, .. } => Some(graph),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum WorkspaceError {
    InvalidPath,
    MissingConfiguration,
}
