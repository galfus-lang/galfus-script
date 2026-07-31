use super::*;
use crate::{BinaryAssociativity, BinaryOperatorKind, OperatorKind, UnaryOperatorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExpressionBoundary {
    None,
    BeforeBlock,
}

impl Parser {
    pub(super) fn parse_expression(&mut self) -> Option<NodeId> {
        self.parse_binary_expression(0, ExpressionBoundary::None)
    }

    pub(super) fn parse_expression_before_block(&mut self) -> Option<NodeId> {
        self.parse_binary_expression(0, ExpressionBoundary::BeforeBlock)
    }

    pub(super) fn parse_name_expression(&mut self) -> Option<NodeId> {
        let identifier = self.parse_identifier_or_self()?;
        let span = self.node_span(identifier);

        Some(self.add_node(SyntaxNodeKind::NameExpression, span, vec![identifier]))
    }

    pub(super) fn parse_primary_expression(
        &mut self,
        _boundary: ExpressionBoundary,
    ) -> Option<NodeId> {
        if self.at(&TokenKind::Fn) {
            return self.parse_anonymous_function_expression();
        }

        if self.at(&TokenKind::LeftParen) {
            return self.parse_grouped_expression();
        }

        if self.at(&TokenKind::LeftBracket) {
            return self.parse_array_literal();
        }

        if self.at(&TokenKind::New) {
            return self.parse_new_literal();
        }

        if self.at(&TokenKind::Match) {
            return self.parse_match_expression();
        }

        if self.at(&TokenKind::Instanceof) {
            return self.parse_instanceof_expression();
        }

        if self.at(&TokenKind::Typeof) {
            return self.parse_typeof_expression();
        }

        if self.at(&TokenKind::Integer) {
            return self.parse_integer_literal();
        }

        if self.at(&TokenKind::Float) {
            return self.parse_float_literal();
        }

        if self.at(&TokenKind::String) {
            return self.parse_string_literal();
        }

        if self.at(&TokenKind::True) || self.at(&TokenKind::False) {
            return self.parse_bool_literal();
        }

        if self.at(&TokenKind::Null) {
            return self.parse_null_literal();
        }

        if self.at(&TokenKind::Underscore) {
            let token = self.bump();
            return Some(self.add_node(
                SyntaxNodeKind::WildcardExpression,
                token.span(),
                Vec::new(),
            ));
        }

        if self.at(&TokenKind::Identifier) || self.at(&TokenKind::SelfKw) {
            return self.parse_name_expression();
        }

        let found = self.bump();

        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::UnexpectedToken,
            format!("expected expression, found `{:?}`", found.kind()),
            found.span(),
        ));

        None
    }

    fn parse_postfix_expression(&mut self, boundary: ExpressionBoundary) -> Option<NodeId> {
        let mut expression = self.parse_primary_expression(boundary)?;

        loop {
            self.skip_soft_newlines_before_expression_continuation();

            if self.at(&TokenKind::Dot) {
                expression = self.parse_member_expression(expression)?;
                continue;
            }

            if self.at(&TokenKind::QuestionDot) {
                expression = self.parse_null_safe_member_expression(expression)?;
                continue;
            }

            if self.at(&TokenKind::DotDot) {
                if self.can_start_range_from(expression) {
                    expression = self.parse_range_expression(expression, boundary)?;
                    continue;
                }

                break;
            }

            if self.at(&TokenKind::ColonColon) {
                if self.can_start_range_from(expression) {
                    expression = self.parse_range_expression(expression, boundary)?;
                    continue;
                }

                expression = self.parse_path_expression(expression)?;
                continue;
            }

            if self.at(&TokenKind::LeftBracket) {
                expression = self.parse_index_expression(expression)?;
                continue;
            }

            if self.at(&TokenKind::Less) && self.can_parse_generic_call_suffix() {
                expression = self.parse_generic_expression(expression)?;
                continue;
            }

            if self.at(&TokenKind::LeftParen) {
                expression = self.parse_call_expression(expression)?;
                continue;
            }

            break;
        }

        Some(expression)
    }

    pub(super) fn parse_call_expression(&mut self, target: NodeId) -> Option<NodeId> {
        let arguments = self.parse_argument_list()?;

        let span = Span::cover(self.node_span(target), self.node_span(arguments))
            .unwrap_or_else(|| self.node_span(target));

        Some(self.add_node(
            SyntaxNodeKind::CallExpression,
            span,
            vec![target, arguments],
        ))
    }

    pub(super) fn parse_member_expression(&mut self, target: NodeId) -> Option<NodeId> {
        self.expect(TokenKind::Dot)?;

        self.skip_newlines();

        let member = self.parse_identifier()?;

        let span = Span::cover(self.node_span(target), self.node_span(member))
            .unwrap_or_else(|| self.node_span(target));

        Some(self.add_node(SyntaxNodeKind::MemberExpression, span, vec![target, member]))
    }

    pub(super) fn parse_path_expression(&mut self, target: NodeId) -> Option<NodeId> {
        self.expect(TokenKind::ColonColon)?;

        self.skip_newlines();

        let anchor = self.parse_identifier()?;

        let span = Span::cover(self.node_span(target), self.node_span(anchor))
            .unwrap_or_else(|| self.node_span(target));

        Some(self.add_node(SyntaxNodeKind::PathExpression, span, vec![target, anchor]))
    }

    fn parse_binary_expression(
        &mut self,
        min_precedence: u8,
        boundary: ExpressionBoundary,
    ) -> Option<NodeId> {
        let mut left = self.parse_unary_expression(boundary)?;

        loop {
            self.skip_soft_newlines_before_expression_continuation();

            let operator_token = self.current().clone();

            let Some((precedence, associativity)) =
                Self::binary_operator_info(operator_token.kind())
            else {
                break;
            };

            if precedence < min_precedence {
                break;
            }

            self.bump();

            let operator_kind = BinaryOperatorKind::from_token(operator_token.kind())
                .expect("parser accepted token as binary operator");

            let operator = self.add_operator_node(
                SyntaxNodeKind::BinaryOperator,
                operator_token.span(),
                OperatorKind::Binary(operator_kind),
            );
            self.skip_newlines();

            let next_min_precedence = match associativity {
                BinaryAssociativity::Left => precedence + 1,
                BinaryAssociativity::Right => precedence,
            };

            let right = self.parse_binary_expression(next_min_precedence, boundary)?;

            let span = Span::cover(self.node_span(left), self.node_span(right))
                .unwrap_or_else(|| self.node_span(left));

            left = self.add_node(
                SyntaxNodeKind::BinaryExpression,
                span,
                vec![left, operator, right],
            );
        }

        Some(left)
    }

    pub(super) fn parse_grouped_expression(&mut self) -> Option<NodeId> {
        let left = self.expect(TokenKind::LeftParen)?;

        self.skip_newlines();

        if self.at(&TokenKind::RightParen) {
            let found = self.current().clone();

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::UnexpectedToken,
                "expected expression, found `RightParen`",
                found.span(),
            ));

            return None;
        }

        let first = self.parse_expression()?;
        let mut expressions = vec![first];

        self.skip_newlines();

        let is_tuple = self.at(&TokenKind::Comma);

        while self.at(&TokenKind::Comma) {
            self.bump();
            self.skip_newlines();

            if self.at(&TokenKind::RightParen) {
                break;
            }

            let next = self.parse_expression()?;
            expressions.push(next);

            self.skip_newlines();
        }

        let right = self.expect(TokenKind::RightParen)?;

        let span = Span::cover(left.span(), right.span()).unwrap_or(left.span());

        if is_tuple {
            if expressions.len() < 2 {
                self.graph.push_diagnostic(Diagnostic::error_with_message(
                    ParserDiagnosticCode::UnexpectedToken,
                    "tuple expression requires at least two elements".to_string(),
                    right.span(),
                ));

                return Some(self.add_node(SyntaxNodeKind::GroupedExpression, span, vec![first]));
            }

            return Some(self.add_node(SyntaxNodeKind::TupleExpression, span, expressions));
        }

        Some(self.add_node(SyntaxNodeKind::GroupedExpression, span, vec![first]))
    }

    pub(super) fn parse_unary_expression(
        &mut self,
        boundary: ExpressionBoundary,
    ) -> Option<NodeId> {
        if self.at(&TokenKind::Less) && self.can_parse_cast_expression() {
            return self.parse_cast_expression(boundary);
        }

        if self.at(&TokenKind::Copy) {
            return self.parse_copy_expression(boundary);
        }

        if Self::is_unary_operator(self.current().kind()) {
            let operator_token = self.bump();

            let operator_kind = UnaryOperatorKind::from_token(operator_token.kind())
                .expect("parser accepted token as unary operator");

            let operator = self.graph.syntax_mut().add_operator_node(
                SyntaxNodeKind::UnaryOperator,
                operator_token.span(),
                OperatorKind::Unary(operator_kind),
            );

            self.skip_newlines();

            let operand = self.parse_unary_expression(boundary)?;

            let span = Span::cover(operator_token.span(), self.node_span(operand))
                .unwrap_or(operator_token.span());

            return Some(self.add_node(
                SyntaxNodeKind::UnaryExpression,
                span,
                vec![operator, operand],
            ));
        }

        self.parse_postfix_expression(boundary)
    }

    pub(super) fn parse_index_expression(&mut self, target: NodeId) -> Option<NodeId> {
        let left = self.expect(TokenKind::LeftBracket)?;

        self.skip_newlines();

        if self.at(&TokenKind::RightBracket) {
            let found = self.current().clone();

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::UnexpectedToken,
                "expected expression, found `RightBracket`",
                found.span(),
            ));

            return None;
        }

        let index = self.parse_expression()?;

        self.skip_newlines();

        let right = self.expect(TokenKind::RightBracket)?;
        let target_span = self.node_span(target);

        let span = Span::cover(target_span, right.span())
            .or_else(|| Span::cover(target_span, left.span()))
            .unwrap_or_else(|| self.node_span(target));

        Some(self.add_node(SyntaxNodeKind::IndexExpression, span, vec![target, index]))
    }

    pub(super) fn parse_spread_array_element(&mut self) -> Option<NodeId> {
        let spread_token = self.expect(TokenKind::DotDotDot)?;

        self.skip_newlines();

        let expression = self.parse_expression()?;

        if self
            .graph
            .syntax()
            .node(expression)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::RangeExpression)
        {
            let span = Span::cover(spread_token.span(), self.node_span(expression))
                .unwrap_or(spread_token.span());
            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::UnexpectedToken,
                "range expressions cannot be array spread targets",
                span,
            ));
            return None;
        }

        let span = Span::cover(spread_token.span(), self.node_span(expression))
            .unwrap_or(spread_token.span());

        Some(self.add_node(SyntaxNodeKind::SpreadArrayElement, span, vec![expression]))
    }
}
