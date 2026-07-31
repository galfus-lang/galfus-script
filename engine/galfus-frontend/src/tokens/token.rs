use super::TokenKind;
use galfus_core::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub const fn kind(&self) -> &TokenKind {
        &self.kind
    }

    pub const fn span(&self) -> Span {
        self.span
    }
}
