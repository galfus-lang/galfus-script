use galfus_core::DiagnosticCodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LexicalDiagnosticCode {
    UnterminatedBlockComment,
    UnterminatedStringLiteral,
    UnterminatedMultilineStringLiteral,
    UnknownCharacter,
    InvalidNumericSeparator,
}

impl DiagnosticCodeKind for LexicalDiagnosticCode {
    fn as_code(&self) -> &'static str {
        match self {
            Self::UnterminatedBlockComment => "L0001",
            Self::UnterminatedStringLiteral => "L0002",
            Self::UnterminatedMultilineStringLiteral => "L0003",
            Self::UnknownCharacter => "L0004",
            Self::InvalidNumericSeparator => "L0005",
        }
    }

    fn as_message(&self) -> &'static str {
        match self {
            Self::UnterminatedBlockComment => "unterminated block comment",
            Self::UnterminatedStringLiteral => "unterminated string literal",
            Self::UnterminatedMultilineStringLiteral => "unterminated multiline string literal",
            Self::UnknownCharacter => "unknown character",
            Self::InvalidNumericSeparator => "invalid numeric separator",
        }
    }
}
