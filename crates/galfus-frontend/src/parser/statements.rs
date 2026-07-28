use super::*;
use crate::AssignmentOperatorKind;

impl Parser {
    pub(super) fn parse_statement(&mut self) -> Option<NodeId> {
        if self.at(&TokenKind::Return) {
            return self.parse_return_statement();
        }

        if self.at(&TokenKind::Break) {
            return self.parse_break_statement();
        }

        if self.at(&TokenKind::Continue) {
            return self.parse_continue_statement();
        }

        if self.at(&TokenKind::Var) {
            return self.parse_binding_helper(TokenKind::Var, false, SyntaxNodeKind::VarStatement);
        }

        if self.at(&TokenKind::Const) {
            return self.parse_binding_helper(
                TokenKind::Const,
                true,
                SyntaxNodeKind::ConstStatement,
            );
        }

        if self.at(&TokenKind::If) {
            return self.parse_if_statement();
        }

        if self.at(&TokenKind::For) {
            return self.parse_for_statement();
        }

        if self.at(&TokenKind::Loop) {
            return self.parse_loop_statement();
        }

        if self.at(&TokenKind::Match) {
            return self.parse_match_statement();
        }

        if self.at(&TokenKind::Instanceof) {
            return self.parse_instanceof_statement();
        }

        if self.at(&TokenKind::Typeof) {
            return self.parse_typeof_statement();
        }

        if self.can_start_expression() {
            return self.parse_expression_or_assignment_statement();
        }

        let found = self.bump();

        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::ExpectedStatement,
            format!("expected statement, found `{:?}`", found.kind()),
            found.span(),
        ));

        None
    }

    pub(super) fn parse_return_statement(&mut self) -> Option<NodeId> {
        let return_token = self.expect(TokenKind::Return)?;

        let mut children = Vec::new();
        let mut end_span = return_token.span();

        if self.can_start_expression() {
            let expression = self.parse_expression()?;
            end_span = self.node_span(expression);
            children.push(expression);
        }

        self.expect_statement_end();

        let span = Span::cover(return_token.span(), end_span).unwrap_or(return_token.span());

        Some(self.add_node(SyntaxNodeKind::ReturnStatement, span, children))
    }

    pub(super) fn parse_expression_statement_from(&mut self, expression: NodeId) -> Option<NodeId> {
        let span = self.node_span(expression);

        if !self.expression_can_be_statement(expression) {
            let expression_kind = self
                .node_kind(expression)
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "<missing>".to_string());

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::ExpectedStatement,
                format!(
                    "expected call expression statement, found `{}`",
                    expression_kind
                ),
                span,
            ));
        }

        self.expect_statement_end();

        Some(self.add_node(SyntaxNodeKind::ExpressionStatement, span, vec![expression]))
    }

    pub(super) fn parse_expression_or_assignment_statement(&mut self) -> Option<NodeId> {
        let expression = self.parse_expression()?;

        if Self::is_assignment_operator(self.current().kind()) {
            return self.parse_assignment_statement(expression);
        }

        self.parse_expression_statement_from(expression)
    }

    pub(super) fn parse_if_statement(&mut self) -> Option<NodeId> {
        let if_token = self.expect(TokenKind::If)?;

        self.skip_newlines();

        let condition = self.parse_expression_before_block()?;

        self.skip_newlines();

        let then_block = self.parse_block()?;

        let mut children = vec![condition, then_block];
        let mut end_span = self.node_span(then_block);

        self.skip_newlines();

        if self.at(&TokenKind::Else) {
            let else_clause = self.parse_else_clause()?;
            end_span = self.node_span(else_clause);
            children.push(else_clause);
        }

        let span = Span::cover(if_token.span(), end_span).unwrap_or(if_token.span());

        Some(self.add_node(SyntaxNodeKind::IfStatement, span, children))
    }

    pub(super) fn parse_else_clause(&mut self) -> Option<NodeId> {
        let else_token = self.expect(TokenKind::Else)?;

        self.skip_newlines();

        let child = if self.at(&TokenKind::If) {
            self.parse_if_statement()?
        } else {
            self.parse_block()?
        };

        let span =
            Span::cover(else_token.span(), self.node_span(child)).unwrap_or(else_token.span());

        Some(self.add_node(SyntaxNodeKind::ElseClause, span, vec![child]))
    }

    pub(super) fn parse_assignment_statement(&mut self, target: NodeId) -> Option<NodeId> {
        let target_span = self.node_span(target);

        if !self.expression_can_be_assignment_target(target) {
            let target_kind = self
                .node_kind(target)
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "<missing>".to_string());

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::ExpectedStatement,
                format!("invalid assignment target `{target_kind}`"),
                target_span,
            ));
        }

        let operator_token = if Self::is_assignment_operator(self.current().kind()) {
            self.bump()
        } else {
            self.expect(TokenKind::Equal)?
        };

        let operator_kind = AssignmentOperatorKind::from_token(operator_token.kind())
            .expect("parser accepted token as assignment operator");

        let operator = self.add_operator_node(
            SyntaxNodeKind::AssignmentOperator,
            operator_token.span(),
            OperatorKind::Assignment(operator_kind),
        );

        self.skip_newlines();

        let value = self.parse_expression()?;

        let end_span = self.node_span(value);

        self.expect_statement_end();

        let span = Span::cover(target_span, end_span).unwrap_or(target_span);

        Some(self.add_node(
            SyntaxNodeKind::AssignmentStatement,
            span,
            vec![target, operator, value],
        ))
    }

    pub(super) fn parse_for_binding(&mut self) -> Option<NodeId> {
        let value_name = self.parse_for_binding_name()?;
        let mut children = vec![value_name];
        let mut end_span = self.node_span(value_name);

        self.skip_newlines();

        if self.at(&TokenKind::Comma) {
            self.bump();
            self.skip_newlines();

            let index_name = self.parse_for_binding_name()?;
            end_span = self.node_span(index_name);
            children.push(index_name);
        }

        let span = Span::cover(self.node_span(value_name), end_span)
            .unwrap_or_else(|| self.node_span(value_name));

        Some(self.add_node(SyntaxNodeKind::ForBinding, span, children))
    }

    fn parse_for_binding_name(&mut self) -> Option<NodeId> {
        self.parse_binding_pattern()
    }

    pub(super) fn parse_for_statement(&mut self) -> Option<NodeId> {
        let for_token = self.expect(TokenKind::For)?;

        self.skip_newlines();

        let metadata = self.parse_optional_keyword_metadata(false);

        self.skip_newlines();

        let binding = self.parse_for_binding()?;

        self.skip_newlines();

        self.expect(TokenKind::In)?;

        self.skip_newlines();

        let iterable = self.parse_expression_before_block()?;

        self.skip_newlines();

        let body = self.parse_block()?;

        let mut children = Vec::new();
        if let Some(metadata) = metadata {
            children.push(metadata);
        }
        children.push(binding);
        children.push(iterable);
        children.push(body);

        let span = Span::cover(for_token.span(), self.node_span(body)).unwrap_or(for_token.span());

        Some(self.add_node(SyntaxNodeKind::ForStatement, span, children))
    }

    pub(super) fn parse_break_statement(&mut self) -> Option<NodeId> {
        let break_token = self.expect(TokenKind::Break)?;

        let label = if self.at(&TokenKind::Identifier) {
            self.parse_identifier()
        } else {
            None
        };

        self.expect_statement_end();

        let mut children = Vec::new();
        if let Some(label) = label {
            children.push(label);
        }

        let span = if let Some(label) = label {
            Span::cover(break_token.span(), self.node_span(label)).unwrap_or(break_token.span())
        } else {
            break_token.span()
        };

        Some(self.add_node(SyntaxNodeKind::BreakStatement, span, children))
    }

    pub(super) fn parse_continue_statement(&mut self) -> Option<NodeId> {
        let continue_token = self.expect(TokenKind::Continue)?;

        let label = if self.at(&TokenKind::Identifier) {
            self.parse_identifier()
        } else {
            None
        };

        self.expect_statement_end();

        let mut children = Vec::new();
        if let Some(label) = label {
            children.push(label);
        }

        let span = if let Some(label) = label {
            Span::cover(continue_token.span(), self.node_span(label))
                .unwrap_or(continue_token.span())
        } else {
            continue_token.span()
        };

        Some(self.add_node(SyntaxNodeKind::ContinueStatement, span, children))
    }

    pub(super) fn parse_loop_statement(&mut self) -> Option<NodeId> {
        let loop_token = self.expect(TokenKind::Loop)?;

        self.skip_newlines();

        let metadata = self.parse_optional_keyword_metadata(true);
        self.skip_newlines();

        let condition = if !self.at(&TokenKind::LeftBrace) {
            self.parse_expression()
        } else {
            None
        };
        self.skip_newlines();

        let body = self.parse_block()?;

        let mut children = Vec::new();
        if let Some(metadata) = metadata {
            children.push(metadata);
        }
        if let Some(condition) = condition {
            children.push(condition);
        }
        children.push(body);

        let span =
            Span::cover(loop_token.span(), self.node_span(body)).unwrap_or(loop_token.span());

        Some(self.add_node(SyntaxNodeKind::LoopStatement, span, children))
    }

    pub(super) fn parse_match_statement(&mut self) -> Option<NodeId> {
        let expression = self.parse_match_expression()?;
        self.expect_statement_end();

        let span = self.node_span(expression);

        Some(self.add_node(SyntaxNodeKind::ExpressionStatement, span, vec![expression]))
    }

    pub(super) fn parse_instanceof_statement(&mut self) -> Option<NodeId> {
        let expression = self.parse_instanceof_expression()?;
        self.expect_statement_end();

        let span = self.node_span(expression);

        Some(self.add_node(SyntaxNodeKind::ExpressionStatement, span, vec![expression]))
    }

    pub(super) fn parse_typeof_statement(&mut self) -> Option<NodeId> {
        let expression = self.parse_typeof_expression()?;
        self.expect_statement_end();

        let span = self.node_span(expression);

        Some(self.add_node(SyntaxNodeKind::ExpressionStatement, span, vec![expression]))
    }

    pub(super) fn parse_instanceof_arm(&mut self) -> Option<NodeId> {
        let pattern = self.parse_instanceof_pattern()?;

        self.skip_newlines();

        let body = self.parse_arm_body()?;

        let span = Span::cover(self.node_span(pattern), self.node_span(body))
            .unwrap_or_else(|| self.node_span(pattern));

        Some(self.add_node(SyntaxNodeKind::InstanceofArm, span, vec![pattern, body]))
    }

    pub(super) fn parse_typeof_arm(&mut self) -> Option<NodeId> {
        let pattern = if self.at(&TokenKind::Underscore) {
            self.parse_wildcard_pattern()
        } else {
            self.parse_type()
        }?;

        self.skip_newlines();

        let body = self.parse_arm_body()?;

        let span = Span::cover(self.node_span(pattern), self.node_span(body))
            .unwrap_or_else(|| self.node_span(pattern));

        Some(self.add_node(SyntaxNodeKind::TypeofArm, span, vec![pattern, body]))
    }
}
