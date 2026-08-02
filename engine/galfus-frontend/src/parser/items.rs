use super::*;

impl Parser {
    pub(super) fn parse_item(&mut self) -> Option<NodeId> {
        let decorators = self.parse_optional_decorator_list()?;

        if self.at(&TokenKind::Export) {
            return self.parse_export_item(decorators);
        }

        if self.at(&TokenKind::Fn) {
            return self.parse_function_item(decorators);
        }

        if self.at(&TokenKind::Struct) {
            return self.parse_struct_item(decorators);
        }

        if decorators.is_some() {
            let found = self.current();

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::ExpectedItem,
                "decorators can only be used on functions and structs".to_string(),
                found.span(),
            ));

            return None;
        }

        if self.at(&TokenKind::Import) {
            return self.parse_import_item();
        }

        if self.at(&TokenKind::Var) {
            return self.parse_binding_helper(TokenKind::Var, false, SyntaxNodeKind::VarItem);
        }

        if self.at(&TokenKind::Const) {
            return self.parse_binding_helper(TokenKind::Const, true, SyntaxNodeKind::ConstItem);
        }

        if self.at(&TokenKind::Enum) {
            return self.parse_enum_item();
        }

        if self.at(&TokenKind::Choice) {
            return self.parse_choice_item();
        }

        if self.at(&TokenKind::Type) {
            return self.parse_type_alias_item();
        }

        if self.at(&TokenKind::Constraint) {
            return self.parse_constraint_item();
        }

        let found = self.bump();

        self.graph.push_diagnostic(Diagnostic::error_with_message(
            ParserDiagnosticCode::ExpectedItem,
            format!("expected item, found `{:?}`", found.kind()),
            found.span(),
        ));

        None
    }

    pub(super) fn parse_export_item(&mut self, decorators: Option<NodeId>) -> Option<NodeId> {
        let export_token = self.expect(TokenKind::Export)?;

        self.skip_newlines();

        if decorators.is_some() && !(self.at(&TokenKind::Fn) || self.at(&TokenKind::Struct)) {
            let found = self.current();

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::ExpectedItem,
                "decorators can only be used on functions and structs".to_string(),
                found.span(),
            ));

            return None;
        }

        let item = if self.at(&TokenKind::Var) {
            self.parse_binding_helper(TokenKind::Var, false, SyntaxNodeKind::VarItem)?
        } else if self.at(&TokenKind::Const) {
            self.parse_binding_helper(TokenKind::Const, true, SyntaxNodeKind::ConstItem)?
        } else if self.at(&TokenKind::Fn) {
            self.parse_function_item(decorators)?
        } else if self.at(&TokenKind::Struct) {
            self.parse_struct_item(decorators)?
        } else if self.at(&TokenKind::Enum) {
            self.parse_enum_item()?
        } else if self.at(&TokenKind::Choice) {
            self.parse_choice_item()?
        } else if self.at(&TokenKind::Constraint) {
            self.parse_constraint_item()?
        } else if self.at(&TokenKind::Type) {
            self.parse_type_alias_item()?
        } else {
            let found = self.bump();

            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::ExpectedItem,
                format!("expected exportable item, found `{:?}`", found.kind()),
                found.span(),
            ));

            return None;
        };

        let span =
            Span::cover(export_token.span(), self.node_span(item)).unwrap_or(export_token.span());

        Some(self.add_node(SyntaxNodeKind::ExportItem, span, vec![item]))
    }

    pub(super) fn parse_import_item(&mut self) -> Option<NodeId> {
        let import_token = self.expect(TokenKind::Import)?;

        let clause = self.parse_import_clause()?;

        self.expect(TokenKind::From)?;

        let source = self.parse_import_source()?;

        let span =
            Span::cover(import_token.span(), self.node_span(source)).unwrap_or(import_token.span());

        Some(self.add_node(SyntaxNodeKind::ImportItem, span, vec![clause, source]))
    }

    pub(super) fn parse_function_item(&mut self, decorators: Option<NodeId>) -> Option<NodeId> {
        let fn_token = self.expect(TokenKind::Fn)?;

        self.skip_newlines();

        let metadata = self.parse_optional_keyword_metadata(false);

        let anchor = if let Some(separator_position) = self.find_function_anchor_separator() {
            let anchor = self.parse_function_anchor_until(separator_position)?;
            self.expect(TokenKind::ColonColon)?;
            Some(anchor)
        } else {
            None
        };

        let name = self.parse_identifier()?;

        let generic_parameters = if self.at(&TokenKind::Less) {
            let generics = self.parse_generic_parameter_list()?;
            Some(generics)
        } else {
            None
        };

        self.skip_newlines();

        let parameters = self.parse_parameter_list()?;

        self.skip_newlines();

        self.expect(TokenKind::Colon)?;

        self.skip_newlines();

        let return_type = self.parse_type()?;

        let body = if self.at(&TokenKind::LeftBrace) {
            self.parse_block()
        } else if self.at(&TokenKind::Arrow) {
            Some(self.parse_function_body()?)
        } else {
            None
        };

        let mut children = Vec::new();

        if let Some(decorators) = decorators {
            children.push(decorators);
        }

        if let Some(metadata) = metadata {
            children.push(metadata);
        }

        if let Some(anchor) = anchor {
            children.push(anchor);
        }

        children.push(name);

        if let Some(generic_parameters) = generic_parameters {
            children.push(generic_parameters);
        }

        children.push(parameters);
        children.push(return_type);
        if let Some(body) = body {
            children.push(body);
        }

        let start_span = decorators
            .map(|decorators| self.node_span(decorators))
            .unwrap_or(fn_token.span());

        let end_span = body
            .map(|body_node| self.node_span(body_node))
            .unwrap_or_else(|| self.node_span(return_type));

        let span = Span::cover(start_span, end_span).unwrap_or(fn_token.span());

        Some(self.add_node(SyntaxNodeKind::FunctionItem, span, children))
    }

    pub(super) fn parse_type_alias_item(&mut self) -> Option<NodeId> {
        let type_token = self.expect(TokenKind::Type)?;

        self.skip_newlines();

        let name = self.parse_identifier()?;

        let generic_parameters = if self.at(&TokenKind::Less) {
            let generics = self.parse_generic_parameter_list()?;
            Some(generics)
        } else {
            None
        };

        self.skip_newlines();

        self.expect(TokenKind::Equal)?;

        self.skip_newlines();

        let aliased_type = self.parse_type()?;

        let mut children = vec![name];

        if let Some(generic_parameters) = generic_parameters {
            children.push(generic_parameters);
        }

        children.push(aliased_type);

        let span = Span::cover(type_token.span(), self.node_span(aliased_type))
            .unwrap_or(type_token.span());

        Some(self.add_node(SyntaxNodeKind::TypeAliasItem, span, children))
    }

    pub(super) fn parse_struct_item(&mut self, decorators: Option<NodeId>) -> Option<NodeId> {
        let struct_token = self.expect(TokenKind::Struct)?;

        self.skip_newlines();

        let name = self.parse_identifier()?;

        let generic_parameters = if self.at(&TokenKind::Less) {
            let generics = self.parse_generic_parameter_list()?;
            self.skip_newlines();
            Some(generics)
        } else {
            None
        };

        self.skip_newlines();

        let satisfies = if self.at(&TokenKind::Satisfies) {
            let satisfies = self.parse_satisfies_clause()?;
            self.skip_newlines();
            Some(satisfies)
        } else {
            None
        };

        let fields = self.parse_struct_field_list()?;

        let mut children = Vec::new();

        if let Some(decorators) = decorators {
            children.push(decorators);
        }

        children.push(name);

        if let Some(generic_parameters) = generic_parameters {
            children.push(generic_parameters);
        }

        if let Some(satisfies) = satisfies {
            children.push(satisfies);
        }

        children.push(fields);

        let start_span = decorators
            .map(|decorators| self.node_span(decorators))
            .unwrap_or(struct_token.span());
        let span = Span::cover(start_span, self.node_span(fields)).unwrap_or(struct_token.span());

        Some(self.add_node(SyntaxNodeKind::StructItem, span, children))
    }

    pub(super) fn parse_enum_item(&mut self) -> Option<NodeId> {
        let enum_token = self.expect(TokenKind::Enum)?;

        self.skip_newlines();

        let base_type = if self.at(&TokenKind::LeftParen) {
            self.bump();
            self.skip_newlines();

            let ty = self.parse_type()?;

            self.skip_newlines();
            self.expect(TokenKind::RightParen)?;
            self.skip_newlines();

            Some(ty)
        } else if self.at(&TokenKind::Less) {
            let less = self.current().clone();
            self.graph.push_diagnostic(Diagnostic::error_with_message(
                ParserDiagnosticCode::UnexpectedToken,
                "enum base type must use enum(T) syntax instead of enum<T>".to_string(),
                less.span(),
            ));
            self.bump();
            self.skip_newlines();

            let ty = self.parse_type()?;

            self.skip_newlines();
            self.expect(TokenKind::Greater)?;
            self.skip_newlines();

            Some(ty)
        } else {
            None
        };

        let metadata = if base_type.is_none() {
            self.parse_optional_keyword_metadata(false)
        } else {
            None
        };

        let name = self.parse_identifier()?;

        self.skip_newlines();

        let variants = self.parse_enum_variant_list()?;

        let mut children = Vec::new();

        if let Some(base_type) = base_type {
            children.push(base_type);
        }

        if let Some(metadata) = metadata {
            children.push(metadata);
        }

        children.push(name);
        children.push(variants);

        let span =
            Span::cover(enum_token.span(), self.node_span(variants)).unwrap_or(enum_token.span());

        Some(self.add_node(SyntaxNodeKind::EnumItem, span, children))
    }

    pub(super) fn parse_choice_item(&mut self) -> Option<NodeId> {
        let choice_token = self.expect(TokenKind::Choice)?;

        self.skip_newlines();

        let name = self.parse_identifier()?;

        let generic_parameters = if self.at(&TokenKind::Less) {
            let generics = self.parse_generic_parameter_list()?;
            self.skip_newlines();
            Some(generics)
        } else {
            None
        };

        let variants = self.parse_choice_variant_list()?;

        let mut children = vec![name];

        if let Some(generic_parameters) = generic_parameters {
            children.push(generic_parameters);
        }

        children.push(variants);

        let span = Span::cover(choice_token.span(), self.node_span(variants))
            .unwrap_or(choice_token.span());

        Some(self.add_node(SyntaxNodeKind::ChoiceItem, span, children))
    }

    pub(super) fn parse_struct_field_default(&mut self) -> Option<NodeId> {
        let equal = self.expect(TokenKind::Equal)?;

        self.skip_newlines();

        let value = self.parse_expression()?;

        let span = Span::cover(equal.span(), self.node_span(value)).unwrap_or(equal.span());

        Some(self.add_node(SyntaxNodeKind::StructFieldDefault, span, vec![value]))
    }

    pub(super) fn parse_generic_parameter(&mut self) -> Option<NodeId> {
        let name = self.parse_identifier()?;
        let name_span = self.node_span(name);

        let mut children = vec![name];
        let mut end_span = name_span;

        self.skip_newlines();

        if self.at(&TokenKind::Colon) {
            let constraint = self.parse_generic_parameter_constraint()?;
            end_span = self.node_span(constraint);
            children.push(constraint);
        }

        let span = Span::cover(name_span, end_span).unwrap_or(name_span);

        Some(self.add_node(SyntaxNodeKind::GenericParameter, span, children))
    }

    pub(super) fn parse_generic_parameter_constraint(&mut self) -> Option<NodeId> {
        let colon = self.expect(TokenKind::Colon)?;

        self.skip_newlines();

        let constraint = self.parse_type()?;

        let span = Span::cover(colon.span(), self.node_span(constraint)).unwrap_or(colon.span());

        Some(self.add_node(
            SyntaxNodeKind::GenericParameterConstraint,
            span,
            vec![constraint],
        ))
    }

    pub(super) fn parse_constraint_item(&mut self) -> Option<NodeId> {
        let constraint_token = self.expect(TokenKind::Constraint)?;

        self.skip_newlines();

        let name = self.parse_identifier()?;

        let generic_parameters = if self.at(&TokenKind::Less) {
            let generics = self.parse_generic_parameter_list()?;
            Some(generics)
        } else {
            None
        };

        self.skip_newlines();

        let members = self.parse_constraint_member_list()?;

        let mut children = vec![name];

        if let Some(generic_parameters) = generic_parameters {
            children.push(generic_parameters);
        }

        children.push(members);

        let span = Span::cover(constraint_token.span(), self.node_span(members))
            .unwrap_or(constraint_token.span());

        Some(self.add_node(SyntaxNodeKind::ConstraintItem, span, children))
    }

    pub(super) fn parse_constraint_function_signature(&mut self) -> Option<NodeId> {
        let fn_token = self.expect(TokenKind::Fn)?;

        self.skip_newlines();

        let name = self.parse_identifier()?;

        self.skip_newlines();

        let parameters = self.parse_parameter_list()?;

        self.skip_newlines();

        self.expect(TokenKind::Colon)?;

        self.skip_newlines();

        let return_type = self.parse_type()?;

        let span =
            Span::cover(fn_token.span(), self.node_span(return_type)).unwrap_or(fn_token.span());

        Some(self.add_node(
            SyntaxNodeKind::ConstraintFunctionSignature,
            span,
            vec![name, parameters, return_type],
        ))
    }

    pub(super) fn parse_satisfies_clause(&mut self) -> Option<NodeId> {
        let satisfies_token = self.expect(TokenKind::Satisfies)?;

        let mut constraints = Vec::new();

        self.skip_newlines();

        while !self.is_eof() && !self.at(&TokenKind::LeftBrace) {
            let start_position = self.position;

            if let Some(constraint) = self.parse_type() {
                constraints.push(constraint);
            }

            self.skip_newlines();

            if self.at(&TokenKind::LeftBrace) {
                break;
            }

            if self.at(&TokenKind::Comma) {
                self.bump();

                self.skip_newlines();

                if self.at(&TokenKind::LeftBrace) {
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

        let end_span = constraints
            .last()
            .map(|constraint| self.node_span(*constraint))
            .unwrap_or(satisfies_token.span());

        let span = Span::cover(satisfies_token.span(), end_span).unwrap_or(satisfies_token.span());

        Some(self.add_node(SyntaxNodeKind::SatisfiesClause, span, constraints))
    }

    pub(super) fn parse_struct_expansion(&mut self) -> Option<NodeId> {
        let spread_token = self.expect(TokenKind::DotDotDot)?;

        self.skip_newlines();

        let target = self.parse_type()?;

        let span =
            Span::cover(spread_token.span(), self.node_span(target)).unwrap_or(spread_token.span());

        Some(self.add_node(SyntaxNodeKind::StructExpansion, span, vec![target]))
    }

    pub(super) fn parse_spread_struct_literal_field(&mut self) -> Option<NodeId> {
        let spread_token = self.expect(TokenKind::DotDotDot)?;

        self.skip_newlines();

        let expression = self.parse_expression()?;

        let span = Span::cover(spread_token.span(), self.node_span(expression))
            .unwrap_or(spread_token.span());

        Some(self.add_node(
            SyntaxNodeKind::SpreadStructLiteralField,
            span,
            vec![expression],
        ))
    }
}
