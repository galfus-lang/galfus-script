use super::*;

impl Parser {
    pub(super) fn parse_integer_literal(&mut self) -> Option<NodeId> {
        let token = self.expect(TokenKind::Integer)?;

        Some(self.add_node(SyntaxNodeKind::IntegerLiteral, token.span(), Vec::new()))
    }

    pub(super) fn parse_float_literal(&mut self) -> Option<NodeId> {
        let token = self.expect(TokenKind::Float)?;

        Some(self.add_node(SyntaxNodeKind::FloatLiteral, token.span(), Vec::new()))
    }

    pub(super) fn parse_bool_literal(&mut self) -> Option<NodeId> {
        let token = if self.at(&TokenKind::True) {
            self.bump()
        } else {
            self.expect(TokenKind::False)?
        };

        Some(self.add_node(SyntaxNodeKind::BoolLiteral, token.span(), Vec::new()))
    }

    pub(super) fn parse_null_literal(&mut self) -> Option<NodeId> {
        let token = self.expect(TokenKind::Null)?;

        Some(self.add_node(SyntaxNodeKind::NullLiteral, token.span(), Vec::new()))
    }

    pub(super) fn parse_string_literal(&mut self) -> Option<NodeId> {
        let token = self.expect(TokenKind::String)?;

        Some(self.add_node(SyntaxNodeKind::StringLiteral, token.span(), Vec::new()))
    }

    pub(super) fn parse_array_element(&mut self) -> Option<NodeId> {
        if self.at(&TokenKind::DotDotDot) {
            return self.parse_spread_array_element();
        }

        let expression = self.parse_expression()?;
        let span = self.node_span(expression);

        Some(self.add_node(SyntaxNodeKind::ArrayElement, span, vec![expression]))
    }

    pub(super) fn parse_array_literal(&mut self) -> Option<NodeId> {
        let left = self.expect(TokenKind::LeftBracket)?;

        let mut elements = Vec::new();

        self.skip_newlines();

        while !self.is_eof() && !self.at(&TokenKind::RightBracket) {
            let start_position = self.position;

            if let Some(element) = self.parse_array_element() {
                elements.push(element);
            }

            self.skip_newlines();

            if self.at(&TokenKind::RightBracket) {
                break;
            }

            if self.at(&TokenKind::Comma) {
                self.bump();
                self.skip_newlines();

                if self.at(&TokenKind::RightBracket) {
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

        let right = self.expect(TokenKind::RightBracket)?;

        let span = Span::cover(left.span(), right.span()).unwrap_or(left.span());

        Some(self.add_node(SyntaxNodeKind::ArrayLiteral, span, elements))
    }

    pub(super) fn parse_struct_literal_field(&mut self) -> Option<NodeId> {
        let name = self.parse_identifier()?;
        let name_span = self.node_span(name);

        if !self.at(&TokenKind::Colon) {
            return Some(self.add_node(
                SyntaxNodeKind::StructLiteralFieldShorthand,
                name_span,
                vec![name],
            ));
        }

        self.expect(TokenKind::Colon)?;

        self.skip_newlines();

        let value = self.parse_expression()?;

        let span = Span::cover(name_span, self.node_span(value)).unwrap_or(name_span);

        Some(self.add_node(SyntaxNodeKind::StructLiteralField, span, vec![name, value]))
    }
}
