use super::*;

impl Parser {
    pub(super) fn parse_constraint_field(&mut self) -> Option<NodeId> {
        let name = self.parse_identifier()?;
        let name_span = self.node_span(name);

        self.skip_newlines();

        self.expect(TokenKind::Colon)?;

        self.skip_newlines();

        let field_type = self.parse_type()?;

        let span = Span::cover(name_span, self.node_span(field_type)).unwrap_or(name_span);

        Some(self.add_node(
            SyntaxNodeKind::ConstraintField,
            span,
            vec![name, field_type],
        ))
    }

    pub(super) fn parse_function_type(&mut self) -> Option<NodeId> {
        let fn_token = self.expect(TokenKind::Fn)?;

        self.skip_newlines();

        let parameters = self.parse_function_type_parameter_list()?;

        self.skip_newlines();

        self.expect(TokenKind::Colon)?;

        self.skip_newlines();

        let return_type = self.parse_type()?;

        let span =
            Span::cover(fn_token.span(), self.node_span(return_type)).unwrap_or(fn_token.span());

        Some(self.add_node(
            SyntaxNodeKind::FunctionType,
            span,
            vec![parameters, return_type],
        ))
    }

    pub(super) fn parse_grouped_type(&mut self) -> Option<NodeId> {
        let left = self.expect(TokenKind::LeftParen)?;

        self.skip_newlines();

        let first = self.parse_type()?;
        let mut types = vec![first];

        self.skip_newlines();

        let is_tuple = self.at(&TokenKind::Comma);

        while self.at(&TokenKind::Comma) {
            self.bump();
            self.skip_newlines();

            if self.at(&TokenKind::RightParen) {
                break;
            }

            let next = self.parse_type()?;
            types.push(next);

            self.skip_newlines();
        }

        let right = self.expect(TokenKind::RightParen)?;

        let span = Span::cover(left.span(), right.span()).unwrap_or(left.span());

        if is_tuple {
            if types.len() < 2 {
                self.graph.push_diagnostic(Diagnostic::error_with_message(
                    ParserDiagnosticCode::ExpectedType,
                    "tuple type requires at least two elements".to_string(),
                    right.span(),
                ));

                return Some(self.add_node(SyntaxNodeKind::GroupedType, span, vec![first]));
            }

            return Some(self.add_node(SyntaxNodeKind::TupleType, span, types));
        }

        Some(self.add_node(SyntaxNodeKind::GroupedType, span, vec![first]))
    }

    pub(super) fn make_named_type_from_identifier(&mut self, identifier: NodeId) -> NodeId {
        let span = self.node_span(identifier);
        self.add_node(SyntaxNodeKind::NamedType, span, vec![identifier])
    }

    pub(super) fn parse_named_type_or_path(&mut self) -> Option<NodeId> {
        let first_identifier = self.parse_identifier()?;

        let mut identifiers = vec![first_identifier];
        let mut end_span = self.node_span(first_identifier);

        while self.at(&TokenKind::ColonColon) {
            self.bump();

            let identifier = self.parse_identifier()?;
            end_span = self.node_span(identifier);
            identifiers.push(identifier);
        }

        if identifiers.len() == 1 {
            return Some(self.make_named_type_from_identifier(first_identifier));
        }

        let span = Span::cover(self.node_span(first_identifier), end_span)
            .unwrap_or_else(|| self.node_span(first_identifier));

        Some(self.add_node(SyntaxNodeKind::Path, span, identifiers))
    }

    pub(super) fn parse_named_type_or_path_until(
        &mut self,
        stop_position: usize,
    ) -> Option<NodeId> {
        let first_identifier = self.parse_identifier()?;

        let mut identifiers = vec![first_identifier];
        let mut end_span = self.node_span(first_identifier);

        while self.position < stop_position && self.at(&TokenKind::ColonColon) {
            self.bump();

            let identifier = self.parse_identifier()?;
            end_span = self.node_span(identifier);
            identifiers.push(identifier);
        }

        if identifiers.len() == 1 {
            return Some(self.make_named_type_from_identifier(first_identifier));
        }

        let span = Span::cover(self.node_span(first_identifier), end_span)
            .unwrap_or_else(|| self.node_span(first_identifier));

        Some(self.add_node(SyntaxNodeKind::Path, span, identifiers))
    }

    pub(super) fn parse_function_anchor_until(
        &mut self,
        separator_position: usize,
    ) -> Option<NodeId> {
        let mut anchor_type = self.parse_named_type_or_path_until(separator_position)?;

        if self.at(&TokenKind::Less) {
            anchor_type = self.parse_generic_type(anchor_type)?;
        }

        let span = self.node_span(anchor_type);

        Some(self.add_node(SyntaxNodeKind::FunctionAnchor, span, vec![anchor_type]))
    }

    pub(super) fn parse_choice_payload_item(&mut self) -> Option<NodeId> {
        let decorators = self.parse_optional_decorator_list()?;

        let payload_type = self.parse_type()?;

        let mut children = Vec::new();

        if let Some(decorators) = decorators {
            children.push(decorators);
        }

        children.push(payload_type);

        let start_span = decorators
            .map(|decorators| self.node_span(decorators))
            .unwrap_or_else(|| self.node_span(payload_type));

        let span = Span::cover(start_span, self.node_span(payload_type)).unwrap_or(start_span);

        Some(self.add_node(SyntaxNodeKind::ChoicePayloadItem, span, children))
    }
}
