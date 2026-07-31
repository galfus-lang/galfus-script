#[cfg(test)]
mod tests;

use crate::Span;

pub trait DiagnosticCodeKind {
    fn as_code(&self) -> &'static str;
    fn as_message(&self) -> &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    span: Span,
}

impl Diagnostic {
    pub fn new(
        severity: DiagnosticSeverity,
        code: DiagnosticCode,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            span,
        }
    }

    pub fn error(code: impl DiagnosticCodeKind, span: Span) -> Self {
        Self::new(
            DiagnosticSeverity::Error,
            DiagnosticCode::new(code.as_code()),
            code.as_message(),
            span,
        )
    }

    pub fn warning(code: impl DiagnosticCodeKind, span: Span) -> Self {
        Self::new(
            DiagnosticSeverity::Warning,
            DiagnosticCode::new(code.as_code()),
            code.as_message(),
            span,
        )
    }

    pub fn info(code: impl DiagnosticCodeKind, span: Span) -> Self {
        Self::new(
            DiagnosticSeverity::Info,
            DiagnosticCode::new(code.as_code()),
            code.as_message(),
            span,
        )
    }

    pub fn hint(code: impl DiagnosticCodeKind, span: Span) -> Self {
        Self::new(
            DiagnosticSeverity::Hint,
            DiagnosticCode::new(code.as_code()),
            code.as_message(),
            span,
        )
    }

    pub fn error_with_message(
        code: impl DiagnosticCodeKind,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::new(
            DiagnosticSeverity::Error,
            DiagnosticCode::new(code.as_code()),
            message,
            span,
        )
    }

    pub fn warning_with_message(
        code: impl DiagnosticCodeKind,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::new(
            DiagnosticSeverity::Warning,
            DiagnosticCode::new(code.as_code()),
            message,
            span,
        )
    }

    pub fn info_with_message(
        code: impl DiagnosticCodeKind,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::new(
            DiagnosticSeverity::Info,
            DiagnosticCode::new(code.as_code()),
            message,
            span,
        )
    }

    pub fn hint_with_message(
        code: impl DiagnosticCodeKind,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::new(
            DiagnosticSeverity::Hint,
            DiagnosticCode::new(code.as_code()),
            message,
            span,
        )
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    pub fn code(&self) -> &DiagnosticCode {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }
}

#[derive(Debug, Default, Clone)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter()
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}
