use super::DeclarationTypeChecker;
use crate::{PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, TypeId};
use std::collections::HashMap;

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_return_types(&mut self, node: NodeId, current_return_type: Option<TypeId>) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::FunctionItem => {
                let function_return_type = self
                    .last_direct_type_child(node)
                    .and_then(|return_type| self.layer.node_type(return_type));

                for child in syntax_node.children() {
                    self.check_return_types(*child, function_return_type);
                }

                return;
            }

            kind if kind.is_function_expression() => {
                return;
            }

            SyntaxNodeKind::ReturnStatement => {
                self.check_return_statement_type(node, current_return_type);
            }

            _ => {}
        }

        for child in syntax_node.children() {
            self.check_return_types(*child, current_return_type);
        }
    }

    fn check_return_statement_type(&mut self, return_statement: NodeId, expected: Option<TypeId>) {
        let Some(expected) = expected else {
            return;
        };

        let expression = self.graph.syntax().child(return_statement, 0);
        let actual = match expression {
            Some(expression) => {
                match self.infer_expression_type_with_expected(expression, Some(expected)) {
                    Some(actual) => actual,
                    None => return,
                }
            }

            None => self.layer.table().primitive(PrimitiveType::Null),
        };

        if let Some(expression) = expression
            && self.is_unspecialized_choice_instance(expected, actual)
        {
            self.layer.bind_node_type(expression, expected);
            return;
        }

        if self.is_assignable(expected, actual) {
            return;
        }

        if self.value_satisfies_return_constraint(expected, actual) {
            return;
        }

        let diagnostic_node = self
            .graph
            .syntax()
            .child(return_statement, 0)
            .unwrap_or(return_statement);

        self.report_type_mismatch(diagnostic_node, expected, actual);
    }

    fn is_unspecialized_choice_instance(&self, expected: TypeId, actual: TypeId) -> bool {
        let Some(TypeKind::GenericInstance { base, .. }) = self.layer.table().kind(expected) else {
            return false;
        };
        let Some(TypeKind::Named {
            symbol: expected_symbol,
        }) = self.layer.table().kind(*base)
        else {
            return false;
        };
        let Some(TypeKind::Named {
            symbol: actual_symbol,
        }) = self.layer.table().kind(actual)
        else {
            return false;
        };

        expected_symbol == actual_symbol
            && (self.imported_symbol_choices.contains_key(actual_symbol)
                || self
                    .graph
                    .resolution()
                    .and_then(|resolution| resolution.symbol(*actual_symbol))
                    .is_some_and(|symbol| symbol.kind() == SymbolKind::Choice))
    }

    fn value_satisfies_return_constraint(&mut self, expected: TypeId, actual: TypeId) -> bool {
        let expected = self.resolve_alias_type(expected);
        let Some(TypeKind::GenericInstance { base, arguments }) =
            self.layer.table().kind(expected).cloned()
        else {
            return false;
        };
        let Some(TypeKind::Named { symbol }) = self.layer.table().kind(base) else {
            return false;
        };
        let Some(constraint_name) = self.symbol_name(*symbol) else {
            return false;
        };
        let actual = self.resolve_alias_type(actual);
        let (struct_symbol, struct_substitution) = match self.layer.table().kind(actual).cloned() {
            Some(TypeKind::Named { symbol }) => (symbol, HashMap::new()),
            Some(TypeKind::GenericInstance { base, arguments }) => {
                let Some(TypeKind::Named { symbol }) = self.layer.table().kind(base) else {
                    return false;
                };
                let Some(struct_item) = self.type_item_for_symbol(*symbol) else {
                    return false;
                };
                let parameters =
                    self.declaration_symbols_in_node(struct_item, &[SymbolKind::GenericParameter]);

                if parameters.len() != arguments.len() {
                    return false;
                }

                (*symbol, parameters.into_iter().zip(arguments).collect())
            }
            _ => return false,
        };
        let Some(struct_item) = self.type_item_for_symbol(struct_symbol) else {
            return false;
        };
        let Some(satisfies) = self
            .graph
            .syntax()
            .first_child_of_kind(struct_item, SyntaxNodeKind::SatisfiesClause)
        else {
            return false;
        };

        self.graph
            .syntax()
            .node(satisfies)
            .map(|node| node.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .any(|constraint| {
                let Some(base) = self.constraint_application_base_node(constraint) else {
                    return false;
                };
                if self.constraint_base_name(base).as_deref() != Some(constraint_name.as_str()) {
                    return false;
                }

                let actual_arguments = self
                    .constraint_application_argument_types(constraint)
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, &struct_substitution))
                    .collect::<Vec<_>>();

                arguments.len() == actual_arguments.len()
                    && arguments
                        .iter()
                        .zip(actual_arguments)
                        .all(|(expected, actual)| self.is_assignable(*expected, actual))
            })
    }
}
