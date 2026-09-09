use super::call_resolution::path_call_function_id;
use super::function::FunctionBuilder;
use galfus_core::{FunctionId, NodeId, SymbolId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_call_dispatch(
        &mut self,
        expression_id: NodeId,
        target_node: NodeId,
        arguments: Vec<Operand>,
        argument_types: Vec<TypeId>,
        anchored_receiver: Option<NodeId>,
    ) -> Operand {
        if self.is_choice_variant_call_target(target_node) {
            return self.lower_choice_constructor(
                expression_id,
                target_node,
                arguments,
                &argument_types,
            );
        }

        let real_target = self.unwrap_generic_call_target(target_node);
        let target_symbol = self.call_target_symbol_for_receiver(anchored_receiver, target_node);
        if self.is_constraint_call(real_target)
            || self.is_dynamic_anchored_call(anchored_receiver, target_symbol)
        {
            return self.lower_constraint_call(expression_id, real_target, arguments);
        }

        let result_type = self
            .node_type(expression_id)
            .unwrap_or_else(|| TypeId::new(0));
        let destination = self.declare_local(None, result_type);
        if self.is_indirect_call(target_symbol) {
            let function = self.lower_expression(target_node);
            let instruction = if self.is_future_type(result_type) {
                Instruction::Assign(
                    destination,
                    RValue::CreateIndirectFuture {
                        func: function,
                        args: arguments,
                    },
                )
            } else {
                Instruction::IndirectCall {
                    func: function,
                    args: arguments,
                    destination,
                }
            };
            self.current_instructions.push((instruction, None));
            return Operand::Local(destination);
        }

        let mut function = self.direct_call_function_id(
            target_node,
            real_target,
            anchored_receiver,
            target_symbol,
        );
        if let Some(symbol) = target_symbol
            && let Some(specialized) =
                self.specialize_generic_call(symbol, target_node, &argument_types)
        {
            function = specialized;
        }
        let is_external = self.is_external_function(target_symbol);
        let instruction = if self.is_future_type(result_type) {
            Instruction::Assign(
                destination,
                RValue::CreateFuture {
                    func: function,
                    args: arguments,
                    is_external,
                },
            )
        } else {
            Instruction::Call {
                func: function,
                args: arguments,
                destination,
                is_external,
            }
        };
        self.current_instructions.push((instruction, None));
        Operand::Local(destination)
    }

    fn lower_choice_constructor(
        &mut self,
        expression_id: NodeId,
        target_node: NodeId,
        arguments: Vec<Operand>,
        argument_types: &[TypeId],
    ) -> Operand {
        let Some((variant_name, owner_type, _)) = self.get_choice_variant_payload(target_node)
        else {
            return Operand::Constant(Constant::Null);
        };
        let payload = match arguments.len() {
            0 => None,
            1 => arguments.into_iter().next(),
            _ => {
                let tuple_type = self.builder.find_tuple_type(argument_types);
                let tuple = self.declare_local(None, tuple_type);
                self.current_instructions.push((
                    Instruction::Assign(tuple, RValue::NewTuple(tuple_type, arguments)),
                    None,
                ));
                Some(Operand::Local(tuple))
            }
        };
        let expression_type = self
            .builder
            .type_result
            .layer()
            .node_type(expression_id)
            .unwrap_or(owner_type);
        let destination = self.declare_local(None, expression_type);
        self.current_instructions.push((
            Instruction::Assign(
                destination,
                RValue::Choice(expression_type, variant_name, payload),
            ),
            None,
        ));
        Operand::Local(destination)
    }

    fn unwrap_generic_call_target(&self, target_node: NodeId) -> NodeId {
        let syntax = self.builder.graph.syntax();
        let mut target = target_node;
        while syntax
            .node(target)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::GenericExpression)
        {
            let Some(inner) = syntax.node(target).and_then(|node| node.first_child()) else {
                break;
            };
            target = inner;
        }
        target
    }

    fn call_target_symbol_for_receiver(
        &self,
        anchored_receiver: Option<NodeId>,
        target_node: NodeId,
    ) -> Option<SymbolId> {
        anchored_receiver
            .and_then(|receiver| self.anchored_function_symbol(receiver, target_node))
            .or_else(|| {
                anchored_receiver
                    .is_none()
                    .then(|| self.call_target_symbol(target_node))
                    .flatten()
            })
    }

    fn is_dynamic_anchored_call(
        &self,
        anchored_receiver: Option<NodeId>,
        target_symbol: Option<SymbolId>,
    ) -> bool {
        anchored_receiver.is_some_and(|receiver| {
            target_symbol.is_none() && !self.is_imported_struct_receiver(receiver)
        })
    }

    fn is_constraint_call(&self, target_node: NodeId) -> bool {
        let syntax = self.builder.graph.syntax();
        let Some(target) = syntax.node(target_node) else {
            return false;
        };
        if target.kind() != SyntaxNodeKind::PathExpression {
            return false;
        }
        let Some(receiver) = target.child(0) else {
            return false;
        };
        if !matches!(
            syntax.node(receiver).map(|node| node.kind()),
            Some(SyntaxNodeKind::NameExpression | SyntaxNodeKind::Identifier)
        ) {
            return false;
        }
        let Some(receiver_type) = self.builder.type_result.layer().node_type(receiver) else {
            return false;
        };
        let receiver_type = self.builder.resolve_alias_type(receiver_type);
        let Some(TypeKind::Named { symbol }) =
            self.builder.type_result.layer().table().kind(receiver_type)
        else {
            return false;
        };
        self.builder
            .graph
            .resolution()
            .and_then(|resolution| resolution.symbol(*symbol))
            .is_some_and(|symbol| symbol.kind() == SymbolKind::Constraint)
    }

    fn lower_constraint_call(
        &mut self,
        expression_id: NodeId,
        target_node: NodeId,
        arguments: Vec<Operand>,
    ) -> Operand {
        let syntax = self.builder.graph.syntax();
        let method_name = syntax
            .node(target_node)
            .and_then(|node| node.child(1))
            .map(|member| self.builder.node_text(member).to_owned())
            .unwrap_or_default();
        let receiver = syntax.node(target_node).and_then(|node| node.child(0));
        let object = arguments
            .first()
            .cloned()
            .or_else(|| receiver.map(|receiver| self.lower_expression(receiver)))
            .unwrap_or(Operand::Constant(Constant::Null));
        let arguments = if arguments.is_empty() {
            Vec::new()
        } else {
            arguments.into_iter().skip(1).collect()
        };
        let result_type = self
            .node_type(expression_id)
            .unwrap_or_else(|| TypeId::new(0));
        let destination = self.declare_local(None, result_type);
        self.current_instructions.push((
            Instruction::ConstraintCall {
                method_name,
                obj: object,
                args: arguments,
                destination,
                return_type: result_type,
            },
            None,
        ));
        Operand::Local(destination)
    }

    fn is_indirect_call(&self, target_symbol: Option<SymbolId>) -> bool {
        target_symbol
            .and_then(|symbol| self.builder.graph.resolution()?.symbol(symbol))
            .is_some_and(|symbol| {
                matches!(
                    symbol.kind(),
                    SymbolKind::Var
                        | SymbolKind::Const
                        | SymbolKind::Parameter
                        | SymbolKind::RestParameter
                        | SymbolKind::ForBinding
                        | SymbolKind::PatternBinding
                )
            })
    }

    fn direct_call_function_id(
        &self,
        target_node: NodeId,
        real_target: NodeId,
        anchored_receiver: Option<NodeId>,
        target_symbol: Option<SymbolId>,
    ) -> FunctionId {
        if self.is_namespace_call(real_target) {
            path_call_function_id(real_target)
        } else if anchored_receiver.is_some() {
            target_symbol
                .map(|symbol| FunctionId::new(symbol.raw()))
                .unwrap_or_else(|| path_call_function_id(real_target))
        } else {
            target_symbol
                .map(|symbol| self.function_id_for_symbol(symbol, real_target))
                .unwrap_or_else(|| FunctionId::new(target_node.raw()))
        }
    }

    fn is_external_function(&self, target_symbol: Option<SymbolId>) -> bool {
        target_symbol
            .and_then(|symbol| self.builder.type_result.layer().symbol_type(symbol))
            .and_then(
                |ty| match self.builder.type_result.layer().table().kind(ty) {
                    Some(TypeKind::Function(function)) => Some(function.is_external()),
                    _ => None,
                },
            )
            .unwrap_or(false)
    }

    fn is_namespace_call(&self, target_node: NodeId) -> bool {
        let syntax = self.builder.graph.syntax();
        let Some(target) = syntax.node(target_node) else {
            return false;
        };
        if target.kind() != SyntaxNodeKind::PathExpression {
            return false;
        }
        let Some(root) = target.first_child() else {
            return false;
        };
        self.builder
            .graph
            .resolution()
            .and_then(|resolution| resolution.reference_symbol(root))
            .and_then(|symbol| self.builder.graph.resolution()?.symbol(symbol))
            .is_some_and(|symbol| symbol.kind() == SymbolKind::ImportNamespace)
    }
}
