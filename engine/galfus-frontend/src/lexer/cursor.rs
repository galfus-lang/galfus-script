use super::*;

impl Lexer<'_> {
    pub(super) fn is_eof(&self) -> bool {
        self.offset >= self.text.len()
    }

    pub(super) fn current_text(&self) -> &str {
        &self.text[self.offset..]
    }

    pub(super) fn peek(&self) -> Option<char> {
        self.current_text().chars().next()
    }

    pub(super) fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    pub(super) fn peek_next(&self) -> Option<char> {
        let mut chars = self.current_text().chars();
        chars.next()?;
        chars.next()
    }

    pub(super) fn match_char(&mut self, expected: char) -> bool {
        if self.peek() != Some(expected) {
            return false;
        }

        self.bump();
        true
    }

    pub(super) fn starts_with(&self, text: &str) -> bool {
        self.current_text().starts_with(text)
    }
}
