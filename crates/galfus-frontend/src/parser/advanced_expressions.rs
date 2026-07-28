use super::*;
use crate::RangeOperatorKind;
use crate::UnaryOperatorKind;
use crate::parser::expressions::ExpressionBoundary;

impl Parser {
    pub(super) fn parse_new_literal(&mut self) -> Option<NodeId> {
        let new_token = self.expect(TokenKind::New)?;

        self.skip_newlines();

        if self.at(&TokenKind::LeftParen) {
            return self.parse_typed_new_after_paren(new_token);
        }

        let fields = self.parse_struct_literal_field_list()?;

        let span =
            Span::cover(new_token.span(), self.node_span(fields)).unwrap_or(new_token.span());

        Some(self.add_node(SyntaxNodeKind::InferredStructLiteral, span, vec![fields]))
    }

    pub(super) fn parse_typed_new_after_paren(&mut self, new_token: Token) -> Option<NodeId> {
        self.expect(TokenKind::LeftParen)?;

        self.skip_newlines();

        let type_node = self.parse_type()?;

        self.skip_newlines();

        let type_kind = self
            .graph
            .syntax()
            .node(type_node)
            .map(|n| n.kind())
            .unwrap_or(SyntaxNodeKind::NamedType);

        if matches!(type_kind, SyntaxNodeKind::ArrayType) {
            let mut children = vec![type_node];

            if self.at(&TokenKind::Comma) {
                self.bump();
                self.skip_newlines();

                if !self.at(&TokenKind::RightParen)
                    && let Some(length) = self.parse_expression()
                {
                    children.push(length);
                }

                self.skip_newlines();
            }

            let close = self.expect(TokenKind::RightParen)?;

            let span = Span::cover(new_token.span(), close.span()).unwrap_or(new_token.span());

            return Some(self.add_node(SyntaxNodeKind::NewArrayExpression, span, children));
        }

        self.expect(TokenKind::RightParen)?;

        self.skip_newlines();

        let fields = self.parse_struct_literal_field_list()?;

        let span =
            Span::cover(new_token.span(), self.node_span(fields)).unwrap_or(new_token.span());

        Some(self.add_node(SyntaxNodeKind::StructLiteral, span, vec![type_node, fields]))
    }

    pub(super) fn parse_arrow_function_expression(&mut self) -> Option<NodeId> {
        let parameters = self.parse_parameter_list()?;

        self.skip_newlines();

        let return_type = if self.at(&TokenKind::Colon) {
            self.bump();

            self.skip_newlines();

            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();

        self.expect(TokenKind::Arrow)?;

        self.skip_newlines();

        let body = if self.at(&TokenKind::LeftBrace) {
            self.parse_block()?
        } else {
            self.parse_expression()?
        };

        let mut children = vec![parameters];

        if let Some(return_type) = return_type {
            children.push(return_type);
        }

        children.push(body);

        let span = Span::cover(self.node_span(parameters), self.node_span(body))
            .unwrap_or_else(|| self.node_span(parameters));

        Some(self.add_node(SyntaxNodeKind::ArrowFunctionExpression, span, children))
    }

    pub(super) fn parse_copy_expression(&mut self, boundary: ExpressionBoundary) -> Option<NodeId> {
        let copy_token = self.expect(TokenKind::Copy)?;

        self.skip_newlines();

        let value = self.parse_unary_expression(boundary)?;

        let span =
            Span::cover(copy_token.span(), self.node_span(value)).unwrap_or(copy_token.span());

        Some(self.add_node(SyntaxNodeKind::CopyExpression, span, vec![value]))
    }

    pub(super) fn parse_generic_expression(&mut self, target: NodeId) -> Option<NodeId> {
        let arguments = self.parse_generic_argument_list()?;

        let span = Span::cover(self.node_span(target), self.node_span(arguments))
            .unwrap_or_else(|| self.node_span(target));

        Some(self.add_node(
            SyntaxNodeKind::GenericExpression,
            span,
            vec![target, arguments],
        ))
    }

    pub(super) fn parse_cast_expression(&mut self, boundary: ExpressionBoundary) -> Option<NodeId> {
        let left = self.expect(TokenKind::Less)?;

        let ty = self.parse_type()?;

        self.expect(TokenKind::Greater)?;

        self.skip_newlines();

        let value = self.parse_unary_expression(boundary)?;

        let span = Span::cover(left.span(), self.node_span(value)).unwrap_or(left.span());

        Some(self.add_node(SyntaxNodeKind::CastExpression, span, vec![ty, value]))
    }

    pub(super) fn parse_range_expression(
        &mut self,
        start: NodeId,
        _boundary: ExpressionBoundary,
    ) -> Option<NodeId> {
        let operator_token = self.bump();

        let operator_kind = RangeOperatorKind::from_token(operator_token.kind())
            .expect("parser accepted token as range operator");

        let operator = self.add_operator_node(
            SyntaxNodeKind::RangeOperator,
            operator_token.span(),
            OperatorKind::Range(operator_kind),
        );

        let end_or_count = self.parse_range_operand()?;

        let mut children = vec![start, operator, end_or_count];
        let mut end_span = self.node_span(end_or_count);

        if operator_kind == RangeOperatorKind::Quantity && self.at(&TokenKind::Percent) {
            let step = self.parse_range_step()?;
            end_span = self.node_span(step);
            children.push(step);
        }

        let span =
            Span::cover(self.node_span(start), end_span).unwrap_or_else(|| self.node_span(start));

        Some(self.add_node(SyntaxNodeKind::RangeExpression, span, children))
    }

    pub(super) fn parse_range_operand(&mut self) -> Option<NodeId> {
        if self.at(&TokenKind::Minus) {
            let operator_token = self.bump();
            let operator = self.graph.syntax_mut().add_operator_node(
                SyntaxNodeKind::UnaryOperator,
                operator_token.span(),
                OperatorKind::Unary(UnaryOperatorKind::Negate),
            );

            let operand = self.parse_range_operand()?;
            let span = Span::cover(operator_token.span(), self.node_span(operand))
                .unwrap_or(operator_token.span());

            return Some(self.add_node(
                SyntaxNodeKind::UnaryExpression,
                span,
                vec![operator, operand],
            ));
        }

        if self.at(&TokenKind::Integer) {
            return self.parse_integer_literal();
        }

        if self.at(&TokenKind::Float) {
            return self.parse_float_literal();
        }

        let found = self.bump();

        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::UnexpectedToken,
            format!("expected numeric literal, found `{:?}`", found.kind()),
            found.span(),
        ));

        None
    }

    pub(super) fn parse_range_step(&mut self) -> Option<NodeId> {
        let percent = self.expect(TokenKind::Percent)?;
        let expression = self.parse_range_operand()?;

        let span =
            Span::cover(percent.span(), self.node_span(expression)).unwrap_or(percent.span());

        Some(self.add_node(SyntaxNodeKind::RangeStep, span, vec![expression]))
    }

    pub(super) fn parse_null_safe_member_expression(&mut self, target: NodeId) -> Option<NodeId> {
        self.expect(TokenKind::QuestionDot)?;
        self.skip_newlines();

        let member = self.parse_identifier()?;

        let span = Span::cover(self.node_span(target), self.node_span(member))
            .unwrap_or_else(|| self.node_span(target));

        Some(self.add_node(
            SyntaxNodeKind::NullSafeMemberExpression,
            span,
            vec![target, member],
        ))
    }

    pub(super) fn parse_match_expression(&mut self) -> Option<NodeId> {
        let match_token = self.expect(TokenKind::Match)?;

        self.skip_newlines();

        let subject = self.parse_expression_before_block()?;

        self.skip_newlines();

        let arms = self.parse_match_arm_list()?;

        let span =
            Span::cover(match_token.span(), self.node_span(arms)).unwrap_or(match_token.span());

        Some(self.add_node(SyntaxNodeKind::MatchExpression, span, vec![subject, arms]))
    }

    pub(super) fn parse_instanceof_expression(&mut self) -> Option<NodeId> {
        let instanceof_token = self.expect(TokenKind::Instanceof)?;

        self.skip_newlines();

        let subject = self.parse_expression_before_block()?;

        self.skip_newlines();

        let arms = self.parse_instanceof_arm_list()?;

        let span = Span::cover(instanceof_token.span(), self.node_span(arms))
            .unwrap_or(instanceof_token.span());

        Some(self.add_node(
            SyntaxNodeKind::InstanceofExpression,
            span,
            vec![subject, arms],
        ))
    }

    pub(super) fn parse_typeof_expression(&mut self) -> Option<NodeId> {
        let typeof_token = self.expect(TokenKind::Typeof)?;

        self.skip_newlines();

        let subject = self.parse_expression_before_block()?;

        self.skip_newlines();

        let arms = self.parse_typeof_arm_list()?;

        let span =
            Span::cover(typeof_token.span(), self.node_span(arms)).unwrap_or(typeof_token.span());

        Some(self.add_node(SyntaxNodeKind::TypeofExpression, span, vec![subject, arms]))
    }
}
