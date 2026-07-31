use super::*;

impl Lexer<'_> {
    pub(super) fn lex_string(&mut self, start: usize, quote: char) -> TokenKind {
        while let Some(ch) = self.peek() {
            if ch == quote {
                self.bump();
                return TokenKind::String;
            }

            if ch == '\n' || ch == '\r' {
                let span = Span::new(self.source.id(), start, self.offset);

                self.diagnostics.push(Diagnostic::error(
                    LexicalDiagnosticCode::UnterminatedStringLiteral,
                    span,
                ));

                return TokenKind::String;
            }

            self.bump();
        }

        let span = Span::new(self.source.id(), start, self.offset);

        self.diagnostics.push(Diagnostic::error(
            LexicalDiagnosticCode::UnterminatedStringLiteral,
            span,
        ));

        TokenKind::String
    }

    pub(super) fn lex_multiline_string(&mut self, start: usize) -> TokenKind {
        while let Some(ch) = self.peek() {
            if ch == '`' {
                self.bump();
                return TokenKind::String;
            }

            self.bump();
        }

        let span = Span::new(self.source.id(), start, self.offset);

        self.diagnostics.push(Diagnostic::error(
            LexicalDiagnosticCode::UnterminatedMultilineStringLiteral,
            span,
        ));

        TokenKind::String
    }
}
