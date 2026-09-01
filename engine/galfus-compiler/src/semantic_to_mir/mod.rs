pub mod complex_literals;
pub mod expression;
pub mod function;
mod function_helpers;
pub mod helpers;
mod module_items;
pub mod pattern;

use std::collections;

use galfus_core::{FunctionId, ModuleId, NodeId, SymbolId, TypeId};
use galfus_frontend::{ModuleGraph, SymbolKind, SyntaxNodeKind, TypeCheckResult, TypeKind};
use galfus_ir::mir::*;
use std::collections::{HashMap, HashSet};

pub struct MirBuilder<'a> {
    pub(super) graph: &'a ModuleGraph,
    pub(super) type_result: &'a TypeCheckResult,
    pub(super) source_text: &'a str,
    pub(super) string_table: &'a galfus_frontend::StringTable,
    pub(super) next_local_id: u32,
    pub(super) next_block_id: u32,
    pub(super) next_specialized_function_id: u32,
    pub(super) specialisations: HashMap<(FunctionId, Vec<TypeId>), FunctionId>,
    pub(super) specialized_functions: Vec<MirFunction>,
    pub(super) active_specialisations: HashSet<(FunctionId, Vec<TypeId>)>,
    pub(super) workspace_ctx: Option<*mut (dyn WorkspaceContext + 'a)>,
    pub(super) workspace_module_id: Option<ModuleId>,
}

impl<'a> MirBuilder<'a> {
    pub fn new(
        graph: &'a ModuleGraph,
        type_result: &'a TypeCheckResult,
        source_text: &'a str,
        string_table: &'a galfus_frontend::StringTable,
    ) -> Self {
        Self {
            graph,
            type_result,
            source_text,
            string_table,
            next_local_id: 0,
            next_block_id: 0,
            next_specialized_function_id: 0x7FFF_FFFF,
            specialisations: HashMap::new(),
            specialized_functions: Vec::new(),
            active_specialisations: HashSet::new(),
            workspace_ctx: None,
            workspace_module_id: None,
        }
    }

    pub fn with_workspace_ctx(mut self, ctx: &'a mut dyn WorkspaceContext) -> Self {
        self.workspace_ctx = Some(ctx as *mut (dyn WorkspaceContext + 'a));
        self
    }

    pub fn with_workspace_module_id(mut self, module_id: ModuleId) -> Self {
        self.workspace_module_id = Some(module_id);
        self
    }

    pub fn build(mut self) -> MirModule {
        let mut functions = Vec::new();
        let mut globals = Vec::new();
        let mut global_items = Vec::new();

        if let Some(root_node) = self
            .graph
            .syntax()
            .root()
            .and_then(|root| self.graph.syntax().node(root))
        {
            for item in root_node.children() {
                if let Some(node) = self.graph.syntax().node(*item) {
                    match node.kind() {
                        SyntaxNodeKind::FunctionItem => {
                            if !self.requires_specialization(*item)
                                && let Some(func) = self.build_function(*item)
                            {
                                functions.push(func);
                            }
                        }
                        SyntaxNodeKind::ExportItem => {
                            if let Some(inner) = node.first_child() {
                                let is_func = self
                                    .graph
                                    .syntax()
                                    .node(inner)
                                    .map(|inner_node| {
                                        inner_node.kind() == SyntaxNodeKind::FunctionItem
                                    })
                                    .unwrap_or(false);
                                if is_func {
                                    if !self.requires_specialization(inner)
                                        && let Some(func) = self.build_function(inner)
                                    {
                                        functions.push(func);
                                    }
                                } else if let Some(inner_node) = self.graph.syntax().node(inner)
                                    && matches!(
                                        inner_node.kind(),
                                        SyntaxNodeKind::VarItem | SyntaxNodeKind::ConstItem
                                    )
                                {
                                    global_items.push(inner);
                                }
                            }
                        }
                        SyntaxNodeKind::VarItem | SyntaxNodeKind::ConstItem => {
                            global_items.push(*item);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Process global declarations
        for &item in &global_items {
            if let Some(binding) = self
                .graph
                .syntax()
                .first_child_of_kind(item, SyntaxNodeKind::BindingPattern)
            {
                let mut symbols = Vec::new();
                self.collect_symbols_recursive_in_builder(binding, &mut symbols);
                for symbol in symbols {
                    let ty = self
                        .type_result
                        .layer()
                        .symbol_type(symbol)
                        .unwrap_or_else(|| TypeId::new(0));
                    let name = self
                        .graph
                        .resolution()
                        .and_then(|res| res.symbol(symbol))
                        .map(|sym| {
                            self.string_table
                                .resolve(sym.name())
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();
                    globals.push(GlobalDecl { name, ty });
                }
            }
        }

        // Build synthetic init function if there are global initializers
        if let Some(init_func) = self.build_global_initializers(&global_items) {
            functions.push(init_func);
        }

        functions.append(&mut self.specialized_functions);

        MirModule {
            functions,
            globals,
            constant_pool: Vec::new(),
        }
    }

    fn collect_symbols_recursive_in_builder(&self, node_id: NodeId, symbols: &mut Vec<SymbolId>) {
        if let Some(sym) = self
            .graph
            .resolution()
            .and_then(|res| res.declaration_symbol(node_id))
        {
            symbols.push(sym);
        }
        if let Some(node) = self.graph.syntax().node(node_id) {
            for &child in node.children() {
                self.collect_symbols_recursive_in_builder(child, symbols);
            }
        }
    }

    fn build_global_initializers(&mut self, items: &[NodeId]) -> Option<MirFunction> {
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
            return_type: TypeId::new(0),
            type_substitutions: HashMap::new(),
            loop_targets: Vec::new(),
        };

        let syntax = builder_ctx.builder.graph.syntax();

        for &item in items {
            if let Some(node) = syntax.node(item)
                && matches!(
                    node.kind(),
                    SyntaxNodeKind::VarItem | SyntaxNodeKind::ConstItem
                )
                && let Some(initializer) =
                    syntax.first_child_of_kind(item, SyntaxNodeKind::Initializer)
                && let Some(expr) = syntax.first_child(initializer)
            {
                let operand = builder_ctx.lower_expression(expr);

                if let Some(binding) =
                    syntax.first_child_of_kind(item, SyntaxNodeKind::BindingPattern)
                {
                    let symbols = builder_ctx.collect_declaration_symbols(binding);
                    for symbol in symbols {
                        let name = builder_ctx
                            .builder
                            .graph
                            .resolution()
                            .and_then(|res| res.symbol(symbol))
                            .map(|sym| {
                                builder_ctx
                                    .builder
                                    .string_table
                                    .resolve(sym.name())
                                    .unwrap_or("")
                                    .to_string()
                            })
                            .unwrap_or_default();

                        builder_ctx
                            .current_instructions
                            .push((Instruction::StoreGlobal(name, operand.clone()), None));
                    }
                }
            }
        }

        builder_ctx.flush_current_instructions();

        if builder_ctx.blocks.len() == 1 && builder_ctx.blocks[0].instructions.is_empty() {
            return None;
        }

        builder_ctx.close_current_block(Terminator::Return(None));

        let mut func = MirFunction {
            id: FunctionId::new(u32::MAX),
            name: "__init_module".to_string(),
            return_type: TypeId::new(0),
            parameter_types: Vec::new(),
            locals: builder_ctx.locals,
            blocks: builder_ctx.blocks,
            type_substitutions: HashMap::new(),
            is_async: false,
        };
        crate::bytecode_emission::ssa::convert_to_ssa(&mut func);
        Some(func)
    }

    pub(super) fn is_owned_type(&self, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);
        let table = self.type_result.layer().table();

        match table.kind(ty) {
            Some(TypeKind::GenericInstance { base, .. }) => match table.kind(*base) {
                Some(TypeKind::Named { symbol }) => self
                    .graph
                    .resolution()
                    .and_then(|resolution| resolution.symbol(*symbol))
                    .is_some_and(|symbol| {
                        self.string_table.resolve(symbol.name()) == Some("Future")
                    }),
                Some(TypeKind::Path { segments, .. }) => {
                    segments.last().is_some_and(|segment| segment == "Future")
                }
                _ => false,
            },
            Some(TypeKind::Named { symbol }) => self
                .graph
                .resolution()
                .and_then(|resolution| resolution.symbol(*symbol))
                .is_some_and(|symbol| {
                    matches!(symbol.kind(), SymbolKind::Struct | SymbolKind::Choice)
                }),

            Some(TypeKind::Array { .. }) | Some(TypeKind::Tuple { .. }) => true,

            Some(TypeKind::Union { members }) => members
                .iter()
                .copied()
                .any(|member| self.is_owned_type(member)),

            _ => false,
        }
    }

    pub(super) fn resolve_alias_type(&self, ty: TypeId) -> TypeId {
        let mut visited = Vec::new();
        self.resolve_alias_type_with_visited(ty, &mut visited)
    }

    pub(super) fn resolve_alias_type_with_visited(
        &self,
        ty: TypeId,
        visited: &mut Vec<SymbolId>,
    ) -> TypeId {
        let table = self.type_result.layer().table();
        let Some(TypeKind::Named { symbol }) = table.kind(ty) else {
            return ty;
        };
        let Some(resolution) = self.graph.resolution() else {
            return ty;
        };
        let Some(symbol_data) = resolution.symbol(*symbol) else {
            return ty;
        };
        if symbol_data.kind() != SymbolKind::TypeAlias {
            return ty;
        }
        if visited.contains(symbol) {
            return ty;
        }
        visited.push(*symbol);
        let underlying_ty = self.type_result.layer().symbol_type(*symbol).unwrap_or(ty);
        self.resolve_alias_type_with_visited(underlying_ty, visited)
    }

    pub(super) fn is_same_type(&self, left: TypeId, right: TypeId) -> bool {
        self.resolve_alias_type(left) == self.resolve_alias_type(right)
    }
}

pub trait WorkspaceContext {
    fn string_table(&self) -> &galfus_frontend::StringTable;
    fn resolve_import(
        &self,
        caller_module_id: ModuleId,
        node_id: NodeId,
    ) -> Option<(usize, SymbolId)>;
    fn get_generic_params(
        &self,
        target_mod_idx: usize,
        target_symbol: SymbolId,
    ) -> Option<Vec<SymbolId>>;
    fn infer_imported_generic_arguments(
        &mut self,
        caller_module_id: ModuleId,
        target_mod_idx: usize,
        target_symbol: SymbolId,
        generic_params: &[SymbolId],
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>>;
    fn specialize_function(
        &mut self,
        caller_module_id: ModuleId,
        caller_node_id: NodeId,
        target_mod_idx: usize,
        target_symbol: SymbolId,
        concrete_types: Vec<TypeId>,
        substitutions: collections::HashMap<SymbolId, TypeId>,
    ) -> FunctionId;
    fn specialize_builtin_function(
        &mut self,
        caller_module_id: ModuleId,
        caller_node_id: NodeId,
        module_name: &str,
        function_name: &str,
        concrete_types: Vec<TypeId>,
    ) -> Option<FunctionId>;
    fn function_return_type(&self, func_id: FunctionId) -> Option<TypeId>;
}
