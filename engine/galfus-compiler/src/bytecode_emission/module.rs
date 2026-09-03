use super::LowerCtx;
use galfus_bytecode::*;
use galfus_core::SymbolId;
use galfus_frontend::{ModuleGraph, SyntaxNodeKind, TypeCheckResult};
use galfus_ir::mir::MirModule;
use std::collections::HashSet;

pub fn lower_module(
    mir_module: &MirModule,
    type_result: &TypeCheckResult,
    module_graph: &ModuleGraph,
    source_text: &str,
    module_path: &str,
    string_table: &galfus_frontend::StringTable,
) -> (BytecodeModule, galfus_bytecode::graph::ExecutionMetadata) {
    let mut ctx = LowerCtx::new(
        type_result,
        module_graph,
        source_text,
        &mir_module.constant_pool,
        string_table,
        module_path,
        false,
        None,
    );

    let imported_structs = ctx
        .imported_struct_fields
        .keys()
        .copied()
        .collect::<Vec<_>>();
    for symbol in imported_structs {
        crate::bytecode_emission::types::get_or_create_struct_layout(&mut ctx, symbol);
    }

    for (i, func) in mir_module.functions.iter().enumerate() {
        ctx.function_map.insert(func.id, FuncIdx(i as u16));
        ctx.function_names.insert(func.id, func.name.clone());
        ctx.function_return_types.insert(func.id, func.return_type);
        ctx.function_param_types
            .insert(func.id, func.parameter_types.clone());
        ctx.function_is_async.insert(func.id, func.is_async);
    }

    let mut functions = Vec::new();
    let mut init_func_idx = None;
    let mut execution_metadata = galfus_bytecode::graph::ExecutionMetadata::default();

    for (i, mir_func) in mir_module.functions.iter().enumerate() {
        ctx.active_substitutions = mir_func.type_substitutions.clone();
        let is_init = mir_func.name == "__init_module";
        if is_init {
            init_func_idx = Some(FuncIdx(i as u16));
        }

        let return_ty = crate::bytecode_emission::types::lower_type(&mut ctx, mir_func.return_type);
        for &param_ty in &mir_func.parameter_types {
            crate::bytecode_emission::types::lower_type(&mut ctx, param_ty);
        }
        for local_decl in &mir_func.locals {
            crate::bytecode_emission::types::lower_type(&mut ctx, local_decl.ty);
        }
        let param_count = mir_func.parameter_types.len() as u16;
        let local_count = (mir_func.locals.len() as u16).saturating_sub(param_count);

        let mut emitter = crate::bytecode_emission::function::FnEmitter::new(
            &mut ctx,
            mir_func,
            param_count,
            local_count,
        );
        let (instructions, function_spans) = emitter.emit();
        execution_metadata.set_function_spans(FuncIdx(i as u16), function_spans);

        functions.push(BytecodeFunction {
            name: mir_func.name.clone(),
            param_count: param_count.try_into().unwrap(),
            local_count,
            temp_count: emitter.temp_count_max,
            return_ty,
            adapter_proxy_metadata: None,
            instructions,
        });
    }

    let export_symbols = collect_exports(module_graph);
    let mut exports = Vec::new();
    for mir_func in &mir_module.functions {
        if mir_func.name == "__init_module" {
            continue;
        }
        let sym = SymbolId::new(mir_func.id.raw());
        if export_symbols.contains(&sym) {
            let func_idx = ctx.function_map[&mir_func.id];
            exports.push(ExportSlot {
                symbol_name: mir_func.name.clone(),
                kind: ExportKind::Function(func_idx),
            });
        }
    }

    let module = BytecodeModule {
        name: module_path.to_string(),
        global_count: 0,
        constants: ctx.constant_pool,
        functions,
        types: ctx.types,
        struct_layouts: ctx.struct_layouts,
        choice_layouts: ctx.choice_layouts,
        imports: Vec::new(),
        exports,
        init_func_idx,
    };

    (module, execution_metadata)
}

fn collect_exports(graph: &ModuleGraph) -> HashSet<SymbolId> {
    let mut exports = HashSet::new();
    let syntax = graph.syntax();
    let resolution = match graph.resolution() {
        Some(res) => res,
        None => return exports,
    };
    if let Some(root_node) = syntax.root().and_then(|root| syntax.node(root)) {
        for &item in root_node.children() {
            if let Some(node) = syntax.node(item)
                && node.kind() == SyntaxNodeKind::ExportItem
                && let Some(inner) = node.first_child()
                && let Some(inner_node) = syntax.node(inner)
            {
                let ident_node = if inner_node.kind() == SyntaxNodeKind::FunctionItem {
                    syntax.first_child_of_kind(inner, SyntaxNodeKind::Identifier)
                } else {
                    Some(inner)
                };
                if let Some(ident) = ident_node
                    && let Some(sym) = resolution.declaration_symbol(ident)
                {
                    exports.insert(sym);
                }
            }
        }
    }
    exports
}
