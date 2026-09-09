use std::collections;

use super::*;
use std::collections::HashMap;

impl<'a> MirBuilder<'a> {
    fn is_async_function_item(&self, item: NodeId) -> bool {
        let syntax = self.graph.syntax();
        let Some(metadata) = syntax.first_child_of_kind(item, SyntaxNodeKind::KeywordMetadataList)
        else {
            return false;
        };

        syntax
            .node(metadata)
            .into_iter()
            .flat_map(|metadata| metadata.children())
            .filter_map(|flag| syntax.first_child_of_kind(*flag, SyntaxNodeKind::Identifier))
            .any(|identifier| self.node_text(identifier) == "async")
    }

    pub(super) fn build_function(&mut self, item: NodeId) -> Option<MirFunction> {
        self.build_function_with_substitutions(item, None, HashMap::new())
    }

    pub fn build_function_with_substitutions(
        &mut self,
        item: NodeId,
        specialized_id: Option<FunctionId>,
        type_substitutions: HashMap<SymbolId, TypeId>,
    ) -> Option<MirFunction> {
        let syntax = self.graph.syntax();
        let resolution = self.graph.resolution()?;

        // Find the function name
        let name_node = self.function_name_node(item)?;

        // Get function symbol and type
        let symbol = resolution.declaration_symbol(name_node)?;
        let name = self
            .string_table
            .resolve(resolution.symbol(symbol)?.name())
            .unwrap_or("")
            .to_string();
        let func_type = self.type_result.layer().symbol_type(symbol)?;
        let func_id = specialized_id.unwrap_or_else(|| FunctionId::new(symbol.raw()));

        let is_async = self.is_async_function_item(item);

        // Parameters
        let mut parameter_types = Vec::new();
        let mut param_symbols = Vec::new();
        if let Some(param_list_node) = syntax
            .first_child_of_kind(item, SyntaxNodeKind::ParameterList)
            .and_then(|param_list| syntax.node(param_list))
        {
            let sig_params = match self.type_result.layer().table().kind(func_type) {
                Some(TypeKind::Function(function)) => function.parameters().to_vec(),
                _ => Vec::new(),
            };
            for (idx, param) in param_list_node.children().iter().enumerate() {
                let param_node = *param;
                let identifier_node = syntax
                    .first_child_of_kind(param_node, SyntaxNodeKind::BindingPattern)
                    .and_then(|bp| syntax.first_child_of_kind(bp, SyntaxNodeKind::Identifier))
                    .or_else(|| syntax.first_child_of_kind(param_node, SyntaxNodeKind::Identifier))
                    .unwrap_or(param_node);

                let param_symbol = resolution.declaration_symbol(identifier_node);
                let param_ty = param_symbol
                    .and_then(|sym| self.type_result.layer().symbol_type(sym))
                    .or_else(|| self.type_result.layer().node_type(identifier_node))
                    .or_else(|| self.type_result.layer().node_type(param_node))
                    .or_else(|| sig_params.get(idx).map(|param| param.ty()))
                    .unwrap_or_else(|| galfus_core::TypeId::new(0));

                let ty = self.substitute_type(param_ty, &type_substitutions);
                parameter_types.push(ty);
                param_symbols.push((param_symbol, ty, param_node));
            }
        }

        // The callable type of an async function returns Future<T>, while its body and MIR
        // function return T. Keep the payload type here so return validation and bytecode agree.
        let callable_return_type = match self.type_result.layer().table().kind(func_type) {
            Some(TypeKind::Function(f)) => {
                self.substitute_type(f.return_type(), &type_substitutions)
            }
            _ => func_type,
        };
        let inferred_return_type = if is_async {
            match self.type_result.layer().table().kind(callable_return_type) {
                Some(TypeKind::GenericInstance { arguments, .. }) => {
                    arguments.first().copied().unwrap_or(callable_return_type)
                }
                _ => callable_return_type,
            }
        } else {
            callable_return_type
        };
        let return_type = syntax
            .node(item)
            .and_then(|function| {
                function.children().iter().rev().copied().find(|child| {
                    syntax
                        .node(*child)
                        .is_some_and(|node| node.kind().is_type())
                })
            })
            .and_then(|annotation| self.type_result.layer().node_type(annotation))
            .map(|annotation| self.substitute_type(annotation, &type_substitutions))
            .unwrap_or(inferred_return_type);

        // Reset the local ID counter for this function
        self.next_local_id = 0;
        self.next_block_id = 1;

        let mut builder_ctx = function::FunctionBuilder {
            builder: self,
            locals: Vec::new(),
            symbol_to_local: collections::HashMap::new(),
            current_instructions: Vec::new(),
            blocks: vec![BasicBlock {
                parameters: Vec::new(),
                id: BlockId::new(0),
                instructions: Vec::new(),
                terminator: (Terminator::Return(None), None),
            }],
            current_block: BlockId::new(0),
            scopes: vec![Vec::new()],
            return_type,
            type_substitutions: type_substitutions.clone(),
            loop_targets: Vec::new(),
            narrowing_return_targets: Vec::new(),
        };

        // Declare parameters as locals
        let mut param_locals_to_unpack = Vec::new();
        let mut parameter_locals = Vec::new();
        for (symbol, ty, param_node) in param_symbols {
            let has_complex_pattern = syntax
                .first_child_of_kind(param_node, SyntaxNodeKind::BindingPattern)
                .is_some_and(|bp| {
                    syntax.first_child(bp).is_some_and(|c| {
                        syntax.node(c).unwrap().kind() != SyntaxNodeKind::Identifier
                    })
                });

            if has_complex_pattern {
                let local_id = builder_ctx.declare_local(None, ty);
                param_locals_to_unpack.push((local_id, param_node));
                parameter_locals.push((local_id, param_node));
            } else {
                let local_id = builder_ctx.declare_local(symbol, ty);
                parameter_locals.push((local_id, param_node));
            }
        }

        // Replace omitted arguments with their parameter defaults before the body runs.
        for (local_id, param_node) in parameter_locals {
            let Some(default) = syntax
                .first_child_of_kind(param_node, SyntaxNodeKind::ParameterDefault)
                .and_then(|default| syntax.first_child(default))
            else {
                continue;
            };
            let fallback = builder_ctx.lower_expression(default);
            builder_ctx.current_instructions.push((
                Instruction::Assign(
                    local_id,
                    RValue::BinaryOp(
                        MirBinaryOp::NullFallback,
                        Operand::Local(local_id),
                        fallback,
                    ),
                ),
                None,
            ));
        }

        // Unpack destructured parameters
        for (local_id, param_node) in param_locals_to_unpack {
            if let Some(pattern) =
                syntax.first_child_of_kind(param_node, SyntaxNodeKind::BindingPattern)
            {
                builder_ctx.lower_destructuring_binding(pattern, Operand::Local(local_id));
            }
        }

        let last_node = syntax.node(item)?.last_child()?;

        if syntax
            .node(last_node)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::Block)
        {
            builder_ctx.lower_block(last_node);
        } else if syntax
            .node(last_node)
            .is_some_and(|node| node.kind().is_expression())
        {
            let operand = builder_ctx.lower_expression(last_node);
            if !builder_ctx.is_terminated() {
                builder_ctx.close_current_block(Terminator::Return(Some(operand)));
            }
        } else {
            builder_ctx.close_current_block(Terminator::Return(None));
        }
        builder_ctx.flush_current_instructions();

        let mut func = MirFunction {
            id: func_id,
            name,
            return_type,
            parameter_types,
            locals: builder_ctx.locals,
            blocks: builder_ctx.blocks,
            type_substitutions,
            is_async,
        };
        crate::bytecode_emission::ssa::convert_to_ssa(&mut func);
        Some(func)
    }

    pub(super) fn build_function_expression(
        &mut self,
        item: NodeId,
        expr_ty: TypeId,
    ) -> Option<MirFunction> {
        let syntax = self.graph.syntax();
        let resolution = self.graph.resolution()?;

        let func_id = FunctionId::new(self.next_specialized_function_id);
        self.next_specialized_function_id -= 1;
        let name = format!("__anon_func_{}", func_id.raw());

        let mut parameter_types = Vec::new();
        let mut param_symbols = Vec::new();

        if let Some(TypeKind::Function(f)) = self.type_result.layer().table().kind(expr_ty)
            && let Some(param_list_node) = syntax
                .first_child_of_kind(item, SyntaxNodeKind::ParameterList)
                .and_then(|param_list| syntax.node(param_list))
        {
            let sig_params = f.parameters();
            for (idx, param) in param_list_node.children().iter().enumerate() {
                let param_node = *param;
                let identifier_node = syntax
                    .first_child_of_kind(param_node, SyntaxNodeKind::BindingPattern)
                    .and_then(|bp| syntax.first_child_of_kind(bp, SyntaxNodeKind::Identifier))
                    .or_else(|| syntax.first_child_of_kind(param_node, SyntaxNodeKind::Identifier))
                    .unwrap_or(param_node);

                let param_symbol = resolution.declaration_symbol(identifier_node);
                let ty = sig_params
                    .get(idx)
                    .map(|p| p.ty())
                    .unwrap_or_else(|| galfus_core::TypeId::new(0));

                parameter_types.push(ty);
                param_symbols.push((param_symbol, ty));
            }
        }

        let callable_return_type = match self.type_result.layer().table().kind(expr_ty) {
            Some(TypeKind::Function(f)) => f.return_type(),
            _ => galfus_core::TypeId::new(0),
        };

        let is_async = self.is_async_function_item(item);

        let return_type = if is_async {
            match self.type_result.layer().table().kind(callable_return_type) {
                Some(TypeKind::GenericInstance { arguments, .. }) => {
                    arguments.first().copied().unwrap_or(callable_return_type)
                }
                _ => callable_return_type,
            }
        } else {
            callable_return_type
        };

        self.next_local_id = 0;
        self.next_block_id = 1;

        let mut builder_ctx = function::FunctionBuilder {
            builder: self,
            locals: Vec::new(),
            symbol_to_local: collections::HashMap::new(),
            current_instructions: Vec::new(),
            blocks: vec![BasicBlock {
                parameters: Vec::new(),
                id: BlockId::new(0),
                instructions: Vec::new(),
                terminator: (Terminator::Return(None), None),
            }],
            current_block: BlockId::new(0),
            scopes: vec![Vec::new()],
            return_type,
            type_substitutions: collections::HashMap::new(),
            loop_targets: Vec::new(),
            narrowing_return_targets: Vec::new(),
        };

        for (sym, ty) in param_symbols {
            if let Some(s) = sym {
                let local_id = builder_ctx.declare_local(Some(s), ty);
                builder_ctx.symbol_to_local.insert(s, local_id);
            } else {
                builder_ctx.declare_local(None, ty);
            }
        }

        let body = syntax.node(item)?.last_child()?;
        let body_kind = syntax.node(body)?.kind();

        if body_kind == SyntaxNodeKind::Block {
            builder_ctx.lower_block(body);
        } else {
            let op = builder_ctx.lower_expression(body);
            if !builder_ctx.is_terminated() {
                builder_ctx.close_current_block(Terminator::Return(Some(op)));
            }
        }

        let mut func = MirFunction {
            id: func_id,
            name,
            return_type,
            parameter_types,
            locals: builder_ctx.locals,
            blocks: builder_ctx.blocks,
            type_substitutions: collections::HashMap::new(),
            is_async,
        };
        crate::bytecode_emission::ssa::convert_to_ssa(&mut func);
        Some(func)
    }

    pub fn function_item_for_symbol(&self, symbol: SymbolId) -> Option<NodeId> {
        let root = self.graph.syntax().root()?;
        self.find_function_item_for_symbol(root, symbol)
    }

    fn find_function_item_for_symbol(&self, node: NodeId, symbol: SymbolId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem
            && self.function_name_node(node).and_then(|name| {
                self.graph
                    .resolution()
                    .and_then(|res| res.declaration_symbol(name))
            }) == Some(symbol)
        {
            return Some(node);
        }

        for child in syntax_node.children() {
            if let Some(found) = self.find_function_item_for_symbol(*child, symbol) {
                return Some(found);
            }
        }

        None
    }

    pub(super) fn function_name_node(&self, item: NodeId) -> Option<NodeId> {
        let resolution = self.graph.resolution()?;
        self.find_function_name_node(item, resolution)
    }

    fn find_function_name_node(
        &self,
        node: NodeId,
        resolution: &galfus_frontend::ResolutionLayer,
    ) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;
        if syntax_node.kind() == SyntaxNodeKind::Identifier
            && resolution
                .declaration_symbol(node)
                .and_then(|symbol| resolution.symbol(symbol))
                .is_some_and(|symbol| symbol.kind() == SymbolKind::Function)
        {
            return Some(node);
        }
        for child in syntax_node.children() {
            if let Some(name) = self.find_function_name_node(*child, resolution) {
                return Some(name);
            }
        }
        None
    }

    pub(super) fn next_specialized_function_id(&mut self) -> FunctionId {
        let id = self.next_specialized_function_id;
        self.next_specialized_function_id = self.next_specialized_function_id.saturating_sub(1);
        FunctionId::new(id)
    }

    pub(super) fn next_local(&mut self) -> LocalId {
        let id = self.next_local_id;
        self.next_local_id += 1;
        LocalId::new(id)
    }

    pub(super) fn next_block(&mut self) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        BlockId::new(id)
    }

    pub(super) fn node_text(&self, node: NodeId) -> &str {
        if let Some(syntax_node) = self.graph.syntax().node(node) {
            let span = syntax_node.span();
            if span.start() <= self.source_text.len() && span.end() <= self.source_text.len() {
                return &self.source_text[span.start()..span.end()];
            }
        }
        ""
    }

    pub(super) fn find_tuple_type(&self, elements: &[TypeId]) -> TypeId {
        let table = self.type_result.layer().table();
        for id in 0..table.len() {
            let ty_id = TypeId::new(id as u32);
            if matches!(table.kind(ty_id), Some(TypeKind::Tuple { elements: existing }) if existing == elements)
            {
                return ty_id;
            }
        }
        TypeId::new(0)
    }
}
