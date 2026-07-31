use std::collections;

use super::*;
use std::collections::HashMap;

impl<'a> MirBuilder<'a> {
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
        let name = resolution.symbol(symbol)?.name().to_string();
        let func_type = self.type_result.layer().symbol_type(symbol)?;
        let func_id = specialized_id.unwrap_or_else(|| FunctionId::new(symbol.raw()));

        let is_async = name.starts_with("__internal_")
            || name.starts_with("__provider_")
            || syntax
                .first_child_of_kind(item, SyntaxNodeKind::KeywordMetadataList)
                .and_then(|meta| syntax.first_child_of_kind(meta, SyntaxNodeKind::KeywordMetadataFlag))
                .and_then(|flag| syntax.first_child_of_kind(flag, SyntaxNodeKind::Identifier))
                .map(|ident| self.node_text(ident))
                .map(|text| text == "async")
                .unwrap_or(false);

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

        // Return Type derived from function signature in the TypeTable
        let return_type = match self.type_result.layer().table().kind(func_type) {
            Some(TypeKind::Function(f)) => {
                self.substitute_type(f.return_type(), &type_substitutions)
            }
            _ => func_type,
        };

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

        let body = syntax.node(item)?.last_child()?;

        if syntax
            .node(body)
            .is_some_and(|body| body.kind() == SyntaxNodeKind::Block)
        {
            builder_ctx.lower_block(body);
        } else {
            let operand = builder_ctx.lower_expression(body);
            builder_ctx.terminate_block(Terminator::Return(Some(operand)));
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

        let return_type = match self.type_result.layer().table().kind(expr_ty) {
            Some(TypeKind::Function(f)) => f.return_type(),
            _ => galfus_core::TypeId::new(0),
        };

        let is_async = syntax
            .first_child_of_kind(item, SyntaxNodeKind::KeywordMetadataList)
            .and_then(|meta| syntax.first_child_of_kind(meta, SyntaxNodeKind::KeywordMetadataFlag))
            .and_then(|flag| syntax.first_child_of_kind(flag, SyntaxNodeKind::Identifier))
            .map(|ident| self.node_text(ident))
            .map(|text| text == "async")
            .unwrap_or(false);

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
            builder_ctx.terminate_block(Terminator::Return(Some(op)));
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

    fn function_name_node(&self, item: NodeId) -> Option<NodeId> {
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

    pub fn generic_parameters_for_function_item(&self, item: NodeId) -> Vec<SymbolId> {
        let syntax = self.graph.syntax();
        let Some(generic_list) =
            syntax.first_child_of_kind(item, SyntaxNodeKind::GenericParameterList)
        else {
            return Vec::new();
        };

        let Some(generic_node) = syntax.node(generic_list) else {
            return Vec::new();
        };

        generic_node
            .children()
            .iter()
            .filter_map(|parameter| {
                let identifier =
                    syntax.first_child_of_kind(*parameter, SyntaxNodeKind::Identifier)?;
                self.graph
                    .resolution()
                    .and_then(|res| res.declaration_symbol(identifier))
            })
            .collect()
    }

    pub(super) fn requires_specialization(&self, item: NodeId) -> bool {
        !self.generic_parameters_for_function_item(item).is_empty()
            || !self
                .generic_parameters_for_anchored_function_item(item)
                .is_empty()
    }

    pub fn anchored_function_specializations_for_type(
        &self,
        ty: TypeId,
        substitutions: &HashMap<SymbolId, TypeId>,
    ) -> Vec<(NodeId, Vec<TypeId>, HashMap<SymbolId, TypeId>)> {
        let ty = self.resolve_alias_type(ty);
        let Some(TypeKind::GenericInstance { base, arguments }) =
            self.type_result.layer().table().kind(ty).cloned()
        else {
            return Vec::new();
        };

        let mut functions = Vec::new();
        let Some(root) = self.graph.syntax().root() else {
            return functions;
        };
        let arguments = arguments
            .iter()
            .map(|argument| self.substitute_type(*argument, substitutions))
            .collect::<Vec<_>>();
        self.collect_anchored_function_specializations(root, base, &arguments, &mut functions);
        functions
    }

    pub fn function_symbol_for_item(&self, item: NodeId) -> Option<SymbolId> {
        let name = self.function_name_node(item)?;
        self.graph.resolution()?.declaration_symbol(name)
    }

    fn collect_anchored_function_specializations(
        &self,
        node: NodeId,
        base: TypeId,
        arguments: &[TypeId],
        functions: &mut Vec<(NodeId, Vec<TypeId>, HashMap<SymbolId, TypeId>)>,
    ) {
        let syntax = self.graph.syntax();
        let Some(syntax_node) = syntax.node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem {
            let generic_params = self.generic_parameters_for_anchored_function_item(node);
            if generic_params.len() == arguments.len()
                && !generic_params.is_empty()
                && let Some(anchor) = syntax.first_child_of_kind(node, SyntaxNodeKind::FunctionAnchor)
                && let Some(anchor_type) = syntax.first_child(anchor)
                && self.type_result.layer().node_type(anchor_type).is_some_and(|anchor_ty| {
                    matches!(
                        self.type_result.layer().table().kind(self.resolve_alias_type(anchor_ty)),
                        Some(TypeKind::GenericInstance { base: anchor_base, .. }) if *anchor_base == base
                    )
                })
            {
                let substitutions = generic_params
                    .into_iter()
                    .zip(arguments.iter().copied())
                    .collect::<HashMap<_, _>>();
                functions.push((node, arguments.to_vec(), substitutions));
            }
        }

        for &child in syntax_node.children() {
            self.collect_anchored_function_specializations(child, base, arguments, functions);
        }
    }

    fn generic_parameters_for_anchored_function_item(&self, item: NodeId) -> Vec<SymbolId> {
        let Some(symbol) = self.function_symbol_for_item(item) else {
            return Vec::new();
        };
        let Some(function_type) = self.type_result.layer().symbol_type(symbol) else {
            return Vec::new();
        };
        let mut parameters = Vec::new();
        self.collect_generic_parameters_from_type(function_type, &mut parameters);
        parameters
    }

    fn collect_generic_parameters_from_type(&self, ty: TypeId, parameters: &mut Vec<SymbolId>) {
        match self.type_result.layer().table().kind(ty) {
            Some(TypeKind::GenericParameter { symbol }) => {
                if !parameters.contains(symbol) {
                    parameters.push(*symbol);
                }
            }
            Some(TypeKind::Array { element }) | Some(TypeKind::Range { element }) => {
                self.collect_generic_parameters_from_type(*element, parameters);
            }
            Some(TypeKind::Tuple { elements }) | Some(TypeKind::Union { members: elements }) => {
                for element in elements {
                    self.collect_generic_parameters_from_type(*element, parameters);
                }
            }
            Some(TypeKind::Function(function)) => {
                for parameter in function.parameters() {
                    self.collect_generic_parameters_from_type(parameter.ty(), parameters);
                }
                self.collect_generic_parameters_from_type(function.return_type(), parameters);
            }
            Some(TypeKind::GenericInstance { base, arguments }) => {
                self.collect_generic_parameters_from_type(*base, parameters);
                for argument in arguments {
                    self.collect_generic_parameters_from_type(*argument, parameters);
                }
            }
            _ => {}
        }
    }

    pub(super) fn substitute_type(
        &self,
        ty: TypeId,
        substitutions: &collections::HashMap<SymbolId, TypeId>,
    ) -> TypeId {
        let ty = self.resolve_alias_type(ty);

        match self.type_result.layer().table().kind(ty) {
            Some(TypeKind::GenericParameter { symbol }) => {
                substitutions.get(symbol).copied().unwrap_or(ty)
            }
            _ => ty,
        }
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

    pub(super) fn get_struct_fields(&self, struct_symbol: SymbolId) -> Vec<(String, TypeId)> {
        let mut visited = collections::HashSet::new();
        self.get_struct_fields_internal(struct_symbol, &mut visited)
    }

    pub(super) fn get_struct_fields_internal(
        &self,
        struct_symbol: SymbolId,
        visited: &mut collections::HashSet<SymbolId>,
    ) -> Vec<(String, TypeId)> {
        if !visited.insert(struct_symbol) {
            return Vec::new();
        }

        let resolution = match self.graph.resolution() {
            Some(res) => res,
            None => return Vec::new(),
        };

        let struct_symbol_data = match resolution.symbol(struct_symbol) {
            Some(data) => data,
            None => return Vec::new(),
        };

        let mut fields = Vec::new();
        let root = self.graph.syntax().root().unwrap();
        if let Some(item_node) = self.find_struct_item_by_name(root, struct_symbol_data.name()) {
            let syntax = self.graph.syntax();
            let field_children = syntax
                .first_child_of_kind(item_node, SyntaxNodeKind::StructFieldList)
                .and_then(|fl| syntax.node(fl))
                .map(|n| n.children())
                .unwrap_or(&[]);

            for &field_child in field_children {
                let node_kind = syntax.node(field_child).map(|n| n.kind());
                if node_kind == Some(SyntaxNodeKind::StructExpansion) {
                    let target_sym = syntax
                        .child(field_child, 0)
                        .and_then(|target| self.type_result.layer().node_type(target))
                        .and_then(|target_ty| self.struct_symbol_for_type(target_ty));
                    if let Some(target_sym) = target_sym {
                        for (exp_name, exp_ty) in
                            self.get_struct_fields_internal(target_sym, visited)
                        {
                            if !fields.iter().any(|(n, _)| *n == exp_name) {
                                fields.push((exp_name, exp_ty));
                            }
                        }
                    }
                } else if node_kind == Some(SyntaxNodeKind::StructField)
                    && let Some(ident_node) =
                        syntax.first_child_of_kind(field_child, SyntaxNodeKind::Identifier)
                {
                    let name_str = self.node_text(ident_node).to_string();
                    let field_ty = resolution
                        .declaration_symbol(ident_node)
                        .and_then(|sym| self.type_result.layer().symbol_type(sym))
                        .or_else(|| self.type_result.layer().node_type(field_child));
                    if let Some(ty) = field_ty
                        && !fields.iter().any(|(n, _)| *n == name_str)
                    {
                        fields.push((name_str, ty));
                    }
                }
            }
        }
        fields
    }

    pub(super) fn find_struct_item_by_name(
        &self,
        node: NodeId,
        struct_name: &str,
    ) -> Option<NodeId> {
        let syntax = self.graph.syntax();
        let syntax_node = syntax.node(node)?;
        if syntax_node.kind() == SyntaxNodeKind::StructItem {
            let has_matching_identifier = syntax
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .is_some_and(|id| self.node_text(id) == struct_name);
            if has_matching_identifier {
                return Some(node);
            }
        }
        for &child in syntax_node.children() {
            if let Some(found) = self.find_struct_item_by_name(child, struct_name) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn struct_symbol_for_type(&self, ty: TypeId) -> Option<SymbolId> {
        let layer = self.type_result.layer();
        let table = layer.table();
        let mut current = ty;
        loop {
            match table.kind(current) {
                Some(TypeKind::Named { symbol }) => {
                    let resolution = self.graph.resolution()?;
                    let is_struct = resolution
                        .symbol(*symbol)
                        .is_some_and(|sd| sd.kind() == SymbolKind::Struct);
                    if is_struct {
                        return Some(*symbol);
                    }
                    break;
                }
                Some(TypeKind::GenericInstance { base, .. }) => {
                    current = *base;
                }
                _ => break,
            }
        }
        None
    }

    pub(super) fn find_struct_field_default_expr(
        &self,
        struct_symbol: SymbolId,
        field_name: &str,
    ) -> Option<NodeId> {
        let resolution = self.graph.resolution()?;
        let struct_symbol_data = resolution.symbol(struct_symbol)?;
        let root = self.graph.syntax().root().unwrap();
        let struct_item = self.find_struct_item_by_name(root, struct_symbol_data.name())?;

        let field_node = self.find_struct_field_node_by_name(struct_item, field_name)?;
        let syntax = self.graph.syntax();
        let default_node =
            self.find_descendant_of_kind(field_node, SyntaxNodeKind::StructFieldDefault)?;
        syntax.child(default_node, 0)
    }

    pub(super) fn find_struct_field_node_by_name(
        &self,
        node: NodeId,
        field_name: &str,
    ) -> Option<NodeId> {
        let syntax = self.graph.syntax();
        let syntax_node = syntax.node(node)?;
        if matches!(
            syntax_node.kind(),
            SyntaxNodeKind::StructField | SyntaxNodeKind::WeakStructField
        ) {
            let matches_name = syntax
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .is_some_and(|identifier| self.node_text(identifier) == field_name);
            if matches_name {
                return Some(node);
            }
        }
        for &child in syntax_node.children() {
            if let Some(found) = self.find_struct_field_node_by_name(child, field_name) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn find_descendant_of_kind(
        &self,
        node: NodeId,
        kind: SyntaxNodeKind,
    ) -> Option<NodeId> {
        let syntax = self.graph.syntax();
        let syntax_node = syntax.node(node)?;
        for &child in syntax_node.children() {
            if let Some(child_node) = syntax.node(child) {
                if child_node.kind() == kind {
                    return Some(child);
                }
                if let Some(found) = self.find_descendant_of_kind(child, kind) {
                    return Some(found);
                }
            }
        }
        None
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
