use galfus_core::DiagnosticCodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckDiagnosticCode {
    ImportModuleNotFound,
    MissingExport,
    UnsupportedImportTarget,
    BodylessFunction,
    InvalidAdapterProxyDeclaration,
}

impl DiagnosticCodeKind for CheckDiagnosticCode {
    fn as_code(&self) -> &'static str {
        match self {
            Self::ImportModuleNotFound => "C0001",
            Self::MissingExport => "C0002",
            Self::UnsupportedImportTarget => "C0003",
            Self::BodylessFunction => "C0004",
            Self::InvalidAdapterProxyDeclaration => "C0005",
        }
    }

    fn as_message(&self) -> &'static str {
        match self {
            Self::ImportModuleNotFound => "import module not found",
            Self::MissingExport => "missing export",
            Self::UnsupportedImportTarget => "unsupported import target",
            Self::BodylessFunction => "body-less function outside external proxy",
            Self::InvalidAdapterProxyDeclaration => "invalid external proxy declaration",
        }
    }
}
