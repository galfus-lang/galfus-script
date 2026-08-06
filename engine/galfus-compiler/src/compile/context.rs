use std::collections;

use super::resolve::resolve_import_target;
use crate::CompilerState;
use crate::input::CompiledModule;
use crate::semantic_to_mir::WorkspaceContext;
use galfus_core::{FunctionId, NodeId, SymbolId, TypeId};
use galfus_frontend::{
    FunctionParameterType, FunctionType, SymbolKind, SyntaxNodeKind, TypeKind, TypeTable,
};
use std::collections::HashMap;

pub(super) struct MyWorkspaceContext<'a> {
    modules: &'a [CompiledModule],
    pub(super) state: &'a mut CompilerState,
    pub(super) string_table: &'a galfus_frontend::StringTable,
}

impl<'a> MyWorkspaceContext<'a> {
    pub(super) fn new(
        modules: &'a [CompiledModule],
        state: &'a mut CompilerState,
        string_table: &'a galfus_frontend::StringTable,
    ) -> Self {
        Self {
            modules,
            state,
            string_table,
        }
    }

    fn translate_symbol(
        &self,
        caller_mod_idx: usize,
        target_mod_idx: usize,
        sym: SymbolId,
    ) -> SymbolId {
        let caller_res = match self.modules[caller_mod_idx].graph().resolution() {
            Some(res) => res,
            None => return sym,
        };
        let caller_sym_data = match caller_res.symbol(sym) {
            Some(s) => s,
            None => return sym,
        };
        let sym_name_id = caller_sym_data.name();
        let sym_name = self.string_table.resolve(sym_name_id).unwrap_or("");

        let target_res = match self.modules[target_mod_idx].graph().resolution() {
            Some(res) => res,
            None => return sym,
        };

        for target_sym in target_res.symbols() {
            if self.string_table.resolve(target_sym.name()).unwrap_or("") == sym_name {
                return target_sym.id();
            }
        }

        for import in target_res.imports() {
            if import.local_name() == sym_name {
                return import.local_symbol();
            }
        }

        sym
    }

    fn translate_type(&self, caller_mod_idx: usize, target_mod_idx: usize, ty: TypeId) -> TypeId {
        let caller_module = &self.modules[caller_mod_idx];
        let caller_table = caller_module.type_result().unwrap().layer().table();

        let modules_ptr = self.modules.as_ptr() as usize as *mut CompiledModule;
        let target_module_mut = unsafe { &mut *modules_ptr.add(target_mod_idx) };
        let target_table = target_module_mut
            .type_result_mut()
            .unwrap()
            .layer_mut()
            .table_mut();

        self.translate_type_helper(
            caller_mod_idx,
            target_mod_idx,
            caller_table,
            target_table,
            ty,
        )
    }

    fn translate_type_helper(
        &self,
        caller_mod_idx: usize,
        target_mod_idx: usize,
        caller_table: &TypeTable,
        target_table: &mut TypeTable,
        ty: TypeId,
    ) -> TypeId {
        let kind = match caller_table.kind(ty) {
            Some(k) => k,
            None => return ty,
        };

        let translated_kind = match kind {
            TypeKind::Primitive(prim) => TypeKind::Primitive(*prim),
            TypeKind::Named { symbol } => {
                let target_symbol = self.translate_symbol(caller_mod_idx, target_mod_idx, *symbol);
                TypeKind::Named {
                    symbol: target_symbol,
                }
            }
            TypeKind::GenericParameter { symbol } => {
                let target_symbol = self.translate_symbol(caller_mod_idx, target_mod_idx, *symbol);
                TypeKind::GenericParameter {
                    symbol: target_symbol,
                }
            }
            TypeKind::Array { element } => {
                let target_element = self.translate_type_helper(
                    caller_mod_idx,
                    target_mod_idx,
                    caller_table,
                    target_table,
                    *element,
                );
                TypeKind::Array {
                    element: target_element,
                }
            }
            TypeKind::Range { element } => {
                let target_element = self.translate_type_helper(
                    caller_mod_idx,
                    target_mod_idx,
                    caller_table,
                    target_table,
                    *element,
                );
                TypeKind::Range {
                    element: target_element,
                }
            }
            TypeKind::Tuple { elements } => {
                let target_elements = elements
                    .iter()
                    .map(|&e| {
                        self.translate_type_helper(
                            caller_mod_idx,
                            target_mod_idx,
                            caller_table,
                            target_table,
                            e,
                        )
                    })
                    .collect::<Vec<_>>();
                TypeKind::Tuple {
                    elements: target_elements,
                }
            }
            TypeKind::Union { members } => {
                let target_members = members
                    .iter()
                    .map(|&e| {
                        self.translate_type_helper(
                            caller_mod_idx,
                            target_mod_idx,
                            caller_table,
                            target_table,
                            e,
                        )
                    })
                    .collect::<Vec<_>>();
                TypeKind::Union {
                    members: target_members,
                }
            }
            TypeKind::Function(func) => {
                let target_return_type = self.translate_type_helper(
                    caller_mod_idx,
                    target_mod_idx,
                    caller_table,
                    target_table,
                    func.return_type(),
                );
                let target_parameters = func
                    .parameters()
                    .iter()
                    .map(|param| {
                        let target_ty = self.translate_type_helper(
                            caller_mod_idx,
                            target_mod_idx,
                            caller_table,
                            target_table,
                            param.ty(),
                        );
                        if param.is_rest() {
                            FunctionParameterType::rest(target_ty)
                        } else if param.has_default() {
                            FunctionParameterType::with_default(target_ty)
                        } else {
                            FunctionParameterType::new(target_ty)
                        }
                    })
                    .collect::<Vec<_>>();
                TypeKind::Function(FunctionType::new(
                    target_parameters,
                    target_return_type,
                    func.is_external(),
                ))
            }
            TypeKind::GenericInstance { base, arguments } => {
                let target_base = self.translate_type_helper(
                    caller_mod_idx,
                    target_mod_idx,
                    caller_table,
                    target_table,
                    *base,
                );
                let target_arguments = arguments
                    .iter()
                    .map(|&arg| {
                        self.translate_type_helper(
                            caller_mod_idx,
                            target_mod_idx,
                            caller_table,
                            target_table,
                            arg,
                        )
                    })
                    .collect::<Vec<_>>();
                TypeKind::GenericInstance {
                    base: target_base,
                    arguments: target_arguments,
                }
            }
            TypeKind::Path { root, segments } => {
                let target_root = self.translate_symbol(caller_mod_idx, target_mod_idx, *root);
                TypeKind::Path {
                    root: target_root,
                    segments: segments.clone(),
                }
            }
            TypeKind::Error => TypeKind::Error,
        };
        target_table.intern(translated_kind)
    }
}

impl<'a> WorkspaceContext for MyWorkspaceContext<'a> {
    fn string_table(&self) -> &galfus_frontend::StringTable {
        self.string_table
    }
    fn resolve_import(
        &self,
        caller_module_id: galfus_core::ModuleId,
        node_id: NodeId,
    ) -> Option<(usize, SymbolId)> {
        let current_mod_idx = self
            .modules
            .iter()
            .position(|m| m.id() == caller_module_id)?;

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
            resolve_import_target(self.modules, current_mod_idx, func_id)?;
        let target_mod_idx = self
            .modules
            .iter()
            .position(|m| m.id() == target_module_id)?;
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
            .modules
            .iter()
            .position(|m| m.id() == caller_module_id)
            .unwrap_or(0);

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

        let target_module = &self.modules[target_mod_idx];
        let type_res = target_module.type_result().unwrap();
        let mut builder = crate::semantic_to_mir::MirBuilder::new(
            target_module.graph(),
            type_res,
            target_module.source().text(),
            self.string_table,
        )
        .with_workspace_module_id(target_module.id());
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
        let target_mod_idx_opt = self.modules.iter().position(|module| {
            module.path().as_str() == module_name
                || module.path().as_str() == format!("{module_name}.gfs")
        });
        let target_mod_idx = target_mod_idx_opt?;
        let resolution = self.modules[target_mod_idx].graph().resolution()?;
        let target_symbol = resolution
            .symbols()
            .iter()
            .find(|symbol| {
                symbol.kind() == SymbolKind::Function
                    && self.string_table.resolve(symbol.name()).unwrap_or("") == function_name
            })
            .map(|symbol| symbol.id())?;
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

impl<'a> MyWorkspaceContext<'a> {
    fn specialize_anchored_function(
        &mut self,
        target_mod_idx: usize,
        function_item: NodeId,
        concrete_types: Vec<TypeId>,
        substitutions: HashMap<SymbolId, TypeId>,
    ) {
        let target_module = &self.modules[target_mod_idx];
        let type_res = target_module.type_result().unwrap();
        let mut builder = crate::semantic_to_mir::MirBuilder::new(
            target_module.graph(),
            type_res,
            target_module.source().text(),
            self.string_table,
        )
        .with_workspace_module_id(target_module.id());
        let Some(target_symbol) = builder.function_symbol_for_item(function_item) else {
            return;
        };
        let target_module_id = target_module.id();
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
