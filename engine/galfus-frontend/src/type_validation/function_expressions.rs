use super::DeclarationTypeChecker;
use crate::{FunctionParameterType, PrimitiveType, SyntaxNodeKind};
use galfus_core::{NodeId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_function_expression_type(&mut self, node: NodeId) -> Option<TypeId> {
        let parameters_node = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::ParameterList)?;

        let parameters = self.lower_function_parameters(parameters_node)?;
        let explicit_return_type = self
            .last_direct_type_child(node)
            .and_then(|return_type| self.layer.node_type(return_type));

        let body = self.function_body(node)?;

        let return_type = match explicit_return_type {
            Some(return_type) => {
                self.check_function_body_type(body, return_type);
                return_type
            }
            None => self.infer_function_body_type(body)?,
        };

        let return_type = if self.is_async_function(node) {
            self.async_future_type(node, return_type)
        } else {
            return_type
        };

        Some(
            self.layer
                .table_mut()
                .intern_function(parameters, return_type, false),
        )
    }

    fn lower_function_parameters(
        &mut self,
        parameters_node: NodeId,
    ) -> Option<Vec<FunctionParameterType>> {
        let parameter_nodes = self
            .graph
            .syntax()
            .node(parameters_node)?
            .children()
            .to_vec();

        let mut parameters = Vec::new();

        for parameter in parameter_nodes {
            let Some(parameter_node) = self.graph.syntax().node(parameter) else {
                continue;
            };

            match parameter_node.kind() {
                SyntaxNodeKind::Parameter | SyntaxNodeKind::RestParameter => {}
                _ => continue,
            }

            let ty = match self
                .first_type_child(parameter)
                .and_then(|type_node| self.layer.node_type(type_node))
            {
                Some(ty) => ty,
                None => {
                    self.report_cannot_infer_type(
                        parameter,
                        "function expression parameter requires an explicit type",
                    );

                    self.layer.table_mut().error()
                }
            };

            let has_default = self
                .graph
                .syntax()
                .first_child_of_kind(parameter, SyntaxNodeKind::ParameterDefault)
                .is_some();

            if parameter_node.kind() == SyntaxNodeKind::RestParameter {
                parameters.push(FunctionParameterType::rest(ty));
            } else if has_default {
                parameters.push(FunctionParameterType::with_default(ty, None));
            } else {
                parameters.push(FunctionParameterType::new(ty));
            }
        }

        Some(parameters)
    }

    fn function_body(&self, node: NodeId) -> Option<NodeId> {
        self.graph.syntax().node(node)?.children().last().copied()
    }

    fn infer_function_body_type(&mut self, body: NodeId) -> Option<TypeId> {
        let body_node = self.graph.syntax().node(body)?;

        if body_node.kind() == SyntaxNodeKind::Block {
            return Some(self.layer.table().primitive(PrimitiveType::Null));
        }

        self.infer_expression_type(body)
    }

    fn check_function_body_type(&mut self, body: NodeId, expected: TypeId) {
        let Some(body_node) = self.graph.syntax().node(body) else {
            return;
        };

        if body_node.kind() == SyntaxNodeKind::Block {
            self.check_return_types(body, Some(expected));
            return;
        }

        let Some(actual) = self.infer_expression_type(body) else {
            return;
        };

        if self.is_assignable(expected, actual) {
            return;
        }

        self.report_type_mismatch(body, expected, actual);
    }
}
