use super::function::FunctionBuilder;
use galfus_core::{FunctionId, NodeId, SymbolId, TypeId};
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
}
