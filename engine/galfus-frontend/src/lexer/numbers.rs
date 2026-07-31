use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NumberBase {
    Decimal,
    Hex,
    Binary,
    Octal,
}

impl Lexer<'_> {
    pub(super) fn is_decimal_digit(ch: char) -> bool {
        ch.is_ascii_digit()
    }

    pub(super) fn is_hex_digit(ch: char) -> bool {
        ch.is_ascii_hexdigit()
    }

    pub(super) fn is_binary_digit(ch: char) -> bool {
        ch == '0' || ch == '1'
    }

    pub(super) fn is_octal_digit(ch: char) -> bool {
        matches!(ch, '0'..='7')
    }

    pub(super) fn is_digit_for_base(ch: char, base: NumberBase) -> bool {
        match base {
            NumberBase::Decimal => Self::is_decimal_digit(ch),
            NumberBase::Hex => Self::is_hex_digit(ch),
            NumberBase::Binary => Self::is_binary_digit(ch),
            NumberBase::Octal => Self::is_octal_digit(ch),
        }
    }

    pub(super) fn lex_number(&mut self, start: usize) -> TokenKind {
        let (base, digits_start) = self.consume_number_prefix(start);

        while let Some(ch) = self.peek() {
            if Self::is_digit_for_base(ch, base) || ch == '_' {
                self.bump();
            } else {
                break;
            }
        }

        let mut kind = TokenKind::Integer;

        if base == NumberBase::Decimal && self.starts_float_fraction() {
            self.bump(); // .

            while let Some(ch) = self.peek() {
                if Self::is_decimal_digit(ch) || ch == '_' {
                    self.bump();
                } else {
                    break;
                }
            }

            kind = TokenKind::Float;
        }

        self.validate_number_separators(digits_start, self.offset, base);

        kind
    }

    pub(super) fn consume_number_prefix(&mut self, start: usize) -> (NumberBase, usize) {
        let after_first_digit = self.offset;

        if self.text.get(start..after_first_digit) != Some("0") {
            return (NumberBase::Decimal, start);
        }

        match self.peek() {
            Some('x') | Some('X') => {
                self.bump();
                (NumberBase::Hex, self.offset)
            }
            Some('b') | Some('B') => {
                self.bump();
                (NumberBase::Binary, self.offset)
            }
            Some('o') | Some('O') => {
                self.bump();
                (NumberBase::Octal, self.offset)
            }
            _ => (NumberBase::Decimal, start),
        }
    }

    pub(super) fn starts_float_fraction(&self) -> bool {
        if self.peek() != Some('.') {
            return false;
        }

        if self.peek_next() == Some('.') {
            return false;
        }

        matches!(self.peek_next(), Some(ch) if Self::is_decimal_digit(ch))
    }

    pub(super) fn validate_number_separators(
        &mut self,
        start: usize,
        end: usize,
        base: NumberBase,
    ) {
        let text = &self.text[start..end];

        let mut previous_was_digit = false;
        let mut previous_was_separator = false;

        for (relative_offset, ch) in text.char_indices() {
            if ch == '_' {
                let absolute_offset = start + relative_offset;

                if !previous_was_digit || previous_was_separator {
                    let span = Span::new(self.source.id(), absolute_offset, absolute_offset + 1);

                    self.diagnostics.push(Diagnostic::error(
                        LexicalDiagnosticCode::InvalidNumericSeparator,
                        span,
                    ));
                }

                previous_was_separator = true;
                previous_was_digit = false;
                continue;
            }

            if Self::is_digit_for_base(ch, base) {
                previous_was_digit = true;
                previous_was_separator = false;
                continue;
            }

            // Prefix chars like x/b/o are ignored here.
            previous_was_digit = false;
            previous_was_separator = false;
        }

        if previous_was_separator {
            let span = Span::new(self.source.id(), end - 1, end);

            self.diagnostics.push(Diagnostic::error(
                LexicalDiagnosticCode::InvalidNumericSeparator,
                span,
            ));
        }
    }
}
