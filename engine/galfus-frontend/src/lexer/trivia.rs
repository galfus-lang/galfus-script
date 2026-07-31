use super::*;

impl Lexer<'_> {
    pub(super) fn skip_trivia(&mut self) {
        loop {
            let start = self.offset;

            self.skip_whitespace();

            if self.starts_with("//") {
                self.skip_line_comment();
            } else if self.starts_with("/*") {
                self.skip_block_comment();
            }

            if self.offset == start {
                break;
            }
        }
    }

    pub(super) fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            match ch {
                // ASCII horizontal whitespace
                ' '          // U+0020 Space
                | '\t'       // U+0009 Horizontal Tab
                | '\u{000B}' // U+000B Vertical Tab
                | '\u{000C}' // U+000C Form Feed
                // Unicode non-breaking and special spaces
                | '\u{00A0}' // U+00A0 No-Break Space
                | '\u{1680}' // U+1680 Ogham Space Mark
                | '\u{2000}' // U+2000 En Quad
                | '\u{2001}' // U+2001 Em Quad
                | '\u{2002}' // U+2002 En Space
                | '\u{2003}' // U+2003 Em Space
                | '\u{2004}' // U+2004 Three-Per-Em Space
                | '\u{2005}' // U+2005 Four-Per-Em Space
                | '\u{2006}' // U+2006 Six-Per-Em Space
                | '\u{2007}' // U+2007 Figure Space
                | '\u{2008}' // U+2008 Punctuation Space
                | '\u{2009}' // U+2009 Thin Space
                | '\u{200A}' // U+200A Hair Space
                | '\u{202F}' // U+202F Narrow No-Break Space
                | '\u{205F}' // U+205F Medium Mathematical Space
                | '\u{3000}' // U+3000 Ideographic Space (CJK)
                | '\u{FEFF}' // U+FEFF BOM / Zero Width No-Break Space
                => {
                    self.bump();
                }

                _ => break,
            }
        }
    }

    pub(super) fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == '\n' || ch == '\r' {
                break;
            }

            self.bump();
        }
    }

    pub(super) fn skip_block_comment(&mut self) {
        if !self.starts_with("/*") {
            return;
        }

        let start = self.offset;

        self.bump(); // /
        self.bump(); // *

        while !self.is_eof() {
            if self.starts_with("*/") {
                self.bump(); // *
                self.bump(); // /
                return;
            }

            self.bump();
        }

        let span = Span::new(self.source.id(), start, self.offset);

        self.diagnostics.push(Diagnostic::error(
            LexicalDiagnosticCode::UnterminatedBlockComment,
            span,
        ));
    }
}
