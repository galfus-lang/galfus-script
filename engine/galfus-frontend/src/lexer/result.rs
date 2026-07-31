use crate::Token;
use galfus_core::DiagnosticBag;

#[derive(Debug, Clone)]
pub struct LexResult {
    tokens: Vec<Token>,
    diagnostics: DiagnosticBag,
}

impl LexResult {
    pub fn new(tokens: Vec<Token>, diagnostics: DiagnosticBag) -> Self {
        Self {
            tokens,
            diagnostics,
        }
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub fn into_parts(self) -> (Vec<Token>, DiagnosticBag) {
        (self.tokens, self.diagnostics)
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}
