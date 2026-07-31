use galfus_core::DiagnosticCodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenTreeDiagnosticCode {
    UnclosedDelimiter,
    UnexpectedClosingDelimiter,
}

impl DiagnosticCodeKind for TokenTreeDiagnosticCode {
    fn as_code(&self) -> &'static str {
        match self {
            Self::UnclosedDelimiter => "B0001",
            Self::UnexpectedClosingDelimiter => "B0002",
        }
    }

    fn as_message(&self) -> &'static str {
        match self {
            Self::UnclosedDelimiter => "unclosed delimiter",
            Self::UnexpectedClosingDelimiter => "unexpected closing delimiter",
        }
    }
}
