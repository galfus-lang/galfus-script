use std::collections;

use super::DeclarationTypeChecker;
use crate::{FunctionType, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, SymbolId, TypeId};

#[derive(Debug, Clone, Copy)]
enum CallArgument {
    Provided { expression: NodeId },
    Omitted { node: NodeId },
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_call_expression_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let target = self.graph.syntax().child(node, 0)?;

        if self.is_choice_variant_call_target(target) {
            return self.infer_choice_variant_call_type(node, expected);
        }

        let arguments = self.graph.syntax().child(node, 1)?;
        let target_type = self.infer_expression_type(target)?;

        let function = match self.layer.table().kind(target_type).cloned() {
            Some(TypeKind::Function(function)) => function,
            Some(TypeKind::Error) => return Some(self.layer.table_mut().error()),
            _ => {
                self.report_not_callable(target, target_type);
                return Some(self.layer.table_mut().error());
            }
        };

        let generic_params = self.generic_parameter_symbols_from_type(target_type);
        let substituted_function = if !generic_params.is_empty() {
            let mut substitutions = collections::HashMap::new();

            if let Some(expected_ty) = expected {
                self.infer_substitutions_from_types(
                    &generic_params,
                    function.return_type(),
                    expected_ty,
                    &mut substitutions,
                );
            }

            let call_arguments = self.call_arguments(arguments);
            let parameters = function.parameters();
            for (i, &arg) in call_arguments.iter().enumerate() {
                if let CallArgument::Provided { expression } = arg
                    && let Some(param) = parameters.get(i)
                {
                    let expected_param_ty = param.ty();
                    let contextual_param_ty =
                        self.substitute_generic_expression_type(expected_param_ty, &substitutions);

                    if let Some(actual_arg_ty) = self
                        .infer_expression_type_with_expected(expression, Some(contextual_param_ty))
                    {
                        self.infer_substitutions_from_types(
                            &generic_params,
                            expected_param_ty,
                            actual_arg_ty,
                            &mut substitutions,
                        );
                    }
                }
            }

            for &param in &generic_params {
                if let collections::hash_map::Entry::Vacant(e) = substitutions.entry(param) {
                    let param_name = self
                        .graph
                        .resolution()
                        .and_then(|res| res.symbol(param))
                        .map(|s| s.name().to_string())
                        .unwrap_or_else(|| "T".to_string());
                    self.report_cannot_infer_type(
                        node,
                        format!("cannot infer generic type `{}`", param_name),
                    );
                    e.insert(self.layer.table_mut().error());
                }
            }

            self.validate_generic_substitution_bounds(node, &substitutions);

            let substituted_type =
                self.substitute_generic_expression_type(target_type, &substitutions);
            match self.layer.table().kind(substituted_type).cloned() {
                Some(TypeKind::Function(f)) => f,
                _ => function,
            }
        } else {
            function
        };

        let call_arguments = self.call_arguments(arguments);
        self.check_call_arguments(node, &substituted_function, call_arguments.as_slice());

        Some(substituted_function.return_type())
    }

    pub(super) fn infer_substitutions_from_types(
        &self,
        generic_params: &[SymbolId],
        param_ty: TypeId,
        arg_ty: TypeId,
        substitutions: &mut collections::HashMap<SymbolId, TypeId>,
    ) {
        let param_ty = self.resolve_alias_type(param_ty);
        let arg_ty = self.resolve_alias_type(arg_ty);

        match self.layer.table().kind(param_ty) {
            Some(TypeKind::GenericParameter { symbol }) => {
                if generic_params.contains(symbol) {
                    substitutions.entry(*symbol).or_insert(arg_ty);
                }
            }
            Some(TypeKind::Array { element: p_elem }) => {
                if let Some(TypeKind::Array { element: a_elem }) = self.layer.table().kind(arg_ty) {
                    self.infer_substitutions_from_types(
                        generic_params,
                        *p_elem,
                        *a_elem,
                        substitutions,
                    );
                }
            }
            Some(TypeKind::Tuple { elements: p_elems }) => {
                if let Some(TypeKind::Tuple { elements: a_elems }) = self.layer.table().kind(arg_ty)
                {
                    for (p_el, a_el) in p_elems.iter().zip(a_elems.iter()) {
                        self.infer_substitutions_from_types(
                            generic_params,
                            *p_el,
                            *a_el,
                            substitutions,
                        );
                    }
                }
            }
            Some(TypeKind::Union { members: p_members }) => {
                for &p_member in p_members {
                    self.infer_substitutions_from_types(
                        generic_params,
                        p_member,
                        arg_ty,
                        substitutions,
                    );
                }
            }
            Some(TypeKind::Function(p_func)) => {
                if let Some(TypeKind::Function(a_func)) = self.layer.table().kind(arg_ty) {
                    for (p_param, a_param) in
                        p_func.parameters().iter().zip(a_func.parameters().iter())
                    {
                        self.infer_substitutions_from_types(
                            generic_params,
                            p_param.ty(),
                            a_param.ty(),
                            substitutions,
                        );
                    }
                    self.infer_substitutions_from_types(
                        generic_params,
                        p_func.return_type(),
                        a_func.return_type(),
                        substitutions,
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn call_argument_nodes(&self, arguments: NodeId) -> Vec<NodeId> {
        let Some(arguments_node) = self.graph.syntax().node(arguments) else {
            return Vec::new();
        };

        arguments_node
            .children()
            .iter()
            .filter_map(|child| {
                let syntax_node = self.graph.syntax().node(*child)?;

                match syntax_node.kind() {
                    SyntaxNodeKind::Argument => self.graph.syntax().child(*child, 0),
                    _ => Some(*child),
                }
            })
            .collect()
    }

    pub(super) fn rest_parameter_element_type(&self, rest_type: TypeId) -> Option<TypeId> {
        match self.layer.table().kind(rest_type) {
            Some(TypeKind::Array { element }) => Some(*element),

            Some(TypeKind::Error) => Some(rest_type),

            _ => Some(rest_type),
        }
    }

    fn call_arguments(&self, arguments: NodeId) -> Vec<CallArgument> {
        let Some(arguments_node) = self.graph.syntax().node(arguments) else {
            return Vec::new();
        };

        arguments_node
            .children()
            .iter()
            .filter_map(|child| self.call_argument(*child))
            .collect()
    }

    fn call_argument(&self, node: NodeId) -> Option<CallArgument> {
        let syntax_node = self.graph.syntax().node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::Argument => {
                let expression = self.graph.syntax().child(node, 0)?;
                Some(CallArgument::Provided { expression })
            }
            SyntaxNodeKind::OmittedArgument => Some(CallArgument::Omitted { node }),
            _ => Some(CallArgument::Provided { expression: node }),
        }
    }

    fn check_call_arguments(
        &mut self,
        call: NodeId,
        function: &FunctionType,
        arguments: &[CallArgument],
    ) {
        let parameters = function.parameters();
        let mut parameter_index = 0;

        for argument in arguments.iter().copied() {
            let Some(parameter) = parameters.get(parameter_index) else {
                self.report_call_argument_count_mismatch(call, function, arguments.len());
                return;
            };

            if parameter.is_rest() {
                self.check_rest_call_argument(argument, parameter.ty());
                continue;
            }

            match argument {
                CallArgument::Provided { expression } => {
                    self.check_single_call_argument_type(expression, parameter.ty());
                    parameter_index += 1;
                }
                CallArgument::Omitted { node } => {
                    if !parameter.has_default() {
                        self.report_omitted_required_argument(node);
                    }

                    parameter_index += 1;
                }
            }
        }

        for parameter in parameters.iter().skip(parameter_index) {
            if parameter.is_rest() {
                return;
            }

            if parameter.has_default() {
                continue;
            }

            self.report_call_argument_count_mismatch(call, function, arguments.len());
            return;
        }
    }

    fn check_rest_call_argument(&mut self, argument: CallArgument, rest_type: TypeId) {
        match argument {
            CallArgument::Provided { expression } => {
                let expected = self
                    .rest_parameter_element_type(rest_type)
                    .unwrap_or(rest_type);

                self.check_single_call_argument_type(expression, expected);
            }
            CallArgument::Omitted { node } => {
                self.report_omitted_required_argument(node);
            }
        }
    }

    fn check_single_call_argument_type(&mut self, expression: NodeId, expected: TypeId) {
        let Some(actual) = self.infer_expression_type_with_expected(expression, Some(expected))
        else {
            return;
        };

        if self.is_assignable(expected, actual) {
            return;
        }

        self.report_type_mismatch(expression, expected, actual);
    }
}
