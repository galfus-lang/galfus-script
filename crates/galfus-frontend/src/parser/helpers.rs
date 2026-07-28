use super::*;
use crate::{AssignmentOperatorKind, BinaryAssociativity, BinaryOperatorKind, UnaryOperatorKind};

impl Parser {
    pub(super) fn can_start_expression(&self) -> bool {
        if self.at(&TokenKind::Less) {
            return self.can_parse_cast_expression();
        }

        matches!(
            self.current().kind(),
            TokenKind::Identifier
                | TokenKind::SelfKw
                | TokenKind::Integer
                | TokenKind::Float
                | TokenKind::String
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
                | TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::New
                | TokenKind::Copy
                | TokenKind::Minus
                | TokenKind::Bang
                | TokenKind::Tilde
                | TokenKind::Match
                | TokenKind::Instanceof
                | TokenKind::Typeof
                | TokenKind::Fn
        )
    }

    pub(super) fn can_continue_expression_after_newline(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Dot
                | TokenKind::ColonColon
                | TokenKind::LeftBracket
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::StarStar
                | TokenKind::Less
                | TokenKind::LessEqual
                | TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::EqualEqual
                | TokenKind::BangEqual
                | TokenKind::AmpAmp
                | TokenKind::PipePipe
                | TokenKind::QuestionQuestion
        )
    }

    pub(super) fn binary_operator_info(kind: &TokenKind) -> Option<(u8, BinaryAssociativity)> {
        let operator = BinaryOperatorKind::from_token(kind)?;

        Some((operator.precedence(), operator.associativity()))
    }

    pub(super) fn is_unary_operator(kind: &TokenKind) -> bool {
        UnaryOperatorKind::from_token(kind).is_some()
    }

    pub(super) fn is_assignment_operator(kind: &TokenKind) -> bool {
        AssignmentOperatorKind::from_token(kind).is_some()
    }

    pub(super) fn expression_can_be_assignment_target(&self, expression: NodeId) -> bool {
        let Some(node) = self.graph.syntax().node(expression) else {
            return false;
        };

        matches!(
            node.kind(),
            SyntaxNodeKind::NameExpression
                | SyntaxNodeKind::MemberExpression
                | SyntaxNodeKind::IndexExpression
        )
    }

    pub(super) fn expression_can_be_statement(&self, expression: NodeId) -> bool {
        let Some(node) = self.graph.syntax().node(expression) else {
            return false;
        };

        match node.kind() {
            SyntaxNodeKind::CallExpression
            | SyntaxNodeKind::MatchExpression
            | SyntaxNodeKind::InstanceofExpression => true,

            SyntaxNodeKind::GroupedExpression => node
                .children()
                .first()
                .copied()
                .map(|child| self.expression_can_be_statement(child))
                .unwrap_or(false),

            _ => false,
        }
    }

    pub(super) fn expect_statement_end(&mut self) {
        if self.at(&TokenKind::Newline) {
            self.skip_newlines();
            return;
        }

        if self.at(&TokenKind::Semicolon) {
            self.bump();
            self.skip_newlines();
            return;
        }

        if self.at(&TokenKind::RightBrace) || self.is_eof() {
            return;
        }

        let found = self.current().clone();

        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::ExpectedToken,
            format!("expected statement terminator, found `{:?}`", found.kind()),
            found.span(),
        ));
    }

    pub(super) fn is_function_metadata_start(&self) -> bool {
        if !self.at(&TokenKind::LeftParen) {
            return false;
        }

        let mut offset = 1;
        while self.peek(offset).kind() == &TokenKind::Newline {
            offset += 1;
        }

        self.peek(offset).kind() == &TokenKind::Identifier
            && matches!(self.token_text(self.peek(offset)), "stamp" | "after")
    }

    pub(super) fn at_type_argument_close(&self) -> bool {
        matches!(
            self.current().kind(),
            TokenKind::Greater
                | TokenKind::ShiftRight
                | TokenKind::GreaterEqual
                | TokenKind::ShiftRightEqual
        )
    }

    pub(super) fn split_current_type_argument_close(&mut self) {
        let token = self.current().clone();
        let span = token.span();

        let source_id = span.source_id();
        let start = span.start();
        let end = span.end();

        match token.kind() {
            TokenKind::ShiftRight => {
                self.tokens[self.position] =
                    Token::new(TokenKind::Greater, Span::new(source_id, start, start + 1));

                self.tokens.insert(
                    self.position + 1,
                    Token::new(TokenKind::Greater, Span::new(source_id, start + 1, end)),
                );
            }

            TokenKind::GreaterEqual => {
                self.tokens[self.position] =
                    Token::new(TokenKind::Greater, Span::new(source_id, start, start + 1));

                self.tokens.insert(
                    self.position + 1,
                    Token::new(TokenKind::Equal, Span::new(source_id, start + 1, end)),
                );
            }

            TokenKind::ShiftRightEqual => {
                self.tokens[self.position] =
                    Token::new(TokenKind::Greater, Span::new(source_id, start, start + 1));

                self.tokens.insert(
                    self.position + 1,
                    Token::new(
                        TokenKind::Greater,
                        Span::new(source_id, start + 1, start + 2),
                    ),
                );

                self.tokens.insert(
                    self.position + 2,
                    Token::new(TokenKind::Equal, Span::new(source_id, start + 2, end)),
                );
            }

            _ => {}
        }
    }

    pub(super) fn expect_type_argument_close(&mut self) -> Option<Token> {
        if matches!(
            self.current().kind(),
            TokenKind::ShiftRight | TokenKind::GreaterEqual | TokenKind::ShiftRightEqual
        ) {
            self.split_current_type_argument_close();
        }

        self.expect(TokenKind::Greater)
    }

    pub(super) fn can_parse_generic_call_suffix(&self) -> bool {
        if !self.at(&TokenKind::Less) {
            return false;
        }

        let mut index = self.position;
        let mut depth = 0usize;

        while index < self.tokens.len() {
            match self.tokens[index].kind() {
                TokenKind::Less => {
                    depth += 1;
                }

                TokenKind::Greater => {
                    depth -= 1;

                    if depth == 0 {
                        index += 1;

                        while index < self.tokens.len()
                            && self.tokens[index].kind() == &TokenKind::Newline
                        {
                            index += 1;
                        }

                        return self.tokens.get(index).is_some_and(|token| {
                            matches!(token.kind(), TokenKind::LeftParen | TokenKind::ColonColon)
                        });
                    }
                }

                TokenKind::ShiftRight => {
                    if depth >= 2 {
                        depth -= 2;

                        if depth == 0 {
                            index += 1;

                            while index < self.tokens.len()
                                && self.tokens[index].kind() == &TokenKind::Newline
                            {
                                index += 1;
                            }

                            return self.tokens.get(index).is_some_and(|token| {
                                matches!(token.kind(), TokenKind::LeftParen | TokenKind::ColonColon)
                            });
                        }
                    } else {
                        return false;
                    }
                }

                TokenKind::Eof => return false,

                _ => {}
            }

            index += 1;
        }

        false
    }

    pub(super) fn find_function_anchor_separator(&self) -> Option<usize> {
        let mut index = self.position;
        let mut depth = 0usize;
        let mut separator = None;

        while index < self.tokens.len() {
            match self.tokens[index].kind() {
                TokenKind::Less => {
                    depth += 1;
                }

                TokenKind::Greater => {
                    depth = depth.saturating_sub(1);
                }

                TokenKind::ShiftRight => {
                    if depth >= 2 {
                        depth -= 2;
                    } else if depth == 1 {
                        depth = 0;
                    }
                }

                TokenKind::ColonColon if depth == 0 => {
                    let next = index + 1;

                    if matches!(
                        self.tokens.get(next).map(|token| token.kind()),
                        Some(TokenKind::Identifier)
                    ) {
                        separator = Some(index);
                    }
                }

                TokenKind::LeftParen if depth == 0 => {
                    return separator;
                }

                TokenKind::Eof | TokenKind::Newline if depth == 0 => {
                    return separator;
                }

                _ => {}
            }

            index += 1;
        }

        separator
    }

    pub(super) fn parse_binding_after_keyword(
        &mut self,
        require_initializer: bool,
    ) -> Option<(Vec<NodeId>, Span)> {
        self.skip_newlines();

        let binding = self.parse_binding_pattern()?;
        let mut children = vec![binding];
        let mut end_span = self.node_span(binding);

        self.skip_newlines();

        if self.at(&TokenKind::Colon) {
            let annotation = self.parse_type_annotation()?;
            end_span = self.node_span(annotation);
            children.push(annotation);
        }

        self.skip_newlines();

        if self.at(&TokenKind::Equal) {
            let initializer = self.parse_initializer()?;
            end_span = self.node_span(initializer);
            children.push(initializer);
        } else if require_initializer {
            let token = self.current();

            self.graph.push_diagnostic(Diagnostic::error(
                ParserDiagnosticCode::ExpectedInitializer,
                token.span(),
            ));

            return None;
        }

        Some((children, end_span))
    }

    pub(super) fn parse_binding_helper(
        &mut self,
        keyword: TokenKind,
        is_const: bool,
        node_kind: SyntaxNodeKind,
    ) -> Option<NodeId> {
        let token = self.expect(keyword)?;
        let (children, end_span) = self.parse_binding_after_keyword(is_const)?;

        self.expect_statement_end();

        let span = Span::cover(token.span(), end_span).unwrap_or(token.span());

        Some(self.add_node(node_kind, span, children))
    }

    pub(super) fn can_parse_cast_expression(&self) -> bool {
        if !self.at(&TokenKind::Less) {
            return false;
        }

        let mut depth = 0usize;
        let mut position = self.position;

        while position < self.tokens.len() {
            let token = &self.tokens[position];

            match token.kind() {
                TokenKind::Less => {
                    depth += 1;
                }

                TokenKind::Greater => {
                    if depth == 0 {
                        return false;
                    }

                    depth -= 1;

                    if depth == 0 {
                        let next = self.token_after_newlines(position + 1);

                        return matches!(
                            next.kind(),
                            TokenKind::Identifier
                                | TokenKind::SelfKw
                                | TokenKind::Integer
                                | TokenKind::Float
                                | TokenKind::String
                                | TokenKind::True
                                | TokenKind::False
                                | TokenKind::Null
                                | TokenKind::LeftParen
                                | TokenKind::LeftBracket
                                | TokenKind::New
                                | TokenKind::Copy
                                | TokenKind::Minus
                                | TokenKind::Bang
                                | TokenKind::Tilde
                                | TokenKind::Less
                        );
                    }
                }

                TokenKind::Eof => return false,

                _ => {}
            }

            position += 1;
        }

        false
    }

    pub(super) fn token_after_newlines(&self, mut position: usize) -> &Token {
        while position < self.tokens.len() && self.tokens[position].kind() == &TokenKind::Newline {
            position += 1;
        }

        self.tokens
            .get(position)
            .unwrap_or_else(|| self.tokens.last().expect("parser has eof token"))
    }

    pub(super) fn can_start_range_from(&self, expression: NodeId) -> bool {
        matches!(
            self.node_kind(expression),
            Some(SyntaxNodeKind::IntegerLiteral | SyntaxNodeKind::FloatLiteral)
        )
    }

    pub(super) fn node_kind(&self, node: NodeId) -> Option<SyntaxNodeKind> {
        self.graph.syntax().node(node).map(|node| node.kind())
    }
}
