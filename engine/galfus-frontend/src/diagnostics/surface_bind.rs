use galfus_core::DiagnosticCodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolverDiagnosticCode {
    DuplicateSymbol,
    UnresolvedName,
    UnresolvedType,
    InvalidFunctionAnchor,
}

impl DiagnosticCodeKind for ResolverDiagnosticCode {
    fn as_code(&self) -> &'static str {
        match self {
            Self::DuplicateSymbol => "S0001",
            Self::UnresolvedName => "S0002",
            Self::UnresolvedType => "S0003",
            Self::InvalidFunctionAnchor => "S0004",
        }
    }

    fn as_message(&self) -> &'static str {
        match self {
            Self::DuplicateSymbol => "duplicate symbol",
            Self::UnresolvedName => "unresolved name",
            Self::UnresolvedType => "unresolved type",
            Self::InvalidFunctionAnchor => "invalid function anchor",
        }
    }
}
