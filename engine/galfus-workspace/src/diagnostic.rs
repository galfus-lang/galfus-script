use galfus_core::DiagnosticCodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceDiagnosticCode {
    MissingConfig,
    InvalidConfig,
    MissingModuleTable,
    MissingModuleName,
    MissingModuleTarget,
    InvalidModuleTarget,
    MissingAppEntry,
    MissingLibrarySurface,
    EntryTargetMissing,
    ExportTargetMissing,
    UnsupportedWorkspaceTarget,
}

impl DiagnosticCodeKind for WorkspaceDiagnosticCode {
    fn as_code(&self) -> &'static str {
        match self {
            Self::MissingConfig => "W0001",
            Self::InvalidConfig => "W0002",
            Self::MissingModuleTable => "W0003",
            Self::MissingModuleName => "W0004",
            Self::MissingModuleTarget => "W0005",
            Self::InvalidModuleTarget => "W0006",
            Self::MissingAppEntry => "W0007",
            Self::MissingLibrarySurface => "W0008",
            Self::EntryTargetMissing => "W0009",
            Self::ExportTargetMissing => "W0010",
            Self::UnsupportedWorkspaceTarget => "W0011",
        }
    }

    fn as_message(&self) -> &'static str {
        match self {
            Self::MissingConfig => "missing galfus.toml",
            Self::InvalidConfig => "invalid galfus.toml",
            Self::MissingModuleTable => "missing [module] table",
            Self::MissingModuleName => "missing module name",
            Self::MissingModuleTarget => "missing module target",
            Self::InvalidModuleTarget => "invalid module target",
            Self::MissingAppEntry => "missing app entry",
            Self::MissingLibrarySurface => "library requires entry or exports",
            Self::EntryTargetMissing => "entry target missing",
            Self::ExportTargetMissing => "export target missing",
            Self::UnsupportedWorkspaceTarget => "unsupported project target",
        }
    }
}
