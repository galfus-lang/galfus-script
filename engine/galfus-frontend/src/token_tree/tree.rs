use crate::{DelimiterKind, Token};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenTree {
    items: Vec<TokenTreeItem>,
}

impl TokenTree {
    pub fn new(items: Vec<TokenTreeItem>) -> Self {
        Self { items }
    }

    pub fn items(&self) -> &[TokenTreeItem] {
        &self.items
    }

    pub fn into_items(self) -> Vec<TokenTreeItem> {
        self.items
    }

    pub fn into_tokens(self) -> Vec<Token> {
        let mut tokens = Vec::new();

        for item in self.items {
            item.append_tokens(&mut tokens);
        }

        tokens
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenTreeItem {
    Token(Token),
    Group(TokenTreeGroup),
}

impl TokenTreeItem {
    pub fn token(&self) -> Option<&Token> {
        match self {
            Self::Token(token) => Some(token),
            Self::Group(_) => None,
        }
    }

    pub fn group(&self) -> Option<&TokenTreeGroup> {
        match self {
            Self::Token(_) => None,
            Self::Group(group) => Some(group),
        }
    }

    fn append_tokens(self, tokens: &mut Vec<Token>) {
        match self {
            Self::Token(token) => tokens.push(token),
            Self::Group(group) => group.append_tokens(tokens),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenTreeGroup {
    delimiter: DelimiterKind,
    opening: Token,
    items: Vec<TokenTreeItem>,
    closing: Option<Token>,
}

impl TokenTreeGroup {
    pub fn new(
        delimiter: DelimiterKind,
        opening: Token,
        items: Vec<TokenTreeItem>,
        closing: Option<Token>,
    ) -> Self {
        Self {
            delimiter,
            opening,
            items,
            closing,
        }
    }

    pub const fn delimiter(&self) -> DelimiterKind {
        self.delimiter
    }

    pub const fn opening(&self) -> &Token {
        &self.opening
    }

    pub fn items(&self) -> &[TokenTreeItem] {
        &self.items
    }

    pub fn closing(&self) -> Option<&Token> {
        self.closing.as_ref()
    }

    fn append_tokens(self, tokens: &mut Vec<Token>) {
        tokens.push(self.opening);

        for item in self.items {
            item.append_tokens(tokens);
        }

        if let Some(closing) = self.closing {
            tokens.push(closing);
        }
    }
}
