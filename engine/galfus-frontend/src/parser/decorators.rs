use super::*;

impl Parser {
    pub(super) fn parse_optional_decorator_list(&mut self) -> Option<Option<NodeId>> {
        self.skip_newlines();

        if self.at(&TokenKind::At) {
            return self.parse_decorator_list().map(Some);
        }

        Some(None)
    }

    pub(super) fn parse_decorator_list(&mut self) -> Option<NodeId> {
        let first_span = self.current().span();
        let mut decorators = Vec::new();
        let mut end_span = first_span;

        while self.at(&TokenKind::At) {
            let decorator = self.parse_decorator()?;
            end_span = self.node_span(decorator);
            decorators.push(decorator);

            self.skip_newlines();
        }

        let span = Span::cover(first_span, end_span).unwrap_or(first_span);

        Some(self.add_node(SyntaxNodeKind::DecoratorList, span, decorators))
    }

    pub(super) fn parse_decorator(&mut self) -> Option<NodeId> {
        let at_token = self.expect(TokenKind::At)?;

        let target = self.parse_decorator_target()?;

        let span = Span::cover(at_token.span(), self.node_span(target)).unwrap_or(at_token.span());

        Some(self.add_node(SyntaxNodeKind::Decorator, span, vec![target]))
    }

    pub(super) fn parse_decorator_target(&mut self) -> Option<NodeId> {
        let identifier = self.parse_identifier()?;

        let mut expression = self.add_node(
            SyntaxNodeKind::NameExpression,
            self.node_span(identifier),
            vec![identifier],
        );

        loop {
            if self.at(&TokenKind::LeftParen) {
                expression = self.parse_call_expression(expression)?;
                continue;
            }

            // TODO: Add parentheses recovery erro if after newline

            self.skip_newlines();

            if self.at(&TokenKind::ColonColon) {
                expression = self.parse_path_expression(expression)?;
                continue;
            }

            break;
        }

        Some(expression)
    }
}
