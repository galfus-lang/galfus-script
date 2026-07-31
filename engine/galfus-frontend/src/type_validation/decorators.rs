use super::DeclarationTypeChecker;
use crate::{FunctionParameterType, FunctionType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, TypeId};

#[derive(Debug, Clone, Copy)]
enum DecoratorArgument {
    Provided { expression: NodeId },
    Omitted { node: NodeId },
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_decorators(&mut self, node: NodeId) {
        self.check_decorators_in_node(node);
    }

    fn check_decorators_in_node(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        let kind = syntax_node.kind();
        let children = syntax_node.children().to_vec();

        for child in children.iter().copied() {
            let Some(child_node) = self.graph.syntax().node(child) else {
                continue;
            };

            if child_node.kind() != SyntaxNodeKind::DecoratorList {
                continue;
            }

            if !self.can_own_decorators(kind) {
                self.report_invalid_decorator_usage(
                    child,
                    format!("decorators are not valid on `{kind:?}`"),
                );
                continue;
            }

            if kind == SyntaxNodeKind::FunctionItem && self.is_function_stamped(node) {
                self.report_invalid_decorator_usage(
                    child,
                    "decorators are not allowed on stamped functions".to_string(),
                );
                continue;
            }

            let Some(target_type) = self.decorated_target_type(node) else {
                self.report_invalid_decorator_usage(child, "cannot infer decorated target type");
                continue;
            };

            self.check_decorator_list(child, target_type);
        }

        for child in children {
            let Some(child_node) = self.graph.syntax().node(child) else {
                continue;
            };

            if child_node.kind() == SyntaxNodeKind::DecoratorList {
                continue;
            }

            self.check_decorators_in_node(child);
        }
    }

    fn can_own_decorators(&self, kind: SyntaxNodeKind) -> bool {
        matches!(
            kind,
            SyntaxNodeKind::FunctionItem
                | SyntaxNodeKind::StructItem
                | SyntaxNodeKind::StructField
                | SyntaxNodeKind::WeakStructField
                | SyntaxNodeKind::Parameter
                | SyntaxNodeKind::RestParameter
                | SyntaxNodeKind::ChoicePayloadItem
        )
    }

    fn decorated_target_type(&self, node: NodeId) -> Option<TypeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::FunctionItem => self
                .direct_identifier_symbol(node, SymbolKind::Function)
                .and_then(|symbol| self.layer.symbol_type(symbol)),

            SyntaxNodeKind::StructItem => self
                .direct_identifier_symbol(node, SymbolKind::Struct)
                .and_then(|symbol| self.layer.symbol_type(symbol)),

            SyntaxNodeKind::Parameter => self
                .direct_identifier_symbol(node, SymbolKind::Parameter)
                .and_then(|symbol| self.layer.symbol_type(symbol))
                .or_else(|| self.type_child_type(node)),

            SyntaxNodeKind::RestParameter => self
                .direct_identifier_symbol(node, SymbolKind::RestParameter)
                .and_then(|symbol| self.layer.symbol_type(symbol))
                .or_else(|| self.type_child_type(node)),

            SyntaxNodeKind::StructField | SyntaxNodeKind::WeakStructField => self
                .direct_identifier_symbol(node, SymbolKind::StructField)
                .and_then(|symbol| self.layer.symbol_type(symbol))
                .or_else(|| self.type_child_type(node)),

            SyntaxNodeKind::ChoicePayloadItem => self.type_child_type(node),

            _ => None,
        }
    }

    fn type_child_type(&self, node: NodeId) -> Option<TypeId> {
        let type_node = self.first_type_child(node)?;
        self.layer.node_type(type_node)
    }

    fn check_decorator_list(&mut self, list: NodeId, target_type: TypeId) {
        let decorators = self
            .graph
            .syntax()
            .node(list)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        // Source order:
        // @outer
        // @inner
        //
        // Semantic application order: inner -> outer.
        for decorator in decorators.into_iter().rev() {
            self.check_decorator_transformer(decorator, target_type);
        }
    }

    fn check_decorator_transformer(&mut self, decorator: NodeId, target_type: TypeId) {
        let Some((callee, argument_list)) = self.decorator_callee_and_arguments(decorator) else {
            self.report_invalid_decorator_usage(
                decorator,
                "decorator target must be a name, path, or call",
            );
            return;
        };

        let arguments = self.decorator_arguments(argument_list);

        let Some(decorator_type) = self.infer_expression_type(callee) else {
            return;
        };

        let function = match self.layer.table().kind(decorator_type).cloned() {
            Some(TypeKind::Function(function)) => function,
            Some(TypeKind::Error) => return,
            _ => {
                self.report_not_callable(callee, decorator_type);
                return;
            }
        };

        self.check_decorator_function_signature(
            decorator,
            &function,
            target_type,
            arguments.as_slice(),
        );
    }

    fn decorator_callee_and_arguments(
        &self,
        decorator: NodeId,
    ) -> Option<(NodeId, Option<NodeId>)> {
        let target = self.graph.syntax().child(decorator, 0)?;
        let target_node = self.graph.syntax().node(target)?;

        match target_node.kind() {
            SyntaxNodeKind::NameExpression | SyntaxNodeKind::PathExpression => Some((target, None)),
            SyntaxNodeKind::CallExpression => {
                let callee = self.graph.syntax().child(target, 0)?;
                let arguments = self.graph.syntax().child(target, 1);

                Some((callee, arguments))
            }
            _ => None,
        }
    }

    fn check_decorator_function_signature(
        &mut self,
        decorator: NodeId,
        function: &FunctionType,
        target_type: TypeId,
        arguments: &[DecoratorArgument],
    ) {
        let parameters = function.parameters();

        let Some(target_parameter) = parameters.first() else {
            self.report_invalid_decorator_usage(
                decorator,
                "decorator function must receive the decorated target as first parameter",
            );
            return;
        };

        self.check_decorator_target_parameter_type(decorator, target_parameter.ty(), target_type);

        self.check_decorator_return_type(decorator, function.return_type(), target_type);

        self.check_decorator_argument_count(decorator, function, arguments.len());
        self.check_decorator_argument_types(arguments, function);
    }

    fn check_decorator_target_parameter_type(
        &mut self,
        decorator: NodeId,
        expected: TypeId,
        actual: TypeId,
    ) {
        if self.is_assignable(expected, actual) {
            return;
        }

        self.report_type_mismatch(decorator, expected, actual);
    }

    fn check_decorator_return_type(
        &mut self,
        decorator: NodeId,
        return_type: TypeId,
        target_type: TypeId,
    ) {
        if self.is_same_decorator_type(return_type, target_type) {
            return;
        }

        let expected = self.describe_type_for_diagnostic(target_type);
        let actual = self.describe_type_for_diagnostic(return_type);

        self.report_invalid_decorator_usage(
            decorator,
            format!(
                "decorator return type must match decorated target type; expected `{expected}`, got `{actual}`"
            ),
        );
    }

    fn is_same_decorator_type(&self, left: TypeId, right: TypeId) -> bool {
        let left = self.resolve_alias_type(left);
        let right = self.resolve_alias_type(right);

        left == right || (self.is_assignable(left, right) && self.is_assignable(right, left))
    }

    fn decorator_arguments(&self, argument_list: Option<NodeId>) -> Vec<DecoratorArgument> {
        let Some(argument_list) = argument_list else {
            return Vec::new();
        };

        let Some(arguments_node) = self.graph.syntax().node(argument_list) else {
            return Vec::new();
        };

        arguments_node
            .children()
            .iter()
            .filter_map(|argument| self.decorator_argument(*argument))
            .collect()
    }

    fn decorator_argument(&self, node: NodeId) -> Option<DecoratorArgument> {
        let syntax_node = self.graph.syntax().node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::Argument => {
                let expression = self.graph.syntax().child(node, 0)?;
                Some(DecoratorArgument::Provided { expression })
            }
            SyntaxNodeKind::OmittedArgument => Some(DecoratorArgument::Omitted { node }),
            _ => Some(DecoratorArgument::Provided { expression: node }),
        }
    }

    fn check_decorator_argument_count(
        &mut self,
        decorator: NodeId,
        function: &FunctionType,
        argument_count: usize,
    ) {
        let explicit_parameters = self.decorator_explicit_parameters(function);

        let required_count = explicit_parameters
            .iter()
            .filter(|parameter| !parameter.has_default() && !parameter.is_rest())
            .count();

        let has_rest = explicit_parameters
            .iter()
            .any(|parameter| parameter.is_rest());

        let max_count = if has_rest {
            None
        } else {
            Some(explicit_parameters.len())
        };

        let too_few = argument_count < required_count;
        let too_many = max_count.is_some_and(|max_count| argument_count > max_count);

        if !too_few && !too_many {
            return;
        }

        let expected = match max_count {
            Some(max_count) if required_count == max_count => required_count.to_string(),
            Some(max_count) => format!("{required_count}..{max_count}"),
            None => format!("{required_count}+"),
        };

        self.report_invalid_decorator_usage(
            decorator,
            format!("decorator expected {expected} explicit argument(s), got {argument_count}"),
        );
    }

    fn check_decorator_argument_types(
        &mut self,
        arguments: &[DecoratorArgument],
        function: &FunctionType,
    ) {
        for (index, argument) in arguments.iter().copied().enumerate() {
            match argument {
                DecoratorArgument::Provided { expression } => {
                    let Some(expected) = self.decorator_argument_parameter_type(function, index)
                    else {
                        continue;
                    };

                    let Some(actual) = self.infer_expression_type(expression) else {
                        continue;
                    };

                    if self.is_assignable(expected, actual) {
                        continue;
                    }

                    self.report_type_mismatch(expression, expected, actual);
                }
                DecoratorArgument::Omitted { node } => {
                    let Some(parameter) = self.decorator_argument_parameter(function, index) else {
                        continue;
                    };

                    if parameter.has_default() && !parameter.is_rest() {
                        continue;
                    }

                    self.report_invalid_decorator_usage(
                        node,
                        "decorator arguments cannot be omitted",
                    );
                }
            }
        }
    }

    fn decorator_argument_parameter_type(
        &self,
        function: &FunctionType,
        argument_index: usize,
    ) -> Option<TypeId> {
        if let Some(parameter) = self.decorator_argument_parameter(function, argument_index) {
            if parameter.is_rest() {
                return self.rest_parameter_element_type(parameter.ty());
            }

            return Some(parameter.ty());
        }

        let explicit_parameters = self.decorator_explicit_parameters(function);
        let rest = explicit_parameters
            .iter()
            .find(|parameter| parameter.is_rest())?;

        self.rest_parameter_element_type(rest.ty())
    }

    fn decorator_argument_parameter<'b>(
        &self,
        function: &'b FunctionType,
        argument_index: usize,
    ) -> Option<&'b FunctionParameterType> {
        let explicit_parameters = self.decorator_explicit_parameters(function);

        explicit_parameters.get(argument_index)
    }

    fn decorator_explicit_parameters<'b>(
        &self,
        function: &'b FunctionType,
    ) -> &'b [FunctionParameterType] {
        function.parameters().get(1..).unwrap_or(&[])
    }
}
