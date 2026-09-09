use super::function::FunctionBuilder;
use galfus_core::{FunctionId, NodeId, SymbolId, TypeId};
use galfus_frontend::{SyntaxNodeKind, TypeKind};
use std::collections::HashMap;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn specialize_generic_call(
        &mut self,
        symbol: SymbolId,
        target_node: NodeId,
        arg_types: &[TypeId],
    ) -> Option<FunctionId> {
        let original_id = FunctionId::new(symbol.raw());
        let function_item = self.builder.function_item_for_symbol(symbol);

        if let Some(function_item) = function_item {
            let generic_params = self
                .builder
                .generic_parameters_for_function_item(function_item);

            if generic_params.is_empty() {
                return None;
            }

            let concrete_types =
                self.concrete_generic_arguments(target_node, &generic_params, arg_types)?;
            if concrete_types.len() != generic_params.len() {
                return None;
            }

            let key = (original_id, concrete_types.clone());
            if let Some(func_id) = self.builder.specialisations.get(&key).copied() {
                return Some(func_id);
            }
            if self.builder.active_specialisations.contains(&key) {
                return Some(original_id);
            }

            let specialized_id = self.builder.next_specialized_function_id();
            self.builder
                .specialisations
                .insert(key.clone(), specialized_id);
            self.builder.active_specialisations.insert(key.clone());

            let substitutions = generic_params
                .into_iter()
                .zip(concrete_types)
                .collect::<HashMap<_, _>>();

            let caller_next_local = self.builder.next_local_id;
            let caller_next_block = self.builder.next_block_id;
            if let Some(mut function) = self.builder.build_function_with_substitutions(
                function_item,
                Some(specialized_id),
                substitutions,
            ) {
                function.name = format!("{}#{}", function.name, specialized_id.raw());
                self.builder.specialized_functions.push(function);
            }
            self.builder.next_local_id = caller_next_local;
            self.builder.next_block_id = caller_next_block;
            self.builder.active_specialisations.remove(&key);

            Some(specialized_id)
        } else if self.builder.workspace_ctx.is_some() {
            let caller_module_id = self.builder.workspace_module_id?;
            let (target_mod_idx, target_symbol, generic_params) = {
                let ctx = self.builder.workspace_ctx.as_deref_mut()?;
                let (target_mod_idx, target_symbol) =
                    ctx.resolve_import(caller_module_id, target_node)?;
                let generic_params = ctx.get_generic_params(target_mod_idx, target_symbol)?;
                (target_mod_idx, target_symbol, generic_params)
            };
            if generic_params.is_empty() {
                return None;
            }

            let concrete_types = self
                .concrete_generic_arguments(target_node, &generic_params, arg_types)
                .or_else(|| {
                    self.builder
                        .workspace_ctx
                        .as_deref_mut()?
                        .infer_imported_generic_arguments(
                            caller_module_id,
                            target_mod_idx,
                            target_symbol,
                            &generic_params,
                            arg_types,
                        )
                })?;
            if concrete_types.len() != generic_params.len() {
                return None;
            }

            let substitutions = generic_params
                .into_iter()
                .zip(concrete_types.clone())
                .collect::<HashMap<_, _>>();

            self.builder
                .workspace_ctx
                .as_deref_mut()?
                .specialize_function(
                    caller_module_id,
                    target_node,
                    target_mod_idx,
                    target_symbol,
                    concrete_types,
                    substitutions,
                )
                .into()
        } else {
            None
        }
    }

    fn concrete_generic_arguments(
        &self,
        target_node: NodeId,
        generic_params: &[SymbolId],
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let syntax = self.builder.graph.syntax();

        if syntax
            .node(target_node)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::GenericExpression)
            && let Some(argument_list) = syntax.child(target_node, 1)
            && let Some(argument_node) = syntax.node(argument_list)
        {
            let explicit = argument_node
                .children()
                .iter()
                .filter_map(|argument| {
                    self.node_type(*argument).or_else(|| {
                        self.first_type_child(*argument)
                            .and_then(|type_node| self.node_type(type_node))
                    })
                })
                .collect::<Vec<_>>();

            if !explicit.is_empty() {
                return Some(explicit);
            }
        }

        self.infer_generic_arguments_from_call(target_node, generic_params, arg_types)
    }

    fn infer_generic_arguments_from_call(
        &self,
        target_node: NodeId,
        generic_params: &[SymbolId],
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let target_ty = self
            .builder
            .resolve_alias_type(self.node_type(target_node)?);
        let TypeKind::Function(function) =
            self.builder.type_result.layer().table().kind(target_ty)?
        else {
            return None;
        };

        let mut substitutions = HashMap::new();
        for (parameter, &arg_ty) in function.parameters().iter().zip(arg_types) {
            self.infer_generic_argument_from_types(
                generic_params,
                parameter.ty(),
                arg_ty,
                &mut substitutions,
            );
        }

        generic_params
            .iter()
            .map(|parameter| substitutions.get(parameter).copied())
            .collect()
    }

    fn infer_generic_argument_from_types(
        &self,
        generic_params: &[SymbolId],
        parameter_ty: TypeId,
        argument_ty: TypeId,
        substitutions: &mut HashMap<SymbolId, TypeId>,
    ) {
        let parameter_ty = self.builder.resolve_alias_type(parameter_ty);
        let argument_ty = self.builder.resolve_alias_type(argument_ty);

        match self.builder.type_result.layer().table().kind(parameter_ty) {
            Some(TypeKind::GenericParameter { symbol }) if generic_params.contains(symbol) => {
                substitutions.entry(*symbol).or_insert(argument_ty);
            }
            Some(TypeKind::Array { element }) => {
                if let Some(TypeKind::Array {
                    element: argument_element,
                }) = self.builder.type_result.layer().table().kind(argument_ty)
                {
                    self.infer_generic_argument_from_types(
                        generic_params,
                        *element,
                        *argument_element,
                        substitutions,
                    );
                }
            }
            Some(TypeKind::Tuple { elements }) => {
                if let Some(TypeKind::Tuple {
                    elements: argument_elements,
                }) = self.builder.type_result.layer().table().kind(argument_ty)
                {
                    for (element, argument_element) in elements.iter().zip(argument_elements) {
                        self.infer_generic_argument_from_types(
                            generic_params,
                            *element,
                            *argument_element,
                            substitutions,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
