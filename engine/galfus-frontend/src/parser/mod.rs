mod advanced_expressions;
mod decorators;
mod expressions;
mod helpers;
mod items;
mod lists;
mod literals;
mod metadata;
mod patterns;
mod start;
mod statements;
mod syntax;
mod syntax_types;
#[cfg(test)]
mod tests;

use crate::{
    ModuleAst, OperatorKind, ParserDiagnosticCode, SyntaxNodeKind, Token, TokenKind,
    build_token_tree, lex,
};
use galfus_core::{Diagnostic, DiagnosticBag, NodeId, SourceFile, Span};

#[derive(Debug, Clone)]
pub struct ParseResult {
    graph: ModuleAst,
}

impl ParseResult {
    pub fn new(graph: ModuleAst) -> Self {
        Self { graph }
    }

    pub fn ast(&self) -> &ModuleAst {
        &self.graph
    }

    pub fn into_ast(self) -> ModuleAst {
        self.graph
    }

    pub fn graph(&self) -> &ModuleAst {
        self.ast()
    }

    pub fn into_graph(self) -> ModuleAst {
        self.into_ast()
    }

    pub fn diagnostics(&self) -> &DiagnosticBag {
        self.graph.diagnostics()
    }

    pub fn has_errors(&self) -> bool {
        self.graph.has_errors()
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
    graph: ModuleAst,
    source_text: String,
}

impl Parser {
    pub fn new(source: &SourceFile, tokens: Vec<Token>, diagnostics: DiagnosticBag) -> Self {
        let mut graph = ModuleAst::new(source.id());

        graph.extend_diagnostics(diagnostics.into_vec());

        Self {
            tokens,
            position: 0,
            graph,
            source_text: source.text().to_string(),
        }
    }

    fn token_text(&self, token: &Token) -> &str {
        let span = token.span();
        &self.source_text[span.start()..span.end()]
    }

    fn node_text(&self, node: NodeId) -> &str {
        let span = self.node_span(node);
        &self.source_text[span.start()..span.end()]
    }

    pub fn finish(mut self) -> ParseResult {
        self.graph.syntax_mut().set_tokens(self.tokens);

        ParseResult::new(self.graph)
    }

    fn peek(&self, offset: usize) -> &Token {
        let index = self.position.saturating_add(offset);

        self.tokens
            .get(index)
            .unwrap_or_else(|| self.tokens.last().expect("lexer must emit EOF token"))
    }

    fn current(&self) -> &Token {
        self.peek(0)
    }

    fn is_eof(&self) -> bool {
        self.current().kind() == &TokenKind::Eof
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.current().kind() == kind
    }

    fn bump(&mut self) -> Token {
        let token = self.current().clone();

        if !self.is_eof() {
            self.position += 1;
        }

        token
    }

    fn expect(&mut self, expected: TokenKind) -> Option<Token> {
        if self.at(&expected) {
            return Some(self.bump());
        }

        let found = self.current();

        let message = format!("expected `{:?}`, found `{:?}`", expected, found.kind(),);

        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::ExpectedToken,
            message,
            found.span(),
        ));

        None
    }

    fn skip_newlines(&mut self) {
        while self.at(&TokenKind::Newline) {
            self.bump();
        }
    }

    fn skip_soft_newlines_before_expression_continuation(&mut self) -> bool {
        if !self.at(&TokenKind::Newline) {
            return false;
        }

        let next = self.peek_after_newlines(0);

        if !Self::can_continue_expression_after_newline(next.kind()) {
            return false;
        }

        self.skip_newlines();

        true
    }

    fn add_node(&mut self, kind: SyntaxNodeKind, span: Span, children: Vec<NodeId>) -> NodeId {
        self.graph.syntax_mut().add_node(kind, span, children)
    }

    fn add_operator_node(
        &mut self,
        kind: SyntaxNodeKind,
        span: Span,
        operator: OperatorKind,
    ) -> NodeId {
        self.graph
            .syntax_mut()
            .add_operator_node(kind, span, operator)
    }

    fn node_span(&self, id: NodeId) -> Span {
        self.graph
            .syntax()
            .node(id)
            .expect("node id must exist")
            .span()
    }

    fn peek_after_newlines(&self, start_offset: usize) -> &Token {
        let mut offset = start_offset;

        while self.peek(offset).kind() == &TokenKind::Newline {
            offset += 1;
        }

        self.peek(offset)
    }
}

pub fn parse(source: &SourceFile) -> ParseResult {
    let lex_result = lex(source);
    let (tokens, mut diagnostics) = lex_result.into_parts();
    let token_tree_result = build_token_tree(tokens);
    let (tree, token_tree_diagnostics) = token_tree_result.into_parts();

    diagnostics.extend(token_tree_diagnostics.into_vec());

    let mut parser = Parser::new(source, tree.into_tokens(), diagnostics);

    parser.parse_source_file();
    parser.finish()
}
