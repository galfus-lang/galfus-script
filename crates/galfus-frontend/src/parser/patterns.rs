use super::*;
use crate::{ParserDiagnosticCode, SyntaxNodeKind, TokenKind};
use galfus_core::{Diagnostic, NodeId, Span};

impl Parser {
    pub(super) fn parse_pattern(&mut self) -> Option<NodeId> {
        if self.at(&TokenKind::Integer)
            || self.at(&TokenKind::Float)
            || self.at(&TokenKind::String)
            || self.at(&TokenKind::True)
            || self.at(&TokenKind::False)
            || self.at(&TokenKind::Null)
        {
            return self.parse_literal_pattern();
        }

        if self.at(&TokenKind::Underscore) {
            return self.parse_wildcard_pattern();
        }

        if self.at(&TokenKind::Identifier) {
            // Check if this is a struct pattern: if there is a `{` after the type part.
            let mut offset = 1;
            let mut depth = 0;
            let is_struct_pattern = loop {
                let kind = self.peek(offset).kind();
                match kind {
                    TokenKind::Newline => {
                        offset += 1;
                    }
                    TokenKind::Less => {
                        depth += 1;
                        offset += 1;
                    }
                    TokenKind::Greater => {
                        if depth > 0 {
                            depth -= 1;
                        }
                        offset += 1;
                    }
                    TokenKind::LeftBrace if depth == 0 => {
                        break self.is_struct_pattern_start(offset);
                    }
                    TokenKind::Identifier
                    | TokenKind::ColonColon
                    | TokenKind::Comma
                    | TokenKind::Integer
                    | TokenKind::Float
                    | TokenKind::String
                    | TokenKind::True
                    | TokenKind::False => {
                        offset += 1;
                    }
                    _ => {
                        break false;
                    }
                }
            };

            if is_struct_pattern {
                return self.parse_struct_pattern();
            }

            let mut is_generic_variant = false;
            let mut next_idx = self.position + 1;
            while next_idx < self.tokens.len()
                && self.tokens[next_idx].kind() == &TokenKind::Newline
            {
                next_idx += 1;
            }
            if next_idx < self.tokens.len() && self.tokens[next_idx].kind() == &TokenKind::Less {
                let saved_position = self.position;
                self.position = next_idx;
                if self.can_parse_generic_call_suffix() {
                    is_generic_variant = true;
                }
                self.position = saved_position;
            }

            if is_generic_variant
                || matches!(self.peek_after_newlines(1).kind(), TokenKind::ColonColon)
            {
                return self.parse_variant_pattern();
            }

            return self.parse_binding_pattern();
        }

        let found = self.bump();

        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::UnexpectedToken,
            format!("expected pattern, found `{:?}`", found.kind()),
            found.span(),
        ));

        None
    }

    fn is_struct_pattern_start(&self, brace_offset: usize) -> bool {
        let mut offset = brace_offset + 1;
        while self.peek(offset).kind() == &TokenKind::Newline {
            offset += 1;
        }

        match self.peek(offset).kind() {
            TokenKind::RightBrace | TokenKind::DotDotDot => true,
            TokenKind::Identifier => {
                offset += 1;
                while self.peek(offset).kind() == &TokenKind::Newline {
                    offset += 1;
                }

                matches!(
                    self.peek(offset).kind(),
                    TokenKind::Colon | TokenKind::Comma | TokenKind::RightBrace
                )
            }
            _ => false,
        }
    }

    pub(super) fn parse_literal_pattern(&mut self) -> Option<NodeId> {
        let literal = if self.at(&TokenKind::Integer) {
            self.parse_integer_literal()?
        } else if self.at(&TokenKind::Float) {
            self.parse_float_literal()?
        } else if self.at(&TokenKind::String) {
            self.parse_string_literal()?
        } else if self.at(&TokenKind::True) || self.at(&TokenKind::False) {
            self.parse_bool_literal()?
        } else if self.at(&TokenKind::Null) {
            self.parse_null_literal()?
        } else {
            let found = self.bump();

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::UnexpectedToken,
                format!("expected literal pattern, found `{:?}`", found.kind()),
                found.span(),
            ));

            return None;
        };

        let span = self.node_span(literal);

        Some(self.add_node(SyntaxNodeKind::LiteralPattern, span, vec![literal]))
    }

    pub(super) fn parse_binding_pattern(&mut self) -> Option<NodeId> {
        self.skip_newlines();

        if self.at(&TokenKind::Underscore) {
            return self.parse_wildcard_pattern();
        }

        let inner = if self.at(&TokenKind::LeftBrace) {
            self.parse_struct_binding_pattern()?
        } else if self.at(&TokenKind::LeftParen) {
            self.parse_tuple_binding_pattern()?
        } else if self.at(&TokenKind::LeftBracket) {
            self.parse_array_binding_pattern()?
        } else {
            self.parse_identifier()?
        };

        let span = self.node_span(inner);

        Some(self.add_node(SyntaxNodeKind::BindingPattern, span, vec![inner]))
    }

    pub(super) fn parse_variant_pattern_payload(&mut self) -> Option<NodeId> {
        let left = self.expect(TokenKind::LeftParen)?;

        let mut patterns = Vec::new();

        self.skip_newlines();

        while !self.is_eof() && !self.at(&TokenKind::RightParen) {
            let start_position = self.position;

            if let Some(pattern) = self.parse_pattern() {
                patterns.push(pattern);
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
                format!("expected `Comma`, found `{:?}`", found.kind()),
                found.span(),
            ));

            if self.position == start_position {
                self.bump();
            }
        }

        let right = self.expect(TokenKind::RightParen)?;

        let span = Span::cover(left.span(), right.span()).unwrap_or(left.span());

        Some(self.add_node(SyntaxNodeKind::VariantPatternPayload, span, patterns))
    }

    pub(super) fn parse_variant_pattern(&mut self) -> Option<NodeId> {
        let identifier = self.parse_identifier()?;
        let span = self.node_span(identifier);
        let mut target = self.add_node(SyntaxNodeKind::NameExpression, span, vec![identifier]);

        loop {
            self.skip_newlines();

            if self.at(&TokenKind::Less) && self.can_parse_generic_call_suffix() {
                target = self.parse_generic_expression(target)?;
                continue;
            }

            if self.at(&TokenKind::ColonColon) {
                let mut offset = 1;
                while self.peek(offset).kind() == &TokenKind::Newline {
                    offset += 1;
                }
                if self.peek(offset).kind() == &TokenKind::Identifier {
                    offset += 1;
                    while self.peek(offset).kind() == &TokenKind::Newline {
                        offset += 1;
                    }
                    let next_next = self.peek(offset).kind();
                    if next_next == &TokenKind::ColonColon || next_next == &TokenKind::Less {
                        self.bump();
                        let member = self.parse_identifier()?;
                        let span = Span::cover(self.node_span(target), self.node_span(member))
                            .unwrap_or_else(|| self.node_span(target));
                        target = self.add_node(
                            SyntaxNodeKind::PathExpression,
                            span,
                            vec![target, member],
                        );
                        continue;
                    }
                }
            }

            break;
        }

        self.expect(TokenKind::ColonColon)?;

        self.skip_newlines();

        let variant = self.parse_identifier()?;

        let mut children = vec![target, variant];
        let mut end_span = self.node_span(variant);

        if self.at(&TokenKind::LeftParen) {
            let payload = self.parse_variant_pattern_payload()?;
            end_span = self.node_span(payload);
            children.push(payload);
        }

        let span =
            Span::cover(self.node_span(target), end_span).unwrap_or_else(|| self.node_span(target));

        Some(self.add_node(SyntaxNodeKind::VariantPattern, span, children))
    }

    pub(super) fn parse_match_arm(&mut self) -> Option<NodeId> {
        let pattern = self.parse_pattern()?;

        self.skip_newlines();

        let body = self.parse_arm_body()?;

        let span = Span::cover(self.node_span(pattern), self.node_span(body))
            .unwrap_or_else(|| self.node_span(pattern));

        Some(self.add_node(SyntaxNodeKind::MatchArm, span, vec![pattern, body]))
    }

    pub(super) fn parse_type_pattern_binding(&mut self, parenthesized: bool) -> Option<NodeId> {
        let left = if parenthesized {
            Some(self.expect(TokenKind::LeftParen)?)
        } else {
            None
        };
        self.skip_newlines();

        let name = self.parse_identifier()?;

        self.skip_newlines();

        let span = if let Some(left) = left {
            let right = self.expect(TokenKind::RightParen)?;

            Span::cover(left.span(), right.span()).unwrap_or(left.span())
        } else {
            self.node_span(name)
        };

        Some(self.add_node(SyntaxNodeKind::TypePatternBinding, span, vec![name]))
    }

    pub(super) fn parse_type_pattern(&mut self) -> Option<NodeId> {
        let pattern_type = self.parse_type()?;

        let mut children = vec![pattern_type];
        let mut end_span = self.node_span(pattern_type);

        self.skip_newlines();

        if self.at(&TokenKind::Identifier) {
            let binding = self.parse_type_pattern_binding(false)?;
            end_span = self.node_span(binding);
            children.push(binding);
        }

        let span = Span::cover(self.node_span(pattern_type), end_span)
            .unwrap_or_else(|| self.node_span(pattern_type));

        Some(self.add_node(SyntaxNodeKind::TypePattern, span, children))
    }

    pub(super) fn parse_instanceof_pattern(&mut self) -> Option<NodeId> {
        if self.at(&TokenKind::Underscore) {
            return self.parse_wildcard_pattern();
        }

        if self.at(&TokenKind::Identifier) {
            let mut offset = 1;
            let mut depth = 0;
            let is_struct_pattern = loop {
                let kind = self.peek(offset).kind();
                match kind {
                    TokenKind::Newline => {
                        offset += 1;
                    }
                    TokenKind::Less => {
                        depth += 1;
                        offset += 1;
                    }
                    TokenKind::Greater => {
                        if depth > 0 {
                            depth -= 1;
                        }
                        offset += 1;
                    }
                    TokenKind::LeftBrace if depth == 0 => {
                        break self.is_struct_pattern_start(offset);
                    }
                    TokenKind::Identifier
                    | TokenKind::ColonColon
                    | TokenKind::Comma
                    | TokenKind::Integer
                    | TokenKind::Float
                    | TokenKind::String
                    | TokenKind::True
                    | TokenKind::False => {
                        offset += 1;
                    }
                    _ => {
                        break false;
                    }
                }
            };

            if is_struct_pattern {
                return self.parse_struct_pattern();
            }

            if matches!(
                self.peek_after_newlines(1).kind(),
                TokenKind::Arrow | TokenKind::LeftBrace
            ) {
                return self.parse_binding_pattern();
            }
        }

        self.parse_type_pattern()
    }

    pub(super) fn parse_arm_body(&mut self) -> Option<NodeId> {
        if self.at(&TokenKind::Arrow) {
            self.bump();
            self.skip_newlines();

            if self.at(&TokenKind::LeftBrace) {
                let block = self.current().span();
                self.graph.push_diagnostic(Diagnostic::error_with_message(
                    ParserDiagnosticCode::ArrowMustIntroduceExpression,
                    "a block arm must follow its pattern directly; remove `=>`".to_string(),
                    block,
                ));
                let _ = self.parse_block();
                return None;
            }

            let expression = self.parse_expression()?;
            let span = self.node_span(expression);
            return Some(self.add_node(SyntaxNodeKind::ExpressionArmBody, span, vec![expression]));
        }

        if self.at(&TokenKind::LeftBrace) {
            let block = self.parse_block()?;
            let span = self.node_span(block);
            return Some(self.add_node(SyntaxNodeKind::BlockArmBody, span, vec![block]));
        }

        let found = self.current().clone();
        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::ExpectedToken,
            format!(
                "expected `Arrow` or `LeftBrace`, found `{:?}`",
                found.kind()
            ),
            found.span(),
        ));
        None
    }

    pub(super) fn parse_struct_binding_pattern(&mut self) -> Option<NodeId> {
        let open = self.expect(TokenKind::LeftBrace)?;

        self.skip_newlines();

        let mut fields = Vec::new();

        while !self.at(&TokenKind::RightBrace) && !self.at(&TokenKind::Eof) {
            self.skip_newlines();

            if self.at(&TokenKind::RightBrace) {
                break;
            }

            let field = self.parse_struct_binding_field()?;
            fields.push(field);

            self.skip_newlines();

            if self.at(&TokenKind::Comma) {
                self.bump();
                self.skip_newlines();
                continue;
            }

            break;
        }

        let close = self.expect(TokenKind::RightBrace)?;

        let span = Span::cover(open.span(), close.span()).unwrap_or(open.span());

        Some(self.add_node(SyntaxNodeKind::StructBindingPattern, span, fields))
    }

    pub(super) fn parse_struct_binding_field(&mut self) -> Option<NodeId> {
        self.skip_newlines();

        let name = self.parse_identifier()?;
        let mut children = vec![name];
        let mut end_span = self.node_span(name);

        self.skip_newlines();

        if self.at(&TokenKind::Colon) {
            self.bump();
            self.skip_newlines();

            let alias = self.parse_binding_pattern()?;
            end_span = self.node_span(alias);
            children.push(alias);
        }

        let span =
            Span::cover(self.node_span(name), end_span).unwrap_or_else(|| self.node_span(name));

        Some(self.add_node(SyntaxNodeKind::StructBindingField, span, children))
    }

    pub(super) fn parse_tuple_binding_pattern(&mut self) -> Option<NodeId> {
        let open = self.expect(TokenKind::LeftParen)?;

        self.skip_newlines();

        let mut elements = Vec::new();

        while !self.at(&TokenKind::RightParen) && !self.at(&TokenKind::Eof) {
            self.skip_newlines();

            if self.at(&TokenKind::RightParen) {
                break;
            }

            let element = self.parse_binding_pattern()?;
            elements.push(element);

            self.skip_newlines();

            if self.at(&TokenKind::Comma) {
                self.bump();
                self.skip_newlines();
                continue;
            }

            break;
        }

        let close = self.expect(TokenKind::RightParen)?;

        let span = Span::cover(open.span(), close.span()).unwrap_or(open.span());

        Some(self.add_node(SyntaxNodeKind::TupleBindingPattern, span, elements))
    }

    pub(super) fn parse_array_binding_pattern(&mut self) -> Option<NodeId> {
        let open = self.expect(TokenKind::LeftBracket)?;

        self.skip_newlines();

        let mut elements = Vec::new();

        while !self.at(&TokenKind::RightBracket) && !self.at(&TokenKind::Eof) {
            self.skip_newlines();

            if self.at(&TokenKind::RightBracket) {
                break;
            }

            let element = if self.at(&TokenKind::DotDotDot) {
                self.parse_rest_binding_pattern()?
            } else {
                self.parse_binding_pattern()?
            };

            elements.push(element);

            self.skip_newlines();

            if self.at(&TokenKind::Comma) {
                self.bump();
                self.skip_newlines();
                continue;
            }

            break;
        }

        let close = self.expect(TokenKind::RightBracket)?;

        let span = Span::cover(open.span(), close.span()).unwrap_or(open.span());

        Some(self.add_node(SyntaxNodeKind::ArrayBindingPattern, span, elements))
    }

    pub(super) fn parse_rest_binding_pattern(&mut self) -> Option<NodeId> {
        let spread = self.expect(TokenKind::DotDotDot)?;

        self.skip_newlines();

        let pattern = self.parse_binding_pattern()?;

        let span = Span::cover(spread.span(), self.node_span(pattern)).unwrap_or(spread.span());

        Some(self.add_node(SyntaxNodeKind::RestBindingPattern, span, vec![pattern]))
    }

    pub(super) fn parse_wildcard_pattern(&mut self) -> Option<NodeId> {
        let token = self.expect(TokenKind::Underscore)?;

        Some(self.add_node(SyntaxNodeKind::WildcardPattern, token.span(), Vec::new()))
    }

    pub(super) fn parse_struct_pattern(&mut self) -> Option<NodeId> {
        let type_node = self.parse_type()?;
        self.skip_newlines();

        let _open = self.expect(TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();

        while !self.at(&TokenKind::RightBrace) && !self.at(&TokenKind::Eof) {
            self.skip_newlines();

            if self.at(&TokenKind::RightBrace) {
                break;
            }

            let field = self.parse_struct_pattern_field()?;
            fields.push(field);

            self.skip_newlines();

            if self.at(&TokenKind::Comma) {
                self.bump();
                self.skip_newlines();
                continue;
            }

            break;
        }

        let close = self.expect(TokenKind::RightBrace)?;

        let span = Span::cover(self.node_span(type_node), close.span())
            .unwrap_or(self.node_span(type_node));

        let mut children = vec![type_node];
        children.extend(fields);

        Some(self.add_node(SyntaxNodeKind::StructPattern, span, children))
    }

    pub(super) fn parse_struct_pattern_field(&mut self) -> Option<NodeId> {
        self.skip_newlines();

        let name = self.parse_identifier()?;
        let mut children = vec![name];
        let mut end_span = self.node_span(name);

        self.skip_newlines();

        if self.at(&TokenKind::Colon) {
            self.bump();
            self.skip_newlines();

            let value_pattern = self.parse_pattern()?;
            end_span = self.node_span(value_pattern);
            children.push(value_pattern);
        }

        let span =
            Span::cover(self.node_span(name), end_span).unwrap_or_else(|| self.node_span(name));

        Some(self.add_node(SyntaxNodeKind::StructPatternField, span, children))
    }
}
