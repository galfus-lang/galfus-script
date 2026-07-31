use super::LexResult;
use crate::{Token, TokenKind};
use galfus_core::{DiagnosticBag, SourceFile, Span};

pub struct Lexer<'a> {
    pub(super) source: &'a SourceFile,
    pub(super) text: &'a str,
    pub(super) offset: usize,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) previous_significant_kind: Option<TokenKind>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            text: source.text(),
            offset: 0,
            diagnostics: DiagnosticBag::new(),
            previous_significant_kind: None,
        }
    }

    pub fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }

    pub(super) fn make_token(&mut self, kind: TokenKind, span: Span) -> Token {
        if kind.is_significant() {
            self.previous_significant_kind = Some(kind.clone());
        }

        Token::new(kind, span)
    }
}

pub fn lex(source: &SourceFile) -> LexResult {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        let is_eof = token.kind() == &TokenKind::Eof;

        tokens.push(token);

        if is_eof {
            break;
        }
    }

    LexResult::new(tokens, lexer.into_diagnostics())
}
