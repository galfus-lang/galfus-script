use super::*;
use crate::SyntaxNodeKind;
use galfus_core::{NodeId, Span};

const METADATA_FLAGS: &[&str] = &["stamp", "after"];

impl Parser {
    pub(super) fn parse_optional_keyword_metadata(&mut self, is_loop: bool) -> Option<NodeId> {
        if !self.at(&TokenKind::LeftParen) {
            return None;
        }

        if is_loop {
            // For loops, we only treat it as metadata if the first item inside is a known flag
            // (stamp, after) or is a pair (starts with identifier followed by colon).
            let mut offset = 1;
            while self.peek(offset).kind() == &TokenKind::Newline {
                offset += 1;
            }
            if self.peek(offset).kind() == &TokenKind::Identifier {
                let text = self.token_text(self.peek(offset));
                let is_flag = text == "stamp" || text == "after";

                let mut next_offset = offset + 1;
                while self.peek(next_offset).kind() == &TokenKind::Newline {
                    next_offset += 1;
                }
                let is_pair = self.peek(next_offset).kind() == &TokenKind::Colon;

                if !is_flag && !is_pair {
                    return None;
                }
            } else {
                return None;
            }
        }

        let left = self.expect(TokenKind::LeftParen)?;
        self.skip_newlines();

        let mut items = Vec::new();

        while !self.is_eof() && !self.at(&TokenKind::RightParen) {
            let start_position = self.position;

            let item = self.parse_keyword_metadata_item();
            if let Some(item) = item {
                items.push(item);
            }

            self.skip_newlines();

            if self.at(&TokenKind::RightParen) {
                break;
            }

            if self.at(&TokenKind::Comma) {
                self.bump();
                self.skip_newlines();
                if self.at(&TokenKind::RightParen) {
                    break;
                }
                continue;
            }

            let found = self.current().clone();
            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::ExpectedToken,
                format!(
                    "expected `Comma` or `RightParen`, found `{:?}`",
                    found.kind()
                ),
                found.span(),
            ));

            if self.position == start_position {
                self.bump();
            }
        }

        let right = self.expect(TokenKind::RightParen)?;
        let span = Span::cover(left.span(), right.span()).unwrap_or(left.span());

        Some(self.add_node(SyntaxNodeKind::KeywordMetadataList, span, items))
    }

    pub(super) fn parse_keyword_metadata_item(&mut self) -> Option<NodeId> {
        self.skip_newlines();

        // 1) Pair: identifier : identifier (or value/expression)
        if self.at(&TokenKind::Identifier) {
            let mut offset = 1;
            while self.peek(offset).kind() == &TokenKind::Newline {
                offset += 1;
            }
            if self.peek(offset).kind() == &TokenKind::Colon {
                let key = self.parse_identifier()?;
                self.expect(TokenKind::Colon)?;
                self.skip_newlines();
                let value = self.parse_identifier()?;
                let span = Span::cover(self.node_span(key), self.node_span(value))
                    .unwrap_or_else(|| self.node_span(key));
                return Some(self.add_node(
                    SyntaxNodeKind::KeywordMetadataPair,
                    span,
                    vec![key, value],
                ));
            }
        }

        // 2) Flag: stamp, after
        if self.at(&TokenKind::Identifier) {
            let text = self.token_text(self.current());
            if METADATA_FLAGS.contains(&text) {
                let ident = self.parse_identifier()?;
                let span = self.node_span(ident);
                return Some(self.add_node(SyntaxNodeKind::KeywordMetadataFlag, span, vec![ident]));
            }
        }

        // 3) Type / value
        if let Some(ty) = self.parse_type() {
            let span = self.node_span(ty);
            return Some(self.add_node(SyntaxNodeKind::KeywordMetadataType, span, vec![ty]));
        }

        None
    }
}
