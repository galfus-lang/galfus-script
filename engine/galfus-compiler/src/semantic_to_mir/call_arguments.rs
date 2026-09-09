use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;

pub(super) struct LoweredCallArguments {
    pub(super) arguments: Vec<Operand>,
    pub(super) argument_types: Vec<TypeId>,
    pub(super) anchored_receiver: Option<NodeId>,
}

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_call_arguments(
        &mut self,
        target_node: NodeId,
        argument_list_node: NodeId,
    ) -> LoweredCallArguments {
        let anchored_receiver = self.anchored_call_receiver(target_node);
        let expected_parameters = self.expected_call_parameters(target_node);
        let parameter_offset = usize::from(anchored_receiver.is_some());
        let mut arguments = Vec::new();
        let mut argument_types = Vec::new();

        if let Some(receiver_node) = anchored_receiver {
            let receiver = self.lower_expression(receiver_node);
            let receiver_type = self
                .node_type(receiver_node)
                .unwrap_or_else(|| TypeId::new(0));
            let receiver = if let Some(&(expected_type, _)) = expected_parameters
                .as_ref()
                .and_then(|parameters| parameters.first())
            {
                self.insert_cast_if_needed(receiver, receiver_type, expected_type)
            } else {
                receiver
            };
            arguments.push(receiver);
            argument_types.push(receiver_type);
        }

        let mut rest_arguments = Vec::new();
        let mut rest_array_type = None;
        let argument_nodes = self
            .builder
            .graph
            .syntax()
            .node(argument_list_node)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        for (index, argument_node) in argument_nodes.into_iter().enumerate() {
            let argument_expression = self.call_argument_expression(argument_node);
            let argument = self.lower_expression(argument_expression);
            let argument_type = self
                .node_type(argument_expression)
                .unwrap_or_else(|| TypeId::new(0));
            let (argument, is_rest) = self.cast_call_argument(
                argument,
                argument_type,
                expected_parameters.as_deref(),
                index + parameter_offset,
                &mut rest_array_type,
            );

            if is_rest {
                rest_arguments.push(argument);
            } else {
                arguments.push(argument);
                argument_types.push(argument_type);
            }
        }

        if rest_array_type.is_none()
            && let Some(parameters) = expected_parameters.as_ref()
            && let Some(&(expected_type, is_rest)) = parameters.last()
            && is_rest
            && arguments.len() < parameters.len()
        {
            rest_array_type = Some(expected_type);
        }

        if let Some(array_type) = rest_array_type {
            let local = self.declare_local(None, array_type);
            self.current_instructions.push((
                Instruction::Assign(local, RValue::NewArray(array_type, rest_arguments)),
                None,
            ));
            arguments.push(Operand::Local(local));
            argument_types.push(array_type);
        }

        if let Some(parameters) = expected_parameters.as_ref() {
            let provided = arguments.len() - parameter_offset;
            if provided < parameters.len() {
                for &(expected_type, is_rest) in parameters.iter().skip(provided) {
                    if !is_rest {
                        arguments.push(Operand::Constant(Constant::Null));
                        argument_types.push(expected_type);
                    }
                }
            }
        }

        LoweredCallArguments {
            arguments,
            argument_types,
            anchored_receiver,
        }
    }

    fn expected_call_parameters(&self, target_node: NodeId) -> Option<Vec<(TypeId, bool)>> {
        if self.is_choice_variant_call_target(target_node) {
            return self
                .get_choice_variant_payload(target_node)
                .map(|(_, _, payload_types)| {
                    payload_types.into_iter().map(|ty| (ty, false)).collect()
                });
        }

        let target_type = self
            .node_type(target_node)
            .map(|ty| self.builder.resolve_alias_type(ty))?;
        let TypeKind::Function(function) =
            self.builder.type_result.layer().table().kind(target_type)?
        else {
            return None;
        };
        Some(
            function
                .parameters()
                .iter()
                .map(|parameter| (parameter.ty(), parameter.is_rest()))
                .collect(),
        )
    }

    fn call_argument_expression(&self, argument_node: NodeId) -> NodeId {
        let syntax = self.builder.graph.syntax();
        syntax
            .node(argument_node)
            .and_then(|node| {
                (node.kind() == SyntaxNodeKind::Argument)
                    .then(|| syntax.child(argument_node, 0))
                    .flatten()
            })
            .unwrap_or(argument_node)
    }

    fn cast_call_argument(
        &mut self,
        argument: Operand,
        argument_type: TypeId,
        expected_parameters: Option<&[(TypeId, bool)]>,
        parameter_index: usize,
        rest_array_type: &mut Option<TypeId>,
    ) -> (Operand, bool) {
        let Some(parameters) = expected_parameters else {
            return (argument, false);
        };
        let Some(&(expected_type, is_rest)) = parameters
            .get(parameter_index)
            .or_else(|| parameters.last().filter(|(_, is_rest)| *is_rest))
        else {
            return (argument, false);
        };

        if is_rest {
            *rest_array_type = Some(expected_type);
        }
        let target_type = if is_rest {
            self.rest_element_type(expected_type)
        } else {
            expected_type
        };
        (
            self.insert_cast_if_needed(argument, argument_type, target_type),
            is_rest,
        )
    }

    fn rest_element_type(&self, array_type: TypeId) -> TypeId {
        let resolved_type = self.builder.resolve_alias_type(array_type);
        match self.builder.type_result.layer().table().kind(resolved_type) {
            Some(TypeKind::Array { element }) => *element,
            _ => array_type,
        }
    }
}
