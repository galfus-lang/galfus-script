use crate::{
    DelimiterKind, Token, TokenKind, TokenTree, TokenTreeDiagnosticCode, TokenTreeGroup,
    TokenTreeItem,
};
use galfus_core::{Diagnostic, DiagnosticBag};

#[derive(Debug, Clone)]
pub struct TokenTreeResult {
    tree: TokenTree,
    diagnostics: DiagnosticBag,
}

impl TokenTreeResult {
    pub fn new(tree: TokenTree, diagnostics: DiagnosticBag) -> Self {
        Self { tree, diagnostics }
    }

    pub fn tree(&self) -> &TokenTree {
        &self.tree
    }

    pub fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub fn into_parts(self) -> (TokenTree, DiagnosticBag) {
        (self.tree, self.diagnostics)
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}

pub fn build_token_tree(tokens: Vec<Token>) -> TokenTreeResult {
    TokenTreeBuilder::new(tokens).build()
}

struct TokenTreeBuilder {
    tokens: Vec<Token>,
    position: usize,
    diagnostics: DiagnosticBag,
}

impl TokenTreeBuilder {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn build(mut self) -> TokenTreeResult {
        let mut items = self.parse_items(None);

        if self
            .current()
            .is_some_and(|token| token.kind() == &TokenKind::Eof)
        {
            items.push(TokenTreeItem::Token(
                self.bump().expect("EOF token must exist"),
            ));
        }

        TokenTreeResult::new(TokenTree::new(items), self.diagnostics)
    }

    fn parse_items(&mut self, expected_closing: Option<DelimiterKind>) -> Vec<TokenTreeItem> {
        let mut items = Vec::new();

        while let Some(token) = self.current() {
            if token.kind() == &TokenKind::Eof {
                break;
            }

            if token.kind().is_opening_delimiter() {
                items.push(TokenTreeItem::Group(self.parse_group()));
                continue;
            }

            if token.kind().is_closing_delimiter() {
                if expected_closing.is_some() {
                    break;
                }

                let token = self.bump().expect("current token must exist");
                self.diagnostics.push(Diagnostic::error(
                    TokenTreeDiagnosticCode::UnexpectedClosingDelimiter,
                    token.span(),
                ));
                items.push(TokenTreeItem::Token(token));
                continue;
            }

            items.push(TokenTreeItem::Token(
                self.bump().expect("current token must exist"),
            ));
        }

        items
    }

    fn parse_group(&mut self) -> TokenTreeGroup {
        let opening = self.bump().expect("opening delimiter must exist");
        let delimiter = opening
            .kind()
            .delimiter_kind()
            .expect("opening delimiter must have a kind");
        let items = self.parse_items(Some(delimiter));
        let closing = self.consume_closing(delimiter);

        if closing.is_none() {
            self.diagnostics.push(Diagnostic::error(
                TokenTreeDiagnosticCode::UnclosedDelimiter,
                opening.span(),
            ));
        }

        TokenTreeGroup::new(delimiter, opening, items, closing)
    }

    fn consume_closing(&mut self, delimiter: DelimiterKind) -> Option<Token> {
        let token = self.current()?;

        if token.kind().delimiter_kind() != Some(delimiter) || !token.kind().is_closing_delimiter()
        {
            return None;
        }

        self.bump()
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position)?.clone();
        self.position += 1;
        Some(token)
    }
}
