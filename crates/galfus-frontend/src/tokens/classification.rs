use super::TokenKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DelimiterKind {
    Parenthesis,
    Brace,
    Bracket,
}

impl TokenKind {
    pub const fn delimiter_kind(&self) -> Option<DelimiterKind> {
        match self {
            Self::LeftParen | Self::RightParen => Some(DelimiterKind::Parenthesis),
            Self::LeftBrace | Self::RightBrace => Some(DelimiterKind::Brace),
            Self::LeftBracket | Self::RightBracket => Some(DelimiterKind::Bracket),
            _ => None,
        }
    }

    pub const fn is_opening_delimiter(&self) -> bool {
        matches!(self, Self::LeftParen | Self::LeftBrace | Self::LeftBracket)
    }

    pub const fn is_closing_delimiter(&self) -> bool {
        matches!(
            self,
            Self::RightParen | Self::RightBrace | Self::RightBracket
        )
    }

    pub const fn is_keyword(&self) -> bool {
        matches!(
            self,
            Self::Import
                | Self::From
                | Self::Export
                | Self::As
                | Self::Var
                | Self::Const
                | Self::Fn
                | Self::Return
                | Self::Struct
                | Self::Enum
                | Self::Choice
                | Self::Type
                | Self::Constraint
                | Self::Satisfies
                | Self::Match
                | Self::Instanceof
                | Self::Typeof
                | Self::If
                | Self::Else
                | Self::For
                | Self::In
                | Self::Loop
                | Self::Break
                | Self::Continue
                | Self::Weak
                | Self::Null
                | Self::True
                | Self::False
                | Self::New
                | Self::Copy
                | Self::SelfKw
        )
    }

    pub const fn is_significant(&self) -> bool {
        !matches!(self, Self::Eof | Self::Newline | Self::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::{DelimiterKind, TokenKind};

    #[test]
    fn classifies_current_language_tokens() {
        assert_eq!(
            TokenKind::LeftParen.delimiter_kind(),
            Some(DelimiterKind::Parenthesis)
        );
        assert!(TokenKind::RightBrace.is_closing_delimiter());
        assert!(TokenKind::Typeof.is_keyword());

        assert!(TokenKind::Identifier.is_significant());
        assert!(!TokenKind::Newline.is_significant());
    }
}
