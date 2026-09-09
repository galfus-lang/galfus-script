use std::collections;

use super::resolve::{ModuleIndex, resolve_import_target};
use crate::CompilerState;
use crate::input::CompiledModule;
use crate::semantic_to_mir::WorkspaceContext;
use galfus_core::{FunctionId, NodeId, SymbolId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};
use std::collections::HashMap;

pub(super) struct MyWorkspaceContext<'a, 'index> {
    pub(super) modules: &'a mut [CompiledModule],
    pub(super) state: &'a mut CompilerState,
    pub(super) string_table: &'a galfus_frontend::StringTable,
    module_index: &'index ModuleIndex,
}

impl<'a, 'index> MyWorkspaceContext<'a, 'index> {
    pub(super) fn new(
        modules: &'a mut [CompiledModule],
        state: &'a mut CompilerState,
        string_table: &'a galfus_frontend::StringTable,
        module_index: &'index ModuleIndex,
    ) -> Self {
        Self {
            modules,
            state,
            string_table,
            module_index,
        }
    }

    pub(super) fn modules(&self) -> &[CompiledModule] {
        self.modules
    }
}

impl<'a, 'index> WorkspaceContext for MyWorkspaceContext<'a, 'index> {
    fn string_table(&self) -> &galfus_frontend::StringTable {
        self.string_table
    }
    fn resolve_import(
        &self,
        caller_module_id: galfus_core::ModuleId,
        node_id: NodeId,
    ) -> Option<(usize, SymbolId)> {
        let current_mod_idx = self.module_index.by_id(caller_module_id)?;

        let mut real_target = node_id;
        let module = &self.modules[current_mod_idx];
        let syntax = module.graph().syntax();
        while let Some(node) = syntax.node(real_target)
            && node.kind() == SyntaxNodeKind::GenericExpression
        {
            if let Some(inner) = node.first_child() {
                real_target = inner;
            } else {
                break;
            }
        }

        let func_id = FunctionId::new(0x8000_0000 | real_target.raw());
        let (target_module_id, target_func_id) =
            resolve_import_target(self.modules, self.module_index, current_mod_idx, func_id)?;
        let target_mod_idx = self.module_index.by_id(target_module_id)?;
        let target_symbol = SymbolId::new(target_func_id.raw());
        Some((target_mod_idx, target_symbol))
    }

    fn get_generic_params(
        &self,
        target_mod_idx: usize,
        target_symbol: SymbolId,
    ) -> Option<Vec<SymbolId>> {
        let target_module = &self.modules[target_mod_idx];
        let type_res = target_module.type_result().unwrap();
        let builder = crate::semantic_to_mir::MirBuilder::new(
            target_module.graph(),
            type_res,
            target_module.source().text(),
            self.string_table,
        );
        let function_item = builder.function_item_for_symbol(target_symbol)?;
        Some(builder.generic_parameters_for_function_item(function_item))
    }

    fn infer_imported_generic_arguments(
        &mut self,
        caller_module_id: galfus_core::ModuleId,
        target_mod_idx: usize,
        target_symbol: SymbolId,
        generic_params: &[SymbolId],
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let caller_mod_idx = self
            .modules
            .iter()
            .position(|m| m.id() == caller_module_id)?;
        let translated_arguments = arg_types
            .iter()
            .map(|&ty| self.translate_type(caller_mod_idx, target_mod_idx, ty))
            .collect::<Vec<_>>();
        let target_module = &self.modules[target_mod_idx];
        let function_type = target_module
            .type_result()?
            .layer()
            .symbol_type(target_symbol)?;
        let TypeKind::Function(function) = target_module
            .type_result()?
            .layer()
            .table()
            .kind(function_type)?
        else {
            return None;
        };

        let mut substitutions = HashMap::new();
        for (parameter, argument) in function.parameters().iter().zip(translated_arguments) {
            self.infer_generic_argument_from_types(
                target_mod_idx,
                generic_params,
                parameter.ty(),
                argument,
                &mut substitutions,
            );
        }

        generic_params
            .iter()
            .map(|parameter| substitutions.get(parameter).copied())
            .collect()
    }

    fn specialize_function(
        &mut self,
        caller_module_id: galfus_core::ModuleId,
        _caller_node_id: NodeId,
        target_mod_idx: usize,
        target_symbol: SymbolId,
        concrete_types: Vec<TypeId>,
        substitutions: collections::HashMap<SymbolId, TypeId>,
    ) -> FunctionId {
        let caller_mod_idx = self
            .module_index
            .by_id(caller_module_id)
            .expect("generic specialization caller module must be present in the workspace");

        let concrete_types = concrete_types
            .iter()
            .map(|&ty| self.translate_type(caller_mod_idx, target_mod_idx, ty))
            .collect::<Vec<_>>();

        let substitutions = substitutions
            .into_iter()
            .map(|(sym, ty)| {
                let translated_ty = self.translate_type(caller_mod_idx, target_mod_idx, ty);
                (sym, translated_ty)
            })
            .collect::<HashMap<_, _>>();

        let target_module_id = self.modules[target_mod_idx].id();
        let key = (target_module_id, target_symbol, concrete_types.clone());
        if let Some(func_id) = self.state.specialisations.get(&key).copied() {
            return func_id;
        }

        let specialized_id = FunctionId::new(self.state.next_specialised_id);
        self.state.next_specialised_id = self.state.next_specialised_id.saturating_sub(1);
        self.state.specialisations.insert(key, specialized_id);
        self.state
            .specialised_id_to_target
            .insert(specialized_id, (target_module_id, specialized_id));

        // The builder can recursively ask this context to specialize another
        // function. Snapshot the target inputs so the nested mutable context
        // borrow never aliases `self.modules`.
        let (target_module_id, graph, type_res, source_text) = {
            let target_module = &self.modules[target_mod_idx];
            (
                target_module.id(),
                target_module.graph().clone(),
                target_module.type_result().cloned().unwrap(),
                target_module.source().text().to_owned(),
            )
        };
        let mut builder = crate::semantic_to_mir::MirBuilder::new(
            &graph,
            &type_res,
            &source_text,
            self.string_table,
        )
        .with_workspace_module_id(target_module_id);
        builder = builder.with_workspace_ctx(self);

        if let Some(function_item) = builder.function_item_for_symbol(target_symbol)
            && let Some(mut function) = builder.build_function_with_substitutions(
                function_item,
                Some(specialized_id),
                substitutions,
            )
        {
            function.name = format!("{}#{}", function.name, specialized_id.raw());
            let anchored_specializations = builder.anchored_function_specializations_for_type(
                function.return_type,
                &function.type_substitutions,
            );
            self.state
                .specialised_functions
                .entry(target_module_id)
                .or_default()
                .push(function);
            self.state.mark_specialised_module(target_module_id);

            for (method_item, method_types, method_substitutions) in anchored_specializations {
                self.specialize_anchored_function(
                    target_mod_idx,
                    method_item,
                    method_types,
                    method_substitutions,
                );
            }
        }

        specialized_id
    }

    fn specialize_builtin_function(
        &mut self,
        caller_module_id: galfus_core::ModuleId,
        caller_node_id: NodeId,
        module_name: &str,
        function_name: &str,
        concrete_types: Vec<TypeId>,
    ) -> Option<FunctionId> {
        let target_mod_idx = self
            .module_index
            .module_path_index(module_name)
            .or_else(|| {
                self.module_index
                    .module_path_index(&format!("{module_name}.gfs"))
            })?;
        let resolution = self.modules[target_mod_idx].graph().resolution()?;
        let target_symbol = resolution
            .export_by_name(function_name)
            .and_then(|id| resolution.export_record(id))
            .filter(|export| export.kind() == SymbolKind::Function)
            .map(|export| export.symbol())?;
        let generic_params = self.get_generic_params(target_mod_idx, target_symbol)?;
        if generic_params.len() != concrete_types.len() {
            return None;
        }
        let substitutions = generic_params
            .into_iter()
            .zip(concrete_types.clone())
            .collect();
        Some(self.specialize_function(
            caller_module_id,
            caller_node_id,
            target_mod_idx,
            target_symbol,
            concrete_types,
            substitutions,
        ))
    }

    fn function_return_type(&self, func_id: FunctionId) -> Option<TypeId> {
        self.state
            .specialised_functions
            .values()
            .flat_map(|funcs| funcs.iter())
            .find(|f| f.id == func_id)
            .map(|f| f.return_type)
    }
}

impl<'a, 'index> MyWorkspaceContext<'a, 'index> {
    fn specialize_anchored_function(
        &mut self,
        target_mod_idx: usize,
        function_item: NodeId,
        concrete_types: Vec<TypeId>,
        substitutions: HashMap<SymbolId, TypeId>,
    ) {
        let (target_module_id, graph, type_res, source_text) = {
            let target_module = &self.modules[target_mod_idx];
            (
                target_module.id(),
                target_module.graph().clone(),
                target_module.type_result().cloned().unwrap(),
                target_module.source().text().to_owned(),
            )
        };
        let mut builder = crate::semantic_to_mir::MirBuilder::new(
            &graph,
            &type_res,
            &source_text,
            self.string_table,
        )
        .with_workspace_module_id(target_module_id);
        let Some(target_symbol) = builder.function_symbol_for_item(function_item) else {
            return;
        };
        let key = (target_module_id, target_symbol, concrete_types);
        if self.state.specialisations.contains_key(&key) {
            return;
        }

        let specialized_id = FunctionId::new(self.state.next_specialised_id);
        self.state.next_specialised_id = self.state.next_specialised_id.saturating_sub(1);
        self.state.specialisations.insert(key, specialized_id);
        self.state
            .specialised_id_to_target
            .insert(specialized_id, (target_module_id, specialized_id));

        builder = builder.with_workspace_ctx(self);
        if let Some(mut function) = builder.build_function_with_substitutions(
            function_item,
            Some(specialized_id),
            substitutions,
        ) {
            function.name = format!("{}#{}", function.name, specialized_id.raw());
            self.state
                .specialised_functions
                .entry(target_module_id)
                .or_default()
                .push(function);
            self.state.mark_specialised_module(target_module_id);
        }
    }

    fn infer_generic_argument_from_types(
        &self,
        module_idx: usize,
        generic_params: &[SymbolId],
        parameter_type: TypeId,
        argument_type: TypeId,
        substitutions: &mut HashMap<SymbolId, TypeId>,
    ) {
        let table = self.modules[module_idx]
            .type_result()
            .unwrap()
            .layer()
            .table();

        match table.kind(parameter_type) {
            Some(TypeKind::GenericParameter { symbol }) if generic_params.contains(symbol) => {
                substitutions.entry(*symbol).or_insert(argument_type);
            }
            Some(TypeKind::Array { element }) => {
                if let Some(TypeKind::Array {
                    element: argument_element,
                }) = table.kind(argument_type)
                {
                    self.infer_generic_argument_from_types(
                        module_idx,
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
                }) = table.kind(argument_type)
                {
                    for (element, argument_element) in elements.iter().zip(argument_elements) {
                        self.infer_generic_argument_from_types(
                            module_idx,
                            generic_params,
                            *element,
                            *argument_element,
                            substitutions,
                        );
                    }
                }
            }
            Some(TypeKind::GenericInstance { arguments, .. }) => {
                if let Some(TypeKind::GenericInstance {
                    arguments: argument_arguments,
                    ..
                }) = table.kind(argument_type)
                {
                    for (argument, parameter) in argument_arguments.iter().zip(arguments) {
                        self.infer_generic_argument_from_types(
                            module_idx,
                            generic_params,
                            *parameter,
                            *argument,
                            substitutions,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
