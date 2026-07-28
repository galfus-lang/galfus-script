use super::*;

impl Lexer<'_> {
    pub(super) fn keyword_kind(text: &str) -> Option<TokenKind> {
        let kind = match text {
            "_" => TokenKind::Underscore,

            "import" => TokenKind::Import,
            "from" => TokenKind::From,
            "export" => TokenKind::Export,
            "as" => TokenKind::As,
            "var" => TokenKind::Var,
            "const" => TokenKind::Const,
            "fn" => TokenKind::Fn,
            "return" => TokenKind::Return,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "choice" => TokenKind::Choice,
            "type" => TokenKind::Type,
            "constraint" => TokenKind::Constraint,
            "satisfies" => TokenKind::Satisfies,
            "match" => TokenKind::Match,
            "instanceof" => TokenKind::Instanceof,
            "typeof" => TokenKind::Typeof,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "loop" => TokenKind::Loop,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "weak" => TokenKind::Weak,
            "null" => TokenKind::Null,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "new" => TokenKind::New,
            "copy" => TokenKind::Copy,

            "self" => TokenKind::SelfKw,
            _ => return None,
        };

        Some(kind)
    }

    pub(super) fn is_identifier_extra(ch: char) -> bool {
        matches!(ch, '_' | '#' | '$')
    }

    pub(super) fn is_identifier_start(ch: char) -> bool {
        Self::is_identifier_extra(ch) || unicode_ident::is_xid_start(ch)
    }

    pub(super) fn is_identifier_continue(ch: char) -> bool {
        Self::is_identifier_extra(ch) || unicode_ident::is_xid_continue(ch)
    }

    pub(super) fn lex_identifier(&mut self, start: usize) -> TokenKind {
        while let Some(ch) = self.peek() {
            if !Self::is_identifier_continue(ch) {
                break;
            }

            self.bump();
        }

        let text = &self.text[start..self.offset];

        Self::keyword_kind(text).unwrap_or(TokenKind::Identifier)
    }
}
